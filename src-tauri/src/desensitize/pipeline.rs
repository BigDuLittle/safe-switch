//! [desensitize] 管线编排
//!
//! 入站（forwarder 调用）：`process_inbound(body, session_key)` —— 正则 + 关键词识别
//!   敏感实体 → 占位符替换 → 写映射表与审计日志。
//! 出站（响应还原）：`restore_value` / `restore_text` / `RestoreStream`（SSE 行缓冲还原），
//!   按占位符查表还原，失败保留占位符并标记（绝不输出错误明文）。

use crate::desensitize::mapper;
use crate::desensitize::rules;
use crate::desensitize::semantic::{self, KeywordRule};
use futures::Stream;
use serde_json::Value;
use std::collections::HashSet;
use std::pin::Pin;
use std::task::{Context, Poll};

/// 单节点文本超过该长度时，若开关开启则跳过语义通道（仅正则），避免编辑距离扫描大文本的性能开销
pub const SEMANTIC_SKIP_MAX_LEN: usize = 256 * 1024;

/// 一次脱敏命中（合并后）
#[derive(Debug, Clone)]
pub struct Hit {
    pub start: usize,
    pub end: usize,
    pub entity_type: String,
    pub source: String, // regex | keyword | semantic
    pub confidence: f32,
    /// 还原标准词；关键词命中时为用户定义的关键词本身（变体共用占位符）
    pub restore_as: Option<String>,
}

/// 扫描文本：正则通道（按启用规则过滤）+ 关键词语义通道，区间合并、跳过占位符
pub fn scan_all(
    text: &str,
    keywords: &[KeywordRule],
    enabled_rules: &HashSet<String>,
) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();

    // 1. 正则通道（仅扫描用户启用的内置规则）
    for (s, e, et, _name, conf) in rules::scan_regex_filtered(text, enabled_rules) {
        hits.push(Hit {
            start: s,
            end: e,
            entity_type: et,
            source: "regex".to_string(),
            confidence: conf,
            restore_as: None,
        });
    }

    // 2. 关键词语义通道
    for sh in semantic::scan_keywords(text, keywords) {
        hits.push(Hit {
            start: sh.start,
            end: sh.end,
            entity_type: sh.entity_type.clone(),
            source: "semantic".to_string(),
            confidence: sh.confidence,
            restore_as: sh.restore_as,
        });
    }

    // 3. 过滤与占位符重叠的命中（幂等：不二次脱敏；兼容自定义前缀）
    hits.retain(|h| {
        let seg = &text[h.start..h.end];
        !crate::desensitize::is_placeholder(seg)
            && !seg.contains(crate::desensitize::PLACEHOLDER_PREFIX)
            && !seg.contains(&crate::desensitize::cached_prefix())
    });

    // 4. 按 start 排序，重叠区间保留 confidence 更高/更长者
    hits.sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    let mut merged: Vec<Hit> = Vec::new();
    for h in hits {
        if let Some(last) = merged.last_mut() {
            if h.start < last.end {
                // 重叠：保留更长/更高置信度
                if (h.end - h.start) > (last.end - last.start) || h.confidence > last.confidence {
                    *last = h;
                }
                continue;
            }
        }
        merged.push(h);
    }
    merged
}

/// 入站脱敏：递归替换 JSON 字符串节点
pub fn process_inbound(body: Value, session_key: &str) -> Value {
    let Some(db) = crate::desensitize::db() else {
        return body;
    };
    // [desensitize] 全局开关关闭：整包旁路（不扫描、不改写）
    if !mapper::get_enabled(db) {
        return body;
    }
    let keywords = mapper::list_keywords(db).unwrap_or_default();
    let skip_large = false; // 大文本跳过语义通道已停用（用户要求移除该功能）
    let scope = mapper::get_scope(db);
    walk_inbound(body, db, session_key, &keywords, skip_large, &scope)
}

/// 判断消息元素是否为用户消息（对象且有 role=="user"）
fn is_user_message(v: &Value) -> bool {
    matches!(v, Value::Object(map) if matches!(map.get("role"), Some(Value::String(r)) if r == "user"))
}

