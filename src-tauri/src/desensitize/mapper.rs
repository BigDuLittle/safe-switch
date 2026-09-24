//! [desensitize] 映射表 / 规则 / 日志 DAO
//!
//! 映射表是「脱敏 → 还原」双向的关键：占位符全局唯一、会话内同原文复用同一占位符
//! （保证上游上下文一致性），还原时按占位符查回原文。
//!
//! 注意：`desensitize_mapping.original` 为本地明文（当前用户权限），
//! `desensitize_hit_log.original_masked` 只存打码形态。后续可加 AES 加密列。
#![allow(
    clippy::all,
    dead_code,
    unused,
    unreachable_patterns,
    private_interfaces
)]

use crate::database::lock_conn;
use crate::database::Database;
use crate::desensitize::semantic::KeywordRule;
use crate::desensitize::{mask_text, PLACEHOLDER_PREFIX, PLACEHOLDER_SUFFIX};
use crate::error::AppError;
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

/// 会话键 → 6 位短哈希（用于占位符，不可逆）
pub fn session_key_hash(session: &str) -> String {
    let mut h = DefaultHasher::new();
    session.hash(&mut h);
    format!("{:06x}", h.finish() & 0xffffff)
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 从 settings 读占位符前缀（未配置默认 __SEC_）
fn read_placeholder_prefix(conn: &Connection) -> String {
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_placeholder_prefix'",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| crate::desensitize::PLACEHOLDER_PREFIX.to_string())
}

/// 实体类型 → 占位符语义标识（最大程度保留语义：手机号 → phone，邮箱 → email）
pub fn type_slug(entity_type: &str) -> String {
    let mut s = entity_type.trim().to_string();
    for p in ["cn_", "intl_", "intranet_", "public_", "other_"] {
        if let Some(stripped) = s.strip_prefix(p) {
            s = stripped.to_string();
            break;
        }
    }
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c.to_ascii_lowercase());
        }
    }
    // 空 / 数字开头（正则无法区分 slug 段与哈希段）→ 兜底标识
    let ok = !out.is_empty() && out.chars().next().unwrap().is_ascii_alphabetic();
    if !ok {
        return "kw".to_string();
    }
    if out.len() > 16 {
        out[..16].to_string()
    } else {
        out
    }
}

/// 生成占位符：{(_<type_slug>_<session_hash>_<seq>_]}
/// 例：{(_phone_ab12cd_0001_]}（旧格式 __SEC_<slug>_<hash>_<seq>__ / __SEC_<hash>_<seq>__ 仍可还原）
fn build_placeholder(
    conn: &Connection,
    session_key: &str,
    entity_type: &str,
) -> Result<String, AppError> {
    let seq: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(id), 0) + 1 FROM desensitize_mapping",
            [],
            |r| r.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(format!(
        "{{(_{}_{}_{:04}_]}}",
        type_slug(entity_type),
        session_key_hash(session_key),
        seq.min(9999),
    ))
}

