//! API 中转（Relay）模块
//!
//! 为第三方 AI 工具提供 OpenAI 兼容的本地中转端点：
//! 第三方工具把 baseURL 指向 `http://127.0.0.1:{port}/vault/v1` 并填入本模块生成的
//! 本地 Key，请求经 CC Switch 校验后透传到用户配置的中转上游（upstream），
//! 响应（含 SSE 流式）原样透传回第三方工具。
//!
//! 配置项（settings 表）：
//! - `api_relay_key`            本地 Key（首次访问自动生成，可重新生成）
//! 上游地址与 Key 来自 providers 表 app_type=api-relay 的当前选中供应商（settings_config.baseUrl / apiKey）

use super::ProxyError;
use crate::database::Database;
use crate::desensitize::pipeline::RestoreStream;
use crate::error::AppError;
use axum::extract::State;
use axum::http::{header, HeaderMap, Request};
use axum::response::Response;
use axum::body::Body;
use http_body_util::BodyExt;
use reqwest::StatusCode;

/// 读取 settings 表中的字符串配置
pub fn get_relay_setting(db: &Database, key: &str) -> Option<String> {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// 写入 settings 表中的字符串配置
pub fn set_relay_setting(db: &Database, key: &str, value: &str) -> Result<(), String> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        rusqlite::params![key, value],
    )
    .map_err(|e| format!("保存配置失败: {e}"))?;
    Ok(())
}

/// 获取本地 Key；不存在时自动生成（UUID v4）
pub fn get_or_create_local_key(db: &Database) -> Result<String, String> {
    if let Some(existing) = get_relay_setting(db, "api_relay_key") {
        if !existing.trim().is_empty() {
            return Ok(existing);
        }
    }
    let key = uuid::Uuid::new_v4().to_string();
    set_relay_setting(db, "api_relay_key", &key)?;
    Ok(key)
}

/// 重新生成本地 Key
pub fn regenerate_local_key(db: &Database) -> Result<String, String> {
    let key = uuid::Uuid::new_v4().to_string();
    set_relay_setting(db, "api_relay_key", &key)?;
    Ok(key)
}

/// 校验本地 Key（Authorization: Bearer <key>）
fn validate_local_key(db: &Database, headers: &HeaderMap) -> Result<(), ProxyError> {
    let expected = get_relay_setting(db, "api_relay_key").unwrap_or_default();
    if expected.trim().is_empty() {
        return Err(ProxyError::AuthError(
            "API 中转本地 Key 未初始化，请在 CC Switch 的 API 中转页面刷新".to_string(),
        ));
    }
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(ProxyError::AuthError(
            "API 中转缺少 Authorization 头".to_string(),
        ));
    };
    let value = value
        .to_str()
        .map_err(|_| ProxyError::AuthError("Authorization 头格式无效".to_string()))?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .unwrap_or("")
        .trim();
    if token != expected {
        return Err(ProxyError::AuthError("API 中转本地 Key 无效".to_string()));
    }
    Ok(())
}