/// 入站路由：按范围模式分派。
/// - "user"（默认）：只处理 messages/input/contents 数组中的 user 消息子树，
///   系统提示词（system 字段/system 消息）、assistant/tool 消息、工具定义等一律不碰；
/// - 其他值（"all"）：整包递归（原行为）。
fn walk_inbound(
    value: Value,
    db: &std::sync::Arc<crate::database::Database>,
    session: &str,
    keywords: &[KeywordRule],
    skip_large: bool,
    scope: &str,
) -> Value {
    if scope != "user" {
        return walk_all(value, db, session, keywords, skip_large);
    }
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| {
                    if k == "system" {
                        (k, v) // 系统提示词不碰
                    } else if k == "messages" || k == "input" || k == "contents" {
                        if let Value::Array(arr) = v {
                            let routed: Vec<Value> = arr
                                .into_iter()
                                .map(|el| {
                                    if is_user_message(&el) {
                                        walk_all(el, db, session, keywords, skip_large)
                                    } else {
                                        el // assistant/system/tool 消息不碰
                                    }
                                })
                                .collect();
                            (k, Value::Array(routed))
                        } else {
                            (k, v)
                        }
                    } else {
                        (k, v) // 工具定义、metadata 等其他字段不碰
                    }
                })
                .collect(),
        ),
        other => other, // 非对象（如顶层数组）：user 模式下保守不碰
    }
}

/// 整包递归脱敏（"all" 模式与 user 消息子树共用）
fn walk_all(
    value: Value,
    db: &std::sync::Arc<crate::database::Database>,
    session: &str,
    keywords: &[KeywordRule],
    skip_large: bool,
) -> Value {
    match value {
        Value::String(s) => Value::String(replace_text_inbound(&s, db, session, keywords, skip_large)),
        Value::Array(arr) => Value::Array(
            arr.into_iter()
                .map(|v| walk_all(v, db, session, keywords, skip_large))
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (k, walk_all(v, db, session, keywords, skip_large)))
                .collect(),
        ),
        other => other,
    }
}

fn replace_text_inbound(
    text: &str,
    db: &std::sync::Arc<crate::database::Database>,
    session: &str,
    keywords: &[KeywordRule],
    skip_large: bool,
) -> String {
    if crate::desensitize::is_placeholder(text) {
        return text.to_string();
    }
    // 大文本且开关开启：语义通道跳过（传空关键词），仅正则兜底
    let eff_kw: &[KeywordRule] = if skip_large && text.len() > SEMANTIC_SKIP_MAX_LEN {
        &[]
    } else {
        keywords
    };
    // 策略级开关：策略①关闭 → 内置 PII 集合置空；策略②关闭 → 关键词置空
    let pii_on = mapper::get_pii_enabled(db);
    let kw_on = mapper::get_keyword_enabled(db);
    let enabled_rules = if pii_on {
        rules::builtin_enabled_set(db)
    } else {
        std::collections::HashSet::new()
    };
    let eff_kw: &[KeywordRule] = if !kw_on { &[] } else { eff_kw };
    let hits = scan_all(text, eff_kw, &enabled_rules);
    if hits.is_empty() {
        return text.to_string();
    }
    let mut out = text.to_string();
    // 从后往前替换，保持字节区间有效
    for h in hits.iter().rev() {
        let seg = &out[h.start..h.end];
        let restore_seg = h.restore_as.as_deref().unwrap_or(seg);
        let placeholder =
            match mapper::ensure_mapping(db, session, restore_seg, &h.entity_type, &h.source, h.confidence)
            {
                Ok(p) if !p.is_empty() => p,
                _ => continue,
            };
        let (ctx_orig, ctx_ph) =
            mapper::context_snippet(text, h.start, h.end, 24, &placeholder);
        let _ = mapper::log_hit(
            db,
            session,
            "in",
            seg,
            Some(&placeholder),
            &h.entity_type,
            &h.source,
            h.confidence,
            true,
            Some(&ctx_orig),
            Some(&ctx_ph),
        );
        out.replace_range(h.start..h.end, &placeholder);
    }
    out
}

/// 出站还原（JSON 值）
pub fn restore_value(body: Value) -> Value {
    let Some(db) = crate::desensitize::db() else {
        return body;
    };
    mapper::restore_value(db, body)
}

/// 出站还原（纯文本）
pub fn restore_text(text: &str) -> String {
    let Some(db) = crate::desensitize::db() else {
        return text.to_string();
    };
    let (out, restored, failed) = mapper::restore_text(db, text);
    if failed > 0 {
        log::warn!("[desensitize] {failed} 个占位符未还原（保留原占位符，避免错误明文）");
    }
    if restored > 0 {
        let _ = mapper::log_hit(
            db,
            "out",
            "out",
            "",
            None,
            "restore",
            "restore",
            1.0,
            failed == 0,
            None,
            None,
        );
    }
    out
}