/// 全局同原文 → 复用同一占位符（跨会话一致；映射表永久保留，仅用户手动清理时删除）。
/// 复用时不更新记录；并发下以库中实际存在的占位符为准（INSERT OR IGNORE 后再查一次）。
pub fn ensure_mapping(
    db: &Database,
    session_key: &str,
    original: &str,
    entity_type: &str,
    hit_source: &str,
    confidence: f32,
) -> Result<String, AppError> {
    if original.is_empty() {
        return Ok(String::new());
    }
    let conn = lock_conn!(db.conn);
    // 全局复用（跨会话）：同一原文值 → 同一占位符，保留语义一致性
    match conn.query_row(
        "SELECT placeholder FROM desensitize_mapping WHERE original = ?1 LIMIT 1",
        [original],
        |r| r.get::<_, String>(0).map(Some),
    ) {
        Ok(Some(ph)) => return Ok(ph),
        _ => {}
    }
    let placeholder = build_placeholder(&conn, session_key, entity_type)?;
    conn.execute(
        "INSERT OR IGNORE INTO desensitize_mapping
         (session_key, placeholder, original, entity_type, hit_source, confidence, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            session_key,
            placeholder,
            original,
            entity_type,
            hit_source,
            confidence,
            now_ts()
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    // 并发兜底：返回库中实际存在的占位符（可能由并发请求先写入）
    match conn.query_row(
        "SELECT placeholder FROM desensitize_mapping WHERE original = ?1 LIMIT 1",
        [original],
        |r| r.get::<_, String>(0),
    ) {
        Ok(ph) => Ok(ph),
        Err(_) => Ok(placeholder),
    }
}

/// 记录脱敏审计日志（原文只存打码形态）
pub fn log_hit(
    db: &Database,
    session_key: &str,
    direction: &str,
    original: &str,
    placeholder: Option<&str>,
    entity_type: &str,
    hit_source: &str,
    confidence: f32,
    restored: bool,
    context: Option<&str>,
    placeholder_context: Option<&str>,
) -> Result<(), AppError> {
    let conn = lock_conn!(db.conn);
    conn.execute(
        "INSERT INTO desensitize_hit_log
         (session_key, direction, original_masked, placeholder, entity_type, hit_source, confidence, restored, context, placeholder_context, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            session_key,
            direction,
            original,
            placeholder.unwrap_or(""),
            entity_type,
            hit_source,
            confidence,
            if restored { 1 } else { 0 },
            context.unwrap_or(""),
            placeholder_context.unwrap_or(""),
            now_ts()
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// 还原单段文本：扫描占位符并查表替换，返回 (还原后文本, 还原数, 失败数)
pub fn restore_text(db: &Database, text: &str) -> (String, usize, usize) {
    let cur = get_placeholder_prefix(db);
    if !text.contains("{(_") && !text.contains(PLACEHOLDER_PREFIX) && !text.contains(&cur) {
        return (text.to_string(), 0, 0);
    }
    let old = regex::escape(PLACEHOLDER_PREFIX);
    let cur_esc = regex::escape(&cur);
    // 新格式：{(_<slug>_<hash>_<seq>_]}；旧格式：<prefix>[<slug>_]<hash>_<seq>__（兼容存量）
    let body = format!(
        r"(?:\{{\(_[a-z0-9_]+?_[0-9a-f]{{6}}_[0-9]{{4}}_\]\}}|(?:{old}|{cur_esc})(?:[a-z0-9_]+?_[0-9a-f]{{6}}_[0-9]{{4}}__|[0-9a-f]{{6}}_[0-9]{{4}}__))"
    );
    let re = regex::Regex::new(&body).unwrap();
    // 非 Result 函数：直接用锁（poison 时取内部值，仅 SQLite 操作，安全）
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let mut restored = 0usize;
    let mut failed = 0usize;
    // 手动扫描占位符并查表还原（避免 Replacer 闭包生命周期约束）
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for caps in re.captures_iter(text) {
        let m = caps.get(0).expect("capture 0 always present");
        out.push_str(&text[last..m.start()]);
        let ph = &caps[0];
        let row = conn.query_row(
            "SELECT original, entity_type, hit_source, confidence FROM desensitize_mapping WHERE placeholder = ?1",
            [ph],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?,
            )),
        );
        match row {
            Ok((orig, et, src, conf)) => {
                restored += 1;
                // 记录还原上下文：占位符在 AI 返回文本中的片段 + 还原后原文片段
                let out_start = out.len();
                out.push_str(&orig);
                let out_end = out.len();
                let (ctx_orig, ctx_ph) = context_snippet(&out, out_start, out_end, 24, ph);
                let _ = log_hit(
                    db,
                    "out",
                    "out",
                    &orig,
                    Some(ph),
                    &et,
                    &src,
                    conf as f32,
                    true,
                    Some(&ctx_orig),
                    Some(&ctx_ph),
                );
            }
            Err(_) => {
                failed += 1;
                out.push_str(ph); // 保留占位符，避免输出错误明文
            }
        }
        last = m.end();
    }
    out.push_str(&text[last..]);
    (out, restored, failed)
}

/// 增量还原（流式跨 chunk）：对累积文本做还原，末尾若有未闭合的
/// 占位符前缀（如 "{(_keyword..." 但无 "_]}"）则保留在 pending，不输出，
/// 等下一段拼接后再处理。返回 (可输出文本, 保留的未闭合片段)。
pub fn restore_text_incremental(db: &Database, accumulated: &str) -> (String, String) {
    log::info!(
        "[desensitize] restore_incremental len={} has_ph={}",
        accumulated.len(),
        accumulated.contains("{(_")
    );
    let (full, restored, failed) = restore_text(db, accumulated);
    log::info!(
        "[desensitize] restore_incremental done restored={} failed={}",
        restored,
        failed
    );
    // 找最后一个未闭合的占位符起点
    if let Some(pos) = full.rfind("{(_") {
        // 从 pos 往后找是否有闭合 "_]}"
        let tail = &full[pos..];
        if !tail.contains("_]}") {
            // 未闭合：pos 之前输出，pos 之后保留
            return (full[..pos].to_string(), full[pos..].to_string());
        }
    }
    (full, String::new())
}

/// 递归还原 JSON 值
pub fn restore_value(db: &Database, value: Value) -> Value {
    match value {
        Value::String(s) => {
            let (out, _, _) = restore_text(db, &s);
            Value::String(out)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(|v| restore_value(db, v)).collect()),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, restore_value(db, v)))
                .collect(),
        ),
        other => other,
    }
}