/// 读取当前选中的上游中转供应商配置（providers 表 app_type=api-relay 的 is_current=1 条目）
fn get_upstream(db: &Database) -> Result<(String, String), ProxyError> {
    let current_id = db
        .get_current_provider("api-relay")
        .map_err(|e| ProxyError::Internal(format!("读取 API 中转当前供应商失败: {e}")))?
        .ok_or_else(|| {
            ProxyError::Internal(
                "API 中转尚未配置上游供应商，请在 CC Switch 的 API 中转页面添加并启用".to_string()
            )
        })?;
    let provider = db
        .get_provider_by_id(&current_id, "api-relay")
        .map_err(|e| ProxyError::Internal(format!("读取 API 中转供应商失败: {e}")))?
        .ok_or_else(|| ProxyError::Internal("API 中转当前供应商不存在".to_string()))?;
    let url = provider
        .settings_config
        .get("baseUrl")
        .or_else(|| provider.settings_config.get("base_url"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ProxyError::Internal("API 中转当前供应商缺少 Base URL".to_string())
        })?
        .to_string();
    let key = provider
        .settings_config
        .get("apiKey")
        .or_else(|| provider.settings_config.get("api_key"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok((url, key))
}

/// 拼接上游完整 URL（保留原始路径，用于 /models 等子路径）
fn join_upstream(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    format!("{base}/{path}")
}

/// 简单字符串短 hash（8位十六进制），用于会话文件命名
fn simple_hash(input: &str) -> String {
    let mut h: u64 = 1469598103934665603;
    for b in input.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    format!("{:08x}", h)
}

/// 透传请求到上游（流式/非流式通用）
async fn forward(
    method: &str,
    upstream_base: &str,
    upstream_key: &str,
    path: &str,
    headers: HeaderMap,
    body: Option<axum::body::Bytes>,
    db: Option<&crate::database::Database>,
) -> Result<Response, ProxyError> {
    let full_url = join_upstream(upstream_base, path);
    // 使用 HTTP/1.1-only 客户端：部分上游网关（如 DeepSeek 的 openresty 网关）
    // 对 HTTP/2 请求返回 400，而 HTTP/1.1 请求正常。
    let client = super::http_client::get();

    let mut builder = match method {
        "GET" => client.get(&full_url),
        _ => client.post(&full_url),
    };

    // 透传请求头（跳过 hop-by-hop 与会被 reqwest 自动处理的头）
    // 注意：authorization 必须跳过——客户端带的是本地 Key，透传后再追加
    // 上游 Key 会导致上游收到两个 Authorization 头而被网关拒绝（400）。
    for (k, v) in headers.iter() {
        if matches!(
            k.as_str(),
            "host"
                | "content-length"
                | "content-encoding"
                | "accept-encoding"
                | "connection"
                | "transfer-encoding"
                | "keep-alive"
                | "upgrade"
                | "proxy-authorization"
                | "proxy-connection"
                | "te"
                | "trailer"
                | "authorization"
        ) {
            continue;
        }
        if let Ok(val) = v.to_str() {
            builder = builder.header(k.as_str(), val);
        }
    }
    builder = builder.header(header::AUTHORIZATION, format!("Bearer {upstream_key}"));

    let original_body = body.clone();

    // [relay-session] 暂时关闭会话记录写入（会话识别不准，后续再优化）
    let _ = original_body;

    if let Some(bytes) = body {
        // [desensitize] 入站脱敏：敏感实体 → 占位符（本机映射，明文不出本机）。
        // process_inbound 内部已判断隐私保护总开关，关闭时整包旁路。
        let out = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => {
                let mut masked = crate::desensitize::pipeline::process_inbound(value, "api-relay");
                // [desensitize] 本次触发了脱敏：在 messages 最前面注入占位符说明，
                // 让模型知道这些是脱敏标记、保持分析即可，不要当成缺失变量。
                let masked_str = serde_json::to_string(&masked).unwrap_or_default();
                if masked_str.contains("{(_") {
                    let note = serde_json::json!({
                        "role": "system",
                        "content": "对话中形如 {(_xxx_xxxx_xxxx_]} 的标记是已脱敏的实体，请直接当作正常名词理解并回答，原样保留标记，不要解释、不要评论、不要追问。"
                    });
                    if let Some(obj) = masked.as_object_mut() {
                        if let Some(msgs) = obj.get_mut("messages").and_then(|m| m.as_array_mut()) {
                            msgs.insert(0, note);
                        }
                    }
                }
                serde_json::to_vec(&masked).unwrap_or(bytes.to_vec())
            }
            Err(_) => bytes.to_vec(),
        };
        builder = builder.body(out);
    }

    let upstream_resp = builder
        .send()
        .await
        .map_err(|e| ProxyError::Internal(format!("转发到中转上游失败: {e}")))?;

    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();
    let is_sse = resp_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/event-stream"))
        .unwrap_or(false);

    let mut builder = Response::builder().status(status);
    for (k, v) in resp_headers.iter() {
        if matches!(
            k.as_str(),
            "content-encoding"
                | "content-length"
                | "connection"
                | "transfer-encoding"
                | "keep-alive"
                | "upgrade"
        ) {
            continue;
        }
        builder = builder.header(k.as_str(), v);
    }

    if is_sse {
        // [desensitize] SSE 流式还原：占位符 → 原文（行缓冲，跨 chunk 安全）
        use futures::TryStreamExt;
        let stream = upstream_resp
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
        if let Some(d) = db {
            let mut req_summary = String::new();
            let mut model = String::new();
            if let Some(ref b) = original_body {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(b) {
                    model = v.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
                    if let Some(msgs) = v.get("messages").and_then(|m| m.as_array()) {
                        for m in msgs.iter().rev() {
                            if m.get("role").and_then(|r| r.as_str()) == Some("user") {
                                if let Some(c) = m.get("content").and_then(|c| c.as_str()) {
                                    req_summary = c.chars().take(200).collect();
                                }
                                break;
                            }
                        }
                    }
                }
            }
            record_log(d, &model, "api-relay", &req_summary, "[流式响应]", status.as_u16() as i64, 0);
        }
        builder
            .body(Body::from_stream(RestoreStream::new(stream)))
            .map_err(|e| ProxyError::Internal(format!("构建中转响应失败: {e}")))
    } else {
        // [desensitize] 非流式响应还原：占位符 → 原文
        let full = upstream_resp
            .bytes()
            .await
            .map_err(|e| ProxyError::Internal(format!("读取上游响应失败: {e}")))?;
        // 请求记录：从原始（脱敏前）body 和原始（还原前）响应提取摘要
        if let Some(d) = db {
            let mut req_summary = String::new();
            let mut model = String::new();
            if let Some(ref b) = original_body {
                if let Ok(v) = serde_json::from_slice::<serde_json::Value>(b) {
                    model = v.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
                    if let Some(msgs) = v.get("messages").and_then(|m| m.as_array()) {
                        for m in msgs.iter().rev() {
                            if m.get("role").and_then(|r| r.as_str()) == Some("user") {
                                if let Some(c) = m.get("content").and_then(|c| c.as_str()) {
                                    req_summary = c.chars().take(200).collect();
                                }
                                break;
                            }
                        }
                    }
                }
            }
            let mut resp_summary = String::new();
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&full) {
                if let Some(c) = v.pointer("/choices/0/message/content").and_then(|c| c.as_str()) {
                    resp_summary = c.chars().take(200).collect();
                }
            }
            record_log(d, &model, "api-relay", &req_summary, &resp_summary, status.as_u16() as i64, 0);
        }

        let restored = crate::desensitize::pipeline::restore_bytes(&full);
        let body = axum::body::Body::from(restored);
        builder
            .body(body)
            .map_err(|e| ProxyError::Internal(format!("构建中转响应失败: {e}")))
    }
}

