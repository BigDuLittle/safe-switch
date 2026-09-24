//! API 中转（Relay）相关命令
//!
//! - get_api_relay_info：返回本地入口信息（端口、本地 Key、当前上游供应商）
//! - regenerate_api_relay_key：重新生成本地 Key
//! 上游供应商的添加/编辑/切换复用统一供应商管理（ProviderList，app_type=api-relay）。
#![allow(
    clippy::all,
    dead_code,
    unused,
    unreachable_patterns,
    private_interfaces
)]

use serde_json::{json, Value};
use tauri::State;

use crate::proxy::relay;
use crate::AppState;

/// 获取 API 中转信息
#[tauri::command]
pub async fn get_api_relay_info(state: State<'_, AppState>) -> Result<Value, String> {
    let db = state.db.as_ref();
    let key = relay::get_or_create_local_key(db)?;

    // API 中转：不再自动启动，由前端开关控制
    let status = state
        .proxy_service
        .get_status()
        .await
        .map_err(|e| e.to_string())?;
    // 代理未运行时 status.port 为 0，回退到配置端口（默认 15721），保证页面 URL 可预期
    let port = if status.running && status.port != 0 {
        status.port
    } else {
        let config = state
            .proxy_service
            .get_config()
            .await
            .map_err(|e| e.to_string())?;
        config.listen_port
    };

    // 当前选中的上游供应商（providers 表 app_type=api-relay）
    let upstream = match db.get_current_provider("api-relay") {
        Ok(Some(id)) => match db.get_provider_by_id(&id, "api-relay") {
            Ok(Some(p)) => {
                let base_url = p
                    .settings_config
                    .get("baseUrl")
                    .or_else(|| p.settings_config.get("base_url"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let api_key_set = p
                    .settings_config
                    .get("apiKey")
                    .or_else(|| p.settings_config.get("api_key"))
                    .and_then(Value::as_str)
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(false);
                Some(json!({
                    "id": p.id,
                    "name": p.name,
                    "base_url": base_url,
                    "api_key_set": api_key_set,
                }))
            }
            Ok(None) => None,
            Err(e) => {
                log::warn!("读取 API 中转当前供应商失败: {e}");
                None
            }
        },
        Ok(None) => None,
        Err(e) => {
            log::warn!("读取 API 中转当前供应商失败: {e}");
            None
        }
    };

    Ok(json!({
        "running": status.running,
        "port": port,
        "address": status.address,
        "key": key,
        "upstream": upstream,
    }))
}

/// 重新生成本地 Key
#[tauri::command]
pub async fn regenerate_api_relay_key(state: State<'_, AppState>) -> Result<Value, String> {
    let key = relay::regenerate_local_key(state.db.as_ref())?;
    Ok(json!({ "key": key }))
}

/// 查询 API 中转请求记录（会话记录）
#[tauri::command]
pub async fn list_api_relay_logs(
    state: State<'_, AppState>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Value, String> {
    let rows = relay::query_logs(state.db.as_ref(), limit.unwrap_or(50), offset.unwrap_or(0));
    Ok(json!({ "logs": rows }))
}

/// 清空 API 中转请求记录
#[tauri::command]
pub async fn clear_api_relay_logs(state: State<'_, AppState>) -> Result<(), String> {
    relay::clear_logs(state.db.as_ref());
    Ok(())
}