/// 出站还原（字节，非流式整包响应）
pub fn restore_bytes(bytes: &[u8]) -> Vec<u8> {
    match std::str::from_utf8(bytes) {
        Ok(s) => restore_text(s).into_bytes(),
        Err(_) => bytes.to_vec(), // 非 UTF-8（二进制/压缩），不做字符串还原
    }
}

/// SSE 单行还原：`data: {...}` 行内的字符串内容还原
pub fn restore_sse_line(line: &str) -> String {
    if !line.starts_with("data:") {
        return line.to_string();
    }
    let payload = line.trim_start_matches("data:").trim();
    if payload == "[DONE]" || payload.is_empty() {
        return line.to_string();
    }
    if let Ok(mut v) = serde_json::from_str::<Value>(payload) {
        v = restore_value(v);
        if let Ok(s) = serde_json::to_string(&v) {
            return format!("data: {s}");
        }
    }
    line.to_string()
}

/// SSE 流式还原器（行缓冲，跨 chunk 安全）
pub struct RestoreStream<S> {
    inner: S,
    buffer: Vec<u8>,
    pending: String,
    done: bool,
}

impl<S> RestoreStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: Vec::with_capacity(4096),
            pending: String::new(),
            done: false,
        }
    }
}

impl<S> Stream for RestoreStream<S>
where
    S: Stream<Item = Result<bytes::Bytes, std::io::Error>>,
{
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // SAFETY: RestoreStream 除 inner 外无其他被 poll 的字段；
            // self 已被 Pin 固定，inner 字段在 poll 期间不会被移动。
            let inner = unsafe { self.as_mut().map_unchecked_mut(|s| &mut s.inner) };
            match inner.poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    let this = unsafe { self.as_mut().get_unchecked_mut() };
                    this.buffer.extend_from_slice(&chunk);
                    // 纯文本级跨 chunk 还原：把累积文本（含 pending）整体还原，
                    // 保留末尾未闭合占位符前缀，等下一段拼上再处理。
                    let text = String::from_utf8_lossy(&this.buffer).into_owned();
                    log::info!("[desensitize] RestoreStream chunk len={} has_kw={} has_phone={} preview={}", text.len(), text.contains("kw_"), text.contains("phone_"), &text.chars().take(150).collect::<String>());
                    let combined = format!("{}{}", this.pending, text);
                    let (replaced, keep) = if let Some(db) = crate::desensitize::db() {
                        mapper::restore_text_incremental(db, &combined)
                    } else {
                        (combined, String::new())
                    };
                    this.pending = keep;
                    this.buffer.clear();
                    if !replaced.is_empty() {
                        return Poll::Ready(Some(Ok(bytes::Bytes::from(replaced.into_bytes()))));
                    }
                    continue;
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    let this = unsafe { self.as_mut().get_unchecked_mut() };
                    if !this.done {
                        this.done = true;
                        let combined = format!("{}{}", this.pending, String::from_utf8_lossy(&this.buffer));
                        let out = if let Some(db) = crate::desensitize::db() {
                            let (r, k) = mapper::restore_text_incremental(db, &combined);
                            format!("{}{}", r, k)
                        } else {
                            combined
                        };
                        this.buffer.clear();
                        this.pending.clear();
                        if !out.is_empty() {
                            return Poll::Ready(Some(Ok(bytes::Bytes::from(out.into_bytes()))));
                        }
                    }
                    return Poll::Ready(None);
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    fn no_kw() -> Vec<KeywordRule> {
        vec![]
    }

    fn all_enabled() -> std::collections::HashSet<String> {
        crate::desensitize::rules::all_default_enabled()
    }

    #[test]
    fn scan_regex_phone() {
        let hits = scan_all("手机 13800138000 联系", &no_kw(), &all_enabled());
        assert!(hits
            .iter()
            .any(|h| h.entity_type == "cn_phone" && h.source == "regex"));
    }

    #[test]
    fn scan_regex_respects_disabled_rule() {
        let enabled = std::collections::HashSet::from([
            "cn_id_card".to_string(),
            "email".to_string(),
            "intranet_ip".to_string(),
            "intranet_domain".to_string(),
            "openai_key".to_string(),
        ]);
        let hits = scan_all("手机 13800138000 联系", &no_kw(), &enabled);
        assert!(hits.is_empty(), "手机号规则关闭后不应命中");
    }

    #[test]
    fn placeholder_skipped() {
        let hits = scan_all("__SEC_ab12cd_0001__", &no_kw(), &all_enabled());
        assert!(hits.is_empty());
        // 新格式占位符同样跳过
        let hits2 = scan_all("{(_phone_ab12cd_0001_]}", &no_kw(), &all_enabled());
        assert!(hits2.is_empty());
    }

    #[test]
    fn restore_text_roundtrip_without_db_keeps_unchanged() {
        // 无 DB 注册时还原原样返回
        assert_eq!(restore_text("hello"), "hello");
    }

    #[test]
    fn sse_line_passthrough_when_no_db() {
        let line = "data: {\"content\":\"hi\"}";
        assert_eq!(restore_sse_line(line), line);
    }

    #[test]
    fn json_walk_inbound_no_db_identity() {
        let v = json!({"a": "13800138000"});
        // 未注册 DB 时 process_inbound 直接返回原值（安全降级）
        let out = process_inbound(v.clone(), "s1");
        assert_eq!(out, v);
    }

    #[test]
    fn large_text_with_empty_keywords_still_regex_masks() {
        // 大文本且语义通道关闭（空关键词）时：正则仍兜底、不产生语义命中
        let mut text = "a".repeat(SEMANTIC_SKIP_MAX_LEN + 10);
        text.push_str(" 联系 13800138000 谢谢");
        let hits = scan_all(&text, &[], &all_enabled());
        assert!(hits.iter().any(|h| h.entity_type == "cn_phone" && h.source == "regex"));
        assert!(hits.iter().all(|h| h.source == "regex"));
    }

    fn mem_db_with_tables() -> Arc<crate::database::Database> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::database::Database::create_desensitize_tables_on_conn(&conn).unwrap();
        Arc::new(crate::database::Database {
            conn: std::sync::Mutex::new(conn),
        })
    }
    #[test]
    fn mapping_reused_globally_across_sessions() {
        // 同一原文在不同会话中复用同一占位符（映射表全局保留）
        let db = mem_db_with_tables();
        let ph_a = mapper::ensure_mapping(&db, "session_a", "张三", "person", "keyword", 0.9)
            .unwrap();
        let ph_b = mapper::ensure_mapping(&db, "session_b", "张三", "person", "keyword", 0.9)
            .unwrap();
        assert_eq!(ph_a, ph_b, "same original must reuse the same placeholder");
        assert!(ph_a.starts_with("{(_"), "unexpected: {ph_a}");
        // 不同原文 → 不同占位符
        let ph_c = mapper::ensure_mapping(&db, "session_a", "李四", "person", "keyword", 0.9)
            .unwrap();
        assert_ne!(ph_a, ph_c);
        // 还原：占位符全局查回原文（不依赖会话）
        let (out, restored, failed) = mapper::restore_text(&db, &format!("你好 {ph_a} 再见"));
        assert_eq!(out, "你好 张三 再见");
        assert_eq!(restored, 1);
        assert_eq!(failed, 0);
    }

    #[test]
    fn user_scope_skips_system_and_assistant_but_masks_user() {
        // 默认范围 "user"：system 字段、system/assistant 消息、工具定义不碰，仅 user 内容脱敏
        let db = mem_db_with_tables();
        let body = json!({
            "system": "你是助手 13800138000",
            "messages": [
                {"role": "system", "content": "规则 13800138000"},
                {"role": "user", "content": "我的电话 13800138000"},
                {"role": "assistant", "content": "好的 13800138000"}
            ],
            "tools": [{"function": {"name": "f", "description": "示例 13800138000"}}]
        });
        let out = walk_inbound(body, &db, "s1", &[], false, "user");
        assert_eq!(out["system"], "你是助手 13800138000");
        assert_eq!(out["messages"][0]["content"], "规则 13800138000");
        let user_c = out["messages"][1]["content"].as_str().unwrap();
        assert!(user_c.contains("{(_") && !user_c.contains("13800138000"));
        assert_eq!(out["messages"][2]["content"], "好的 13800138000");
        assert_eq!(out["tools"][0]["function"]["description"], "示例 13800138000");
    }

    #[test]
    fn type_slug_normalizes_entity_types() {
        assert_eq!(mapper::type_slug("cn_phone"), "phone");
        assert_eq!(mapper::type_slug("cn_id_card"), "id_card");
        assert_eq!(mapper::type_slug("intl_phone"), "phone");
        assert_eq!(mapper::type_slug("email"), "email");
        assert_eq!(mapper::type_slug("intranet_ip"), "ip");
        assert_eq!(mapper::type_slug("other_token"), "token");
        assert_eq!(mapper::type_slug("私人数据"), "kw");
        assert_eq!(mapper::type_slug("123abc"), "kw");
        assert_eq!(
            mapper::type_slug("averylongentitytype_that_exceeds_limit"),
            "averylongentityt"
        );
    }

    #[test]
    fn placeholder_new_format_slug_and_restore_roundtrip() {
        let db = mem_db_with_tables();
        // 新格式占位符带类型标识：{(_phone_<hash>_<seq>_]}
        let ph = mapper::ensure_mapping(&db, "sess-1", "13800138000", "cn_phone", "regex", 0.0)
            .unwrap();
        assert!(ph.starts_with("{(_phone_"), "unexpected: {ph}");
        assert!(ph.ends_with("_]}"));
        // 表中记录可还原回原文
        let (out, restored, failed) =
            mapper::restore_text(&db, &format!("电话 {ph} 谢谢"));
        assert_eq!(out, "电话 13800138000 谢谢");
        assert_eq!(restored, 1);
        assert_eq!(failed, 0);
        // 旧格式占位符（表中无记录）保持原样不破坏文本
        let old_ph = "__SEC_ab12cd_0001__";
        let (out2, _, _) = mapper::restore_text(&db, &format!("x {old_ph} y"));
        assert_eq!(out2, format!("x {old_ph} y"));
    }

    #[test]
    fn strategy_switches_gate_keyword_channel() {
        let db = mem_db_with_tables();
        {
            let conn = db.conn.lock().unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT)",
            )
            .unwrap();
        }
        let kw = vec![crate::desensitize::semantic::KeywordRule {
            id: 1,
            keyword: "北极光".to_string(),
            entity_type: "project".to_string(),
            match_mode: "literal".to_string(),
            threshold: 0.5,
            enabled: true,
        }];
        // 策略②关闭 → 关键词通道不生效
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('desensitize_keyword_enabled', '0')",
                [],
            )
            .unwrap();
        }
        let out = walk_inbound(
            json!({"messages":[{"role":"user","content":"北极光启动"}]}),
            &db,
            "s1",
            &kw,
            false,
            "user",
        );
        assert_eq!(out["messages"][0]["content"], "北极光启动");
        // 策略①关闭 → 内置 PII 不生效
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO settings (key, value) VALUES ('desensitize_pii_enabled', '0')",
                [],
            )
            .unwrap();
        }
        let out2 = walk_inbound(
            json!({"messages":[{"role":"user","content":"电话 13800138000"}]}),
            &db,
            "s1",
            &[],
            false,
            "user",
        );
        assert_eq!(out2["messages"][0]["content"], "电话 13800138000");
        // 策略①恢复开启 → 手机号被替换为新格式占位符
        {
            let conn = db.conn.lock().unwrap();
            conn.execute(
                "UPDATE settings SET value = '1' WHERE key = 'desensitize_pii_enabled'",
                [],
            )
            .unwrap();
        }
        let out3 = walk_inbound(
            json!({"messages":[{"role":"user","content":"电话 13800138000"}]}),
            &db,
            "s1",
            &[],
            false,
            "user",
        );
        let c = out3["messages"][0]["content"].as_str().unwrap().to_string();
        assert!(c.contains("{(_phone_") && !c.contains("13800138000"), "unexpected: {c}");
    }

    #[test]
    fn all_scope_masks_everything_including_system() {
        // "all" 范围：整包递归（原行为）
        let db = mem_db_with_tables();
        let body = json!({
            "system": "你是助手 13800138000",
            "messages": [
                {"role": "system", "content": "规则 13800138000"},
                {"role": "user", "content": "我的电话 13800138000"}
            ]
        });
        let out = walk_inbound(body, &db, "s1", &[], false, "all");
        assert!(out["system"].as_str().unwrap().contains("{(_"));
        assert!(out["messages"][0]["content"].as_str().unwrap().contains("{(_"));
        assert!(out["messages"][1]["content"].as_str().unwrap().contains("{(_"));
    }
}