/// 大文本是否跳过语义通道（settings 表；未配置时默认 true=跳过，仅正则）
pub fn get_skip_semantic_large(db: &Database) -> bool {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_skip_semantic_large'",
        [],
        |r| r.get::<_, String>(0).map(|v| v != "0"),
    )
    .unwrap_or(true)
}

/// 语义匹配相似度阈值（settings 表；未配置时默认 0.75）
pub fn get_semantic_threshold(db: &Database) -> f32 {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_semantic_threshold'",
        [],
        |r| {
            r.get::<_, String>(0)
                .and_then(|v| v.parse::<f32>().map_err(|_| rusqlite::Error::InvalidQuery))
        },
    )
    .unwrap_or(0.75)
}

/// 策略① 内置 PII 开关（settings 表；未配置时默认启用）
pub fn get_pii_enabled(db: &Database) -> bool {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_pii_enabled'",
        [],
        |r| r.get::<_, String>(0).map(|v| v != "0"),
    )
    .unwrap_or(true)
}

/// 策略② 自定义关键词开关（settings 表；未配置时默认启用）
pub fn get_keyword_enabled(db: &Database) -> bool {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_keyword_enabled'",
        [],
        |r| r.get::<_, String>(0).map(|v| v != "0"),
    )
    .unwrap_or(true)
}

/// 全局脱敏开关（settings 表；未配置时默认启用）
pub fn get_enabled(db: &Database) -> bool {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_enabled'",
        [],
        |r| r.get::<_, String>(0).map(|v| v == "1"),
    )
    .unwrap_or(true)
}

/// 脱敏范围（settings 表；未配置时默认 "user"=仅用户内容，不碰系统提示词/工具定义等）
pub fn get_scope(db: &Database) -> String {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_scope'",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "user".to_string())
}

