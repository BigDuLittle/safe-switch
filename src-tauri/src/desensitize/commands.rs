//! [desensitize] 脱敏映射中心 Tauri 命令
//!
//! 前端通过 invoke 调用：仪表盘 / 规则 / 关键词 / 日志 / 试算 / 清空映射。
#![allow(
    clippy::all,
    dead_code,
    unused,
    unreachable_patterns,
    private_interfaces
)]

use crate::desensitize::mapper;
use crate::error::AppError;
use crate::store::AppState;
use serde_json::{json, Value};

fn err_string(e: AppError) -> String {
    e.to_string()
}

/// 仪表盘统计
#[tauri::command]
pub fn get_desensitize_dashboard(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    mapper::dashboard(state.db.as_ref()).map_err(err_string)
}

/// 内置规则列表（enabled 合并 DB 持久化开关；无记录时用内置默认）
#[tauri::command]
pub fn list_desensitize_rules(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    use crate::desensitize::rules;
    let enabled = rules::builtin_enabled_set(state.db.as_ref());
    let rules = rules::BUILTIN_RULES
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "entity_type": r.entity_type,
                "category": r.category,
                "pattern": r.pattern,
                "enabled": enabled.contains(r.entity_type),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!(rules))
}

/// 内置规则类别（含各类计数与中文名，前端分组渲染）
#[tauri::command]
pub fn list_desensitize_rule_categories(
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    use crate::desensitize::rules;
    let enabled = rules::builtin_enabled_set(state.db.as_ref());
    let cats = rules::CATEGORIES
        .iter()
        .map(|(cat, label)| {
            let items = rules::BUILTIN_RULES
                .iter()
                .filter(|r| r.category == *cat)
                .map(|r| {
                    json!({
                        "name": r.name,
                        "entity_type": r.entity_type,
                        "pattern": r.pattern,
                        "enabled": enabled.contains(r.entity_type),
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "category": cat,
                "label": label,
                "count": items.len(),
                "rules": items,
            })
        })
        .collect::<Vec<_>>();
    Ok(json!(cats))
}

/// 设置单条内置规则的启用状态（持久化到 desensitize_rules 表）
#[tauri::command]
pub fn set_desensitize_rule_enabled(
    state: tauri::State<'_, AppState>,
    entity_type: String,
    enabled: bool,
) -> Result<(), String> {
    use crate::desensitize::rules;
    let (name, pattern, category) =
        rules::builtin_rule_meta(&entity_type).ok_or_else(|| format!("未知规则: {entity_type}"))?;
    let conn = crate::database::lock_conn!(state.db.conn);
    let n = conn
        .execute(
            "UPDATE desensitize_rules SET enabled = ?1
             WHERE entity_type = ?2 AND rule_type = 'builtin'",
            rusqlite::params![if enabled { 1 } else { 0 }, &entity_type],
        )
        .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    if n == 0 {
        conn.execute(
            "INSERT INTO desensitize_rules (rule_type, name, pattern, entity_type, category, enabled)
             VALUES ('builtin', ?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                name,
                pattern,
                &entity_type,
                category,
                if enabled { 1 } else { 0 }
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    }
    Ok(())
}

/// 策略②语义类别列表（5 类敏感内容，含启用状态）
#[tauri::command]
pub fn list_desensitize_semantic_categories(
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    use crate::desensitize::semantic;
    let enabled = semantic::semantic_enabled_set(state.db.as_ref());
    let cats = semantic::SEMANTIC_CATEGORIES
        .iter()
        .map(|(et, name, desc)| {
            json!({
                "entity_type": et,
                "name": name,
                "desc": desc,
                "enabled": enabled.contains(*et),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!(cats))
}

/// 设置策略②某语义类别的启用状态（持久化到 desensitize_rules 表，rule_type='semantic'）
#[tauri::command]
pub fn set_desensitize_semantic_category_enabled(
    state: tauri::State<'_, AppState>,
    entity_type: String,
    enabled: bool,
) -> Result<(), String> {
    use crate::desensitize::semantic;
    let (name, desc) = semantic::SEMANTIC_CATEGORIES
        .iter()
        .find(|(et, _, _)| *et == entity_type)
        .map(|(_, n, d)| (*n, *d))
        .ok_or_else(|| format!("未知语义类别: {entity_type}"))?;
    let conn = crate::database::lock_conn!(state.db.conn);
    let n = conn
        .execute(
            "UPDATE desensitize_rules SET enabled = ?1
             WHERE entity_type = ?2 AND rule_type = 'semantic'",
            rusqlite::params![if enabled { 1 } else { 0 }, &entity_type],
        )
        .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    if n == 0 {
        conn.execute(
            "INSERT INTO desensitize_rules (rule_type, name, pattern, entity_type, category, enabled)
             VALUES ('semantic', ?1, ?2, ?3, 'semantic', ?4)",
            rusqlite::params![
                name,
                desc,
                &entity_type,
                if enabled { 1 } else { 0 }
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    }
    Ok(())
}

/// 更新用户关键词的匹配模式（每词「语义」开关：semantic / literal）
#[tauri::command]
pub fn update_desensitize_keyword_mode(
    state: tauri::State<'_, AppState>,
    id: i64,
    match_mode: String,
) -> Result<(), String> {
    if !matches!(match_mode.as_str(), "literal" | "semantic") {
        return Err(format!("不支持的匹配模式: {match_mode}"));
    }
    let conn = crate::database::lock_conn!(state.db.conn);
    let n = conn
        .execute(
            "UPDATE desensitize_keywords SET match_mode = ?1 WHERE id = ?2",
            rusqlite::params![&match_mode, id],
        )
        .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    if n == 0 {
        return Err("关键词不存在".to_string());
    }
    Ok(())
}

/// 用户关键词列表
#[tauri::command]
pub fn list_desensitize_keywords(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let kws = mapper::list_keywords(state.db.as_ref()).map_err(err_string)?;
    Ok(json!(kws
        .iter()
        .map(|k| json!({
            "id": k.id,
            "keyword": k.keyword,
            "entity_type": k.entity_type,
            "match_mode": k.match_mode,
            "threshold": k.threshold,
            "enabled": k.enabled,
        }))
        .collect::<Vec<_>>()))
}

/// 关键词的待匹配形式（关键词本身 + 折叠变体；语义匹配由离线向量模型计算相似度）
#[tauri::command]
pub fn get_desensitize_keyword_variants(keyword: String) -> Result<Value, String> {
    let kw = keyword.trim().to_lowercase();
    if kw.is_empty() {
        return Ok(json!([]));
    }
    let mut out = vec![json!({ "word": kw, "source": "self" })];
    let folded: String = kw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_' && *c != '.')
        .collect();
    if !folded.is_empty() && folded != kw {
        out.push(json!({ "word": folded, "source": "folded" }));
    }
    Ok(json!(out))
}

/// 导出关键词模板（JSON 文件，供批量配置）
#[tauri::command]
pub fn export_desensitize_keywords(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Value, String> {
    let kws = mapper::list_keywords(state.db.as_ref()).map_err(err_string)?;
    let arr: Vec<Value> = kws
        .iter()
        .map(|k| {
            json!({
                "keyword": k.keyword,
                "entity_type": k.entity_type,
                "match_mode": k.match_mode,
                "threshold": k.threshold,
                "enabled": k.enabled,
            })
        })
        .collect();
    let content =
        serde_json::to_string_pretty(&json!({ "keywords": arr })).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(json!({ "count": arr.len(), "path": path }))
}

/// 导入关键词模板（JSON 文件，已存在的关键词跳过）
#[tauri::command]
pub fn import_desensitize_keywords(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Value, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let arr = v
        .get("keywords")
        .and_then(|k| k.as_array())
        .ok_or_else(|| "模板缺少 keywords 数组".to_string())?;
    let mut imported = 0usize;
    let mut skipped = 0usize;
    for item in arr {
        let keyword = item
            .get("keyword")
            .and_then(|k| k.as_str())
            .unwrap_or("")
            .trim();
        if keyword.is_empty() {
            continue;
        }
        let mode = item
            .get("match_mode")
            .and_then(|m| m.as_str())
            .unwrap_or("semantic");
        if !matches!(mode, "literal" | "semantic") {
            continue;
        }
        let threshold = item
            .get("threshold")
            .and_then(|t| t.as_f64())
            .unwrap_or(0.5)
            .clamp(0.3, 0.99);
        let entity_type = item
            .get("entity_type")
            .and_then(|e| e.as_str())
            .unwrap_or("keyword");
        if mapper::keyword_exists(state.db.as_ref(), keyword).map_err(err_string)? {
            skipped += 1;
            continue;
        }
        mapper::add_keyword(state.db.as_ref(), keyword, entity_type, mode, threshold)
            .map_err(err_string)?;
        imported += 1;
    }
    Ok(json!({ "imported": imported, "skipped": skipped }))
}

/// 添加用户关键词
#[tauri::command]
pub fn add_desensitize_keyword(
    state: tauri::State<'_, AppState>,
    keyword: String,
    entity_type: Option<String>,
    match_mode: Option<String>,
    threshold: Option<f64>,
) -> Result<i64, String> {
    let kw = keyword.trim();
    if kw.is_empty() {
        return Err("关键词不能为空".to_string());
    }
    let mode = match_mode.unwrap_or_else(|| "semantic".to_string());
    if !matches!(mode.as_str(), "literal" | "semantic") {
        return Err(format!("不支持的匹配模式: {mode}"));
    }
    mapper::add_keyword(
        state.db.as_ref(),
        kw,
        &entity_type.unwrap_or_else(|| "keyword".to_string()),
        &mode,
        threshold.unwrap_or(0.5).clamp(0.3, 0.99),
    )
    .map_err(err_string)
}

/// 删除用户关键词
#[tauri::command]
pub fn remove_desensitize_keyword(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    mapper::remove_keyword(state.db.as_ref(), id).map_err(err_string)
}

/// 脱敏审计日志（分页）
#[tauri::command]
pub fn query_desensitize_logs(
    state: tauri::State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Value, String> {
    let rows = mapper::query_logs(state.db.as_ref(), limit.unwrap_or(50), offset.unwrap_or(0))
        .map_err(err_string)?;
    Ok(json!({ "logs": rows }))
}

/// 脱敏试算（本地预演，不发请求）。threshold 传入时临时覆盖语义相似度阈值。
#[tauri::command]
pub fn preview_desensitize(
    state: tauri::State<'_, AppState>,
    text: String,
    threshold: Option<f64>,
) -> Result<Value, String> {
    let keywords = mapper::list_keywords(state.db.as_ref()).map_err(err_string)?;
    mapper::preview(
        state.db.as_ref(),
        &text,
        &keywords,
        threshold.map(|t| t as f32),
    )
    .map_err(err_string)
}

/// 语义匹配相似度阈值
#[tauri::command]
pub fn get_desensitize_semantic_threshold(
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    let v = mapper::get_semantic_threshold(state.db.as_ref());
    Ok(json!({ "threshold": v }))
}

/// 保存语义匹配相似度阈值
#[tauri::command]
pub fn set_desensitize_semantic_threshold(
    state: tauri::State<'_, AppState>,
    threshold: f32,
) -> Result<(), String> {
    let clamped = threshold.clamp(0.30, 0.90);
    let conn = crate::database::lock_conn!(state.db.conn);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('desensitize_semantic_threshold', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [format!("{clamped:.2}")],
    )
    .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    Ok(())
}

/// 策略①（内置 PII）开关状态
#[tauri::command]
pub fn get_desensitize_pii_enabled(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let enabled = mapper::get_pii_enabled(state.db.as_ref());
    Ok(json!({ "enabled": enabled }))
}

/// 策略②（自定义关键词）开关状态
#[tauri::command]
pub fn get_desensitize_keyword_enabled(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let enabled = mapper::get_keyword_enabled(state.db.as_ref());
    Ok(json!({ "enabled": enabled }))
}

/// 设置策略①（内置 PII）开关
#[tauri::command]
pub fn set_desensitize_pii_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let v = if enabled { "1" } else { "0" };
    let conn = crate::database::lock_conn!(state.db.conn);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('desensitize_pii_enabled', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [v],
    )
    .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    Ok(())
}

/// 设置策略②（自定义关键词）开关
#[tauri::command]
pub fn set_desensitize_keyword_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let v = if enabled { "1" } else { "0" };
    let conn = crate::database::lock_conn!(state.db.conn);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('desensitize_keyword_enabled', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [v],
    )
    .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    Ok(())
}

/// 离线语义模型状态（本地模型管理页）
#[tauri::command]
pub fn get_desensitize_model_status(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let _ = state.db;
    Ok(json!({ "embedding": crate::desensitize::semantic::model_status() }))
}

/// 清空映射表（可选按会话）
#[tauri::command]
pub fn clear_desensitize_mappings(
    state: tauri::State<'_, AppState>,
    session_key: Option<String>,
) -> Result<Value, String> {
    let n =
        mapper::clear_mappings(state.db.as_ref(), session_key.as_deref()).map_err(err_string)?;
    Ok(json!({ "deleted": n }))
}

/// 查询映射表（分页 + 搜索 + 会话筛选）
#[tauri::command]
pub fn list_desensitize_mappings(
    state: tauri::State<'_, AppState>,
    search: Option<String>,
    session_key: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Value, String> {
    let (total, items) = mapper::list_mappings(
        state.db.as_ref(),
        search.as_deref().unwrap_or(""),
        session_key.as_deref(),
        limit.unwrap_or(50).min(200),
        offset.unwrap_or(0),
    )
    .map_err(err_string)?;
    Ok(json!({ "total": total, "items": items }))
}

/// 删除单条映射（删除后对应占位符不再还原）
#[tauri::command]
pub fn delete_desensitize_mapping(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    mapper::delete_mapping(state.db.as_ref(), id).map_err(err_string)
}

/// 当前占位符前缀（映射规则）
#[tauri::command]
pub fn get_desensitize_placeholder_prefix(
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    let prefix = mapper::get_placeholder_prefix(state.db.as_ref());
    Ok(json!({ "prefix": prefix }))
}

/// 设置占位符前缀（映射规则）
#[tauri::command]
pub fn set_desensitize_placeholder_prefix(
    state: tauri::State<'_, AppState>,
    prefix: String,
) -> Result<(), String> {
    mapper::set_placeholder_prefix(state.db.as_ref(), &prefix).map_err(err_string)
}

/// 全局开关状态（读 settings 表；未配置时默认启用）
#[tauri::command]
pub fn get_desensitize_enabled(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let enabled = mapper::get_enabled(state.db.as_ref());
    Ok(json!({ "enabled": enabled }))
}

/// 设置全局开关
#[tauri::command]
pub fn set_desensitize_enabled(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let conn = crate::database::lock_conn!(state.db.conn);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('desensitize_enabled', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [if enabled { "1" } else { "0" }],
    )
    .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    Ok(())
}

/// 大文本跳过语义通道开关状态（未配置时默认 true=跳过）
#[tauri::command]
pub fn get_desensitize_skip_semantic_large(
    state: tauri::State<'_, AppState>,
) -> Result<Value, String> {
    let skip = mapper::get_skip_semantic_large(state.db.as_ref());
    Ok(json!({ "skipSemanticLarge": skip }))
}

/// 设置大文本跳过语义通道开关
#[tauri::command]
pub fn set_desensitize_skip_semantic_large(
    state: tauri::State<'_, AppState>,
    skip: bool,
) -> Result<(), String> {
    let conn = crate::database::lock_conn!(state.db.conn);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('desensitize_skip_semantic_large', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [if skip { "1" } else { "0" }],
    )
    .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    Ok(())
}

/// 脱敏范围（"user"=仅用户内容 / "all"=全部内容；未配置默认 "user"）
#[tauri::command]
pub fn get_desensitize_scope(state: tauri::State<'_, AppState>) -> Result<Value, String> {
    let scope = mapper::get_scope(state.db.as_ref());
    Ok(json!({ "scope": scope }))
}

/// 设置脱敏范围（仅接受 user / all）
#[tauri::command]
pub fn set_desensitize_scope(
    state: tauri::State<'_, AppState>,
    scope: String,
) -> Result<(), String> {
    if scope != "user" && scope != "all" {
        return Err("scope must be 'user' or 'all'".to_string());
    }
    let conn = crate::database::lock_conn!(state.db.conn);
    conn.execute(
        "INSERT INTO settings (key, value) VALUES ('desensitize_scope', ?1)
         ON CONFLICT(key) DO UPDATE SET value = ?1",
        [&scope],
    )
    .map_err(|e| AppError::Database(e.to_string()).to_string())?;
    Ok(())
}
