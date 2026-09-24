//! API 中转会话记录 provider
//!
//! 会话文件存放于 ~/.cc-switch/relay-sessions/{conversation_hash}.jsonl
//! 每行一条 JSON 记录：{"type":"message","role":"user|assistant","content":"...","timestamp":ms}
#![allow(
    clippy::all,
    dead_code,
    unused,
    unreachable_patterns,
    private_interfaces
)]

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::session_manager::{SessionMessage, SessionMeta};

const PROVIDER_ID: &str = "relay";

/// 会话文件目录：~/.cc-switch/relay-sessions/
pub fn sessions_dir() -> PathBuf {
    crate::config::get_app_config_dir().join("relay-sessions")
}

/// 扫描所有 relay 会话文件
pub fn scan_sessions() -> Vec<SessionMeta> {
    let dir = sessions_dir();
    if !dir.exists() {
        return Vec::new();
    }
    let mut sessions = Vec::new();
    let mut entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    while let Some(Ok(entry)) = entries.next() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
            continue;
        }
        if let Some(meta) = parse_session(&path) {
            sessions.push(meta);
        }
    }
    sessions
}

fn parse_session(path: &Path) -> Option<SessionMeta> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut first_user: Option<String> = None;
    let mut created_at: Option<i64> = None;
    let mut last_active: Option<i64> = None;
    let mut last_msg: Option<String> = None;

    for line in reader.lines().map_while(|l| l.ok()) {
        let val: Value = serde_json::from_str(&line).ok()?;
        let ts = val.get("timestamp").and_then(Value::as_i64);
        if created_at.is_none() {
            created_at = ts;
        }
        last_active = ts;

        let role = val.get("role").and_then(Value::as_str).unwrap_or("");
        let content = val.get("content").and_then(Value::as_str).unwrap_or("");
        if role == "user" && first_user.is_none() && !content.is_empty() {
            first_user = Some(truncate(content, 40));
        }
        last_msg = Some(content.to_string());
    }

    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let session_id = format!("relay-{}", file_name);
    let title = first_user.unwrap_or_else(|| "API 中转会话".to_string());
    let summary = last_msg.map(|s| truncate(&s, 80));

    Some(SessionMeta {
        provider_id: PROVIDER_ID.to_string(),
        session_id,
        title: Some(title),
        summary,
        project_dir: None,
        created_at,
        last_active_at: last_active,
        source_path: Some(path.to_string_lossy().to_string()),
        resume_command: None,
    })
}

pub fn load_messages(path: &Path) -> Result<Vec<SessionMessage>, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open: {e}"))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    for line in reader.lines().map_while(|l| l.ok()) {
        let val: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let role = val
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let content = val
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if role.is_empty() || content.is_empty() {
            continue;
        }
        let ts = val.get("timestamp").and_then(Value::as_i64);
        messages.push(SessionMessage { role, content, ts });
    }
    Ok(messages)
}

pub fn delete_session(path: &Path) -> Result<bool, String> {
    std::fs::remove_file(path).map_err(|e| format!("Failed to delete: {e}"))?;
    Ok(true)
}

/// 追加一条消息到会话文件
///
/// `conversation_hash`：第一条 user 消息的短 hash（文件名校验）
/// `role`："user" 或 "assistant"
/// `content`：消息正文
pub fn append_message(conversation_hash: &str, role: &str, content: &str) {
    if conversation_hash.is_empty() || content.trim().is_empty() {
        return;
    }
    let dir = sessions_dir();
    if !dir.exists() {
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
    }
    let path = dir.join(format!("{}.jsonl", conversation_hash));
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let entry = serde_json::json!({
        "type": "message",
        "role": role,
        "content": content,
        "timestamp": ts,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{}", entry);
    }
}

fn truncate(s: &str, n: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        s.to_string()
    } else {
        let t: String = chars[..n].iter().collect();
        format!("{t}…")
    }
}