/// 用户关键词规则 CRUD
pub fn list_keywords(db: &Database) -> Result<Vec<KeywordRule>, AppError> {
    let conn = lock_conn!(db.conn);
    let mut stmt = conn
        .prepare(
            "SELECT id, keyword, entity_type, match_mode, threshold, enabled
             FROM desensitize_keywords ORDER BY id DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map([], |r| {
            Ok(KeywordRule {
                id: r.get(0)?,
                keyword: r.get(1)?,
                entity_type: r.get(2)?,
                match_mode: r.get(3)?,
                threshold: r.get::<_, f64>(4)? as f32,
                enabled: r.get::<_, i64>(5)? != 0,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| AppError::Database(e.to_string()))?);
    }
    Ok(out)
}

pub fn add_keyword(
    db: &Database,
    keyword: &str,
    entity_type: &str,
    match_mode: &str,
    threshold: f64,
) -> Result<i64, AppError> {
    let conn = lock_conn!(db.conn);
    conn.execute(
        "INSERT INTO desensitize_keywords (keyword, entity_type, match_mode, threshold, enabled)
         VALUES (?1, ?2, ?3, ?4, 1)",
        rusqlite::params![keyword, entity_type, match_mode, threshold],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(conn.last_insert_rowid())
}

/// 关键词是否已存在（导入模板时跳过重复项）
pub fn keyword_exists(db: &Database, keyword: &str) -> Result<bool, AppError> {
    let conn = lock_conn!(db.conn);
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM desensitize_keywords WHERE keyword = ?1",
            [keyword],
            |row| row.get(0),
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(n > 0)
}

pub fn remove_keyword(db: &Database, id: i64) -> Result<(), AppError> {
    let conn = lock_conn!(db.conn);
    conn.execute("DELETE FROM desensitize_keywords WHERE id = ?1", [id])
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// 当前占位符前缀（settings；未配置默认 __SEC_）
pub fn get_placeholder_prefix(db: &Database) -> String {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = 'desensitize_placeholder_prefix'",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| crate::desensitize::PLACEHOLDER_PREFIX.to_string())
}

/// 设置占位符前缀（映射规则）。校验后写入 settings 并刷新缓存。
pub fn set_placeholder_prefix(db: &Database, prefix: &str) -> Result<(), AppError> {
    let prefix = prefix.trim();
    if prefix.is_empty() {
        return Err(AppError::InvalidInput("占位符前缀不能为空".to_string()));
    }
    if prefix.chars().count() > 16 {
        return Err(AppError::InvalidInput(
            "占位符前缀过长（最多 16 个字符）".to_string(),
        ));
    }
    if !prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(AppError::InvalidInput(
            "占位符前缀仅支持字母、数字、下划线".to_string(),
        ));
    }
    let conn = lock_conn!(db.conn);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('desensitize_placeholder_prefix', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [prefix],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;
    crate::desensitize::refresh_prefix_cache(prefix);
    Ok(())
}

/// 查询映射表（分页 + 搜索占位符/原文 + 会话筛选），返回 (总数, 列表)
pub fn list_mappings(
    db: &Database,
    search: &str,
    session_key: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<(i64, Vec<Value>), AppError> {
    let conn = lock_conn!(db.conn);
    let search = search.trim();
    let like = if search.is_empty() {
        String::new()
    } else {
        format!("%{search}%")
    };
    let session = session_key.unwrap_or("").trim();
    let mut conds: Vec<&str> = Vec::new();
    if !search.is_empty() {
        conds.push("(placeholder LIKE ?1 OR original LIKE ?1)");
    }
    if !session.is_empty() {
        conds.push("session_key = ?2");
    }
    let where_sql = if conds.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conds.join(" AND "))
    };
    let total: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM desensitize_mapping{where_sql}"),
            rusqlite::params![like, session],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mut stmt = conn
        .prepare(&format!(
            "SELECT id, session_key, placeholder, original, entity_type, hit_source, confidence, created_at
             FROM desensitize_mapping{where_sql} ORDER BY id DESC LIMIT ?3 OFFSET ?4"
        ))
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![like, session, limit, offset], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, f64>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut items = Vec::new();
    for row in rows {
        let (id, sk, ph, orig, et, src, conf, ts) =
            row.map_err(|e| AppError::Database(e.to_string()))?;
        items.push(json!({
            "id": id,
            "session_key": sk,
            "placeholder": ph,
            "original": orig,
            "entity_type": et,
            "hit_source": src,
            "confidence": conf,
            "created_at": ts,
        }));
    }
    Ok((total, items))
}

/// 删除单条映射（删除后对应占位符不再还原）
pub fn delete_mapping(db: &Database, id: i64) -> Result<(), AppError> {
    let conn = lock_conn!(db.conn);
    conn.execute("DELETE FROM desensitize_mapping WHERE id = ?1", [id])
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

/// 清空映射表（可选按会话）
pub fn clear_mappings(db: &Database, session_key: Option<&str>) -> Result<i64, AppError> {
    let conn = lock_conn!(db.conn);
    let n = match session_key {
        Some(s) => conn.execute(
            "DELETE FROM desensitize_mapping WHERE session_key = ?1",
            [s],
        ),
        None => conn.execute("DELETE FROM desensitize_mapping", []),
    }
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(n as i64)
}

/// 仪表盘统计
pub fn dashboard(db: &Database) -> Result<Value, AppError> {
    let conn = lock_conn!(db.conn);
    let day_start = now_ts() - 86400;
    let today_hits: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM desensitize_hit_log WHERE created_at >= ?1",
            [day_start],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_hits: i64 = conn
        .query_row("SELECT COUNT(*) FROM desensitize_hit_log", [], |r| r.get(0))
        .unwrap_or(0);
    let restored_ok: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM desensitize_hit_log WHERE restored = 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let mapping_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM desensitize_mapping", [], |r| r.get(0))
        .unwrap_or(0);
    let active_sessions: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT session_key) FROM desensitize_mapping WHERE created_at >= ?1",
            [day_start],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let restore_rate = if total_hits > 0 {
        (restored_ok as f64 / total_hits as f64 * 100.0 * 100.0).round() / 100.0
    } else {
        100.0
    };

    // 类型分布（今日）
    let mut type_dist: Vec<Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT entity_type, COUNT(*) c FROM desensitize_hit_log
                 WHERE created_at >= ?1 GROUP BY entity_type ORDER BY c DESC LIMIT 8",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([day_start], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        for row in rows {
            let (t, c) = row.map_err(|e| AppError::Database(e.to_string()))?;
            type_dist.push(json!({ "entity_type": t, "count": c }));
        }
    }

    // 最近命中
    let mut recent: Vec<Value> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT session_key, direction, original_masked, placeholder, entity_type,
                        hit_source, confidence, restored, created_at
                 FROM desensitize_hit_log ORDER BY id DESC LIMIT 8",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, f64>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, i64>(8)?,
                ))
            })
            .map_err(|e| AppError::Database(e.to_string()))?;
        for row in rows {
            let (sk, dir, masked, ph, et, src, conf, res, ts) =
                row.map_err(|e| AppError::Database(e.to_string()))?;
            recent.push(json!({
                "session_key": sk, "direction": dir, "original_masked": masked,
                "placeholder": ph, "entity_type": et, "hit_source": src,
                "confidence": conf, "restored": res != 0, "created_at": ts,
            }));
        }
    }

    Ok(json!({
        "today_hits": today_hits,
        "total_hits": total_hits,
        "restore_rate": restore_rate,
        "mapping_count": mapping_count,
        "active_sessions": active_sessions,
        "type_distribution": type_dist,
        "recent_hits": recent,
    }))
}