/// 处理 POST /relay/v1/chat/completions
pub async fn handle_relay_chat_completions(
    State(state): State<super::server::ProxyState>,
    request: Request<Body>,
) -> Result<Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    validate_local_key(state.db.as_ref(), &parts.headers)?;
    let (upstream_url, upstream_key) = get_upstream(state.db.as_ref())?;

    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("读取请求体失败: {e}")))?
        .to_bytes();

    forward(
        "POST",
        &upstream_url,
        &upstream_key,
        "chat/completions",
        parts.headers,
        Some(body_bytes),
        Some(state.db.as_ref()),
    )
    .await
}

/// 处理 GET /relay/v1/models
pub async fn handle_relay_models(
    State(state): State<super::server::ProxyState>,
    request: Request<Body>,
) -> Result<Response, ProxyError> {
    let (parts, req_body) = request.into_parts();
    validate_local_key(state.db.as_ref(), &parts.headers)?;
    let (upstream_url, upstream_key) = get_upstream(state.db.as_ref())?;

    // GET 请求通常无 body，但按透传原则仍携带
    let body_bytes = req_body
        .collect()
        .await
        .map_err(|e| ProxyError::Internal(format!("读取请求体失败: {e}")))?
        .to_bytes();

    forward(
        "GET",
        &upstream_url,
        &upstream_key,
        "models",
        parts.headers,
        (!body_bytes.is_empty()).then_some(body_bytes),
        None,
    )
    .await
}

/// 确保 401 响应使用统一错误格式（供鉴权失败时引用）
#[allow(dead_code)]
pub fn unauthorized() -> StatusCode {
    StatusCode::UNAUTHORIZED
}

/// 记录一次 API 中转请求（供前端会话/请求记录展示）
pub fn record_log(
    db: &crate::database::Database,
    model: &str,
    provider: &str,
    request_summary: &str,
    response_summary: &str,
    status: i64,
    latency_ms: i64,
) {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let _ = conn.execute(
        "INSERT INTO api_relay_log (created_at, model, provider, request_summary, response_summary, status, latency_ms)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        rusqlite::params![
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
            model,
            provider,
            request_summary,
            response_summary,
            status,
            latency_ms,
        ],
    );
}

/// 查询 API 中转请求记录（分页，倒序）
pub fn query_logs(db: &crate::database::Database, limit: i64, offset: i64) -> Vec<serde_json::Value> {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, created_at, model, provider, request_summary, response_summary, status, latency_ms
         FROM api_relay_log ORDER BY id DESC LIMIT ?1 OFFSET ?2",
    ) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![limit, offset], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "created_at": r.get::<_, i64>(1)?,
                "model": r.get::<_, String>(2)?,
                "provider": r.get::<_, String>(3)?,
                "request_summary": r.get::<_, String>(4)?,
                "response_summary": r.get::<_, String>(5)?,
                "status": r.get::<_, i64>(6)?,
                "latency_ms": r.get::<_, i64>(7)?,
            }))
        }) {
            for row in rows.flatten() {
                out.push(row);
            }
        }
    }
    out
}

/// 清空 API 中转请求记录
pub fn clear_logs(db: &crate::database::Database) {
    let conn = db.conn.lock().unwrap_or_else(|e| e.into_inner());
    let _ = conn.execute("DELETE FROM api_relay_log", []);
}