/// 查询审计日志（分页）
pub fn query_logs(db: &Database, limit: i64, offset: i64) -> Result<Vec<Value>, AppError> {
    let conn = lock_conn!(db.conn);
    let mut stmt = conn
        .prepare(
            "SELECT session_key, direction, original_masked, placeholder, entity_type,
                    hit_source, confidence, restored, context, placeholder_context, created_at
             FROM desensitize_hit_log ORDER BY id DESC LIMIT ?1 OFFSET ?2",
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    let rows = stmt
        .query_map(rusqlite::params![limit, offset], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, f64>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, i64>(10)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?;
    let mut out = Vec::new();
    for row in rows {
        let (sk, dir, masked, ph, et, src, conf, res, ctx, pctx, ts) =
            row.map_err(|e| AppError::Database(e.to_string()))?;
        out.push(json!({
            "session_key": sk, "direction": dir, "original_masked": masked,
            "placeholder": ph, "entity_type": et, "hit_source": src,
            "confidence": conf, "restored": res != 0, "context": ctx.unwrap_or_default(),
            "placeholder_context": pctx.unwrap_or_default(),
            "created_at": ts,
        }));
    }
    Ok(out)
}

/// 截取命中词的上下文片段（前后各 pad 个字符），并返回「原文片段」与「占位符替换后片段」
pub fn context_snippet(
    text: &str,
    start: usize,
    end: usize,
    pad: usize,
    ph: &str,
) -> (String, String) {
    let chars: Vec<char> = text.chars().collect();
    let s = text[..start].chars().count();
    let e = s + text[start..end].chars().count();
    let cs = s.saturating_sub(pad);
    let ce = (e + pad).min(chars.len());
    let mut ctx_orig = String::new();
    let mut ctx_ph = String::new();
    if cs > 0 {
        ctx_orig.push('…');
        ctx_ph.push('…');
    }
    ctx_orig.extend(chars[cs..s].iter());
    ctx_ph.extend(chars[cs..s].iter());
    let seg: String = chars[s..e].iter().collect();
    ctx_orig.push_str(&seg);
    ctx_ph.push_str(ph);
    ctx_orig.extend(chars[e..ce].iter());
    ctx_ph.extend(chars[e..ce].iter());
    if ce < chars.len() {
        ctx_orig.push('…');
        ctx_ph.push('…');
    }
    (ctx_orig, ctx_ph)
}

/// 试算：返回命中列表 + 打码文本 + 占位符文本（只读不落库）。
/// threshold 传入时临时覆盖所有关键词的语义相似度阈值（用于「本地模型管理」调参评估）。
pub fn preview(
    db: &Database,
    text: &str,
    keywords: &[KeywordRule],
    threshold: Option<f32>,
) -> Result<Value, AppError> {
    use crate::desensitize::pipeline;
    use crate::desensitize::rules;
    let eff_kw: Vec<KeywordRule> = match threshold {
        Some(th) => keywords
            .iter()
            .map(|k| {
                let mut k2 = k.clone();
                k2.threshold = th;
                k2
            })
            .collect(),
        None => keywords.to_vec(),
    };
    let enabled_rules = rules::builtin_enabled_set(db);
    let hits = pipeline::scan_all(text, &eff_kw, &enabled_rules);
    let prefix = get_placeholder_prefix(db);
    let mut masked = String::new();
    let mut placeholderized = String::new();
    let mut last = 0usize;
    let mut hit_list: Vec<Value> = Vec::new();
    // 预览占位符：不写入映射表；同原文复用同一占位符（与真实通道一致，保留语义）
    let mut ph_cache: HashMap<String, String> = HashMap::new();
    for h in hits.iter() {
        let seg = &text[h.start..h.end];
        masked.push_str(&text[last..h.start]);
        masked.push_str(&mask_text(seg));
        placeholderized.push_str(&text[last..h.start]);
        let ph = if let Some(p) = ph_cache.get(seg) {
            p.clone()
        } else {
            let slug = type_slug(&h.entity_type);
            let p = format!("{{(_{slug}_test_{:04}_]}}", ph_cache.len() + 1);
            ph_cache.insert(seg.to_string(), p.clone());
            p
        };
        placeholderized.push_str(&ph);
        // 命中词上下文片段（前后各 12 个字符），用于列表展示
        let (ctx_orig, ctx_ph) = context_snippet(text, h.start, h.end, 12, &ph);
        hit_list.push(json!({
            "entity_type": h.entity_type,
            "source": h.source,
            "confidence": h.confidence,
            "original": seg,
            "original_masked": mask_text(seg),
            "placeholder": ph,
            "context": ctx_orig,
            "placeholder_context": ctx_ph,
        }));
        last = h.end;
    }
    masked.push_str(&text[last..]);
    placeholderized.push_str(&text[last..]);
    Ok(json!({
        "hits": hit_list,
        "masked": masked,
        "placeholderized": placeholderized,
    }))
}
