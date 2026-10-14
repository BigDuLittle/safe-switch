//! [desensitize] 离线脱敏映射引擎
//!
//! 请求入站：识别敏感实体（正则 + 关键词语义匹配）→ 替换为全局唯一占位符 `<前缀><session>_<seq>__`
//! 响应出站：扫描占位符 → 查本地映射表 → 还原原文
//!
//! 安全模型：敏感数据不离开本机；映射表仅存本地 SQLite；
//! 占位符自包含会话短哈希，还原时无需请求上下文，天然适配流式/非流式两条响应路径。

pub mod commands;
pub mod mapper;
pub mod pipeline;
pub mod rules;
pub mod semantic;

use crate::database::Database;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

static DB: OnceLock<Arc<Database>> = OnceLock::new();

/// 应用启动时注册数据库引用（由 lib.rs setup 调用）
pub fn init(db: Arc<Database>) {
    let _ = DB.set(db);
    // 预热占位符前缀缓存（映射规则）
    if let Some(db) = DB.get() {
        refresh_prefix_cache(&mapper::get_placeholder_prefix(db));
    }
}

/// 首次启动时把安装包里的 bge 模型拷贝到 ~/.cc-switch/models/。
/// 已存在（且模型权重文件在）则跳过；拷贝失败仅记日志，不阻塞启动
/// （语义匹配会自动降级为纯字面匹配）。
pub fn ensure_bundled_model(resource_dir: Option<&Path>) {
    let Some(resource_dir) = resource_dir else {
        return;
    };
    let src_dir = resource_dir.join("models").join("bge-small-zh-v1.5");
    if !src_dir.exists() {
        log::debug!("bundled model dir not found: {}", src_dir.display());
        return;
    }
    let home = crate::config::get_home_dir();
    let dst_dir = home
        .join(".cc-switch")
        .join("models")
        .join("bge-small-zh-v1.5");
    let marker = dst_dir.join("model.safetensors");
    if marker.exists() {
        return;
    }
    if let Err(e) = copy_dir_recursive(&src_dir, &dst_dir) {
        log::warn!("拷贝内置模型到 {} 失败: {e}", dst_dir.display());
        return;
    }
    log::info!("内置模型已自举到 {}", dst_dir.display());
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

pub(crate) fn db() -> Option<&'static Arc<Database>> {
    DB.get()
}

pub const PLACEHOLDER_PREFIX: &str = "__SEC_";
pub const PLACEHOLDER_SUFFIX: &str = "__";

/// 占位符前缀缓存（映射规则：默认 __SEC_，可在「映射表管理」中修改）
static PREFIX_CACHE: OnceLock<RwLock<String>> = OnceLock::new();

/// 当前生效的占位符前缀（含尾下划线；未配置时为 __SEC_）
pub fn cached_prefix() -> String {
    let lock = PREFIX_CACHE.get_or_init(|| RwLock::new(PLACEHOLDER_PREFIX.to_string()));
    lock.read()
        .map(|g| g.clone())
        .unwrap_or_else(|e| e.into_inner().clone())
}

/// 更新前缀缓存（保存映射规则后调用）
pub fn refresh_prefix_cache(prefix: &str) {
    if let Some(lock) = PREFIX_CACHE.get() {
        if let Ok(mut g) = lock.write() {
            *g = prefix.to_string();
        }
    }
}

/// 判断字符串是否已经是脱敏占位符（跳过二次脱敏 / 避免误还原；
/// 兼容默认前缀与自定义前缀，已生成的旧占位符在新前缀下仍可识别）
pub fn is_placeholder(text: &str) -> bool {
    (text.starts_with("{(_") && text.ends_with("_]}"))
        || ((text.starts_with(PLACEHOLDER_PREFIX) || text.starts_with(&cached_prefix()))
            && text.ends_with(PLACEHOLDER_SUFFIX))
}

/// 对原文做打码显示（日志/审计用，不落原文）
pub fn mask_text(original: &str) -> String {
    let chars: Vec<char> = original.chars().collect();
    let len = chars.len();
    if len <= 2 {
        return "****".to_string();
    }
    if len <= 6 {
        return format!("{}****", chars[..1].iter().collect::<String>());
    }
    // 邮箱：保留 @ 前 2 位与域名
    if let Some(at) = original.find('@') {
        let local = &original[..at];
        let domain = &original[at..];
        let lc: Vec<char> = local.chars().collect();
        let head: String = lc[..2.min(lc.len())].iter().collect();
        return format!("{}****{}", head, domain);
    }
    // 常规：前 3 + **** + 后 2
    let head: String = chars[..3.min(len)].iter().collect();
    let tail: String = chars[len.saturating_sub(2)..].iter().collect();
    format!("{}****{}", head, tail)
}

/// 计算 additive 应用（OpenCode/OpenClaw/Hermes/Pi/Mcode）的脱敏代理 base_url。
///
/// 全局脱敏开关 + 该应用开关都开时，返回 `http://127.0.0.1:PORT/<app>/v1`；
/// 否则返回 None（调用方保持原直连 base_url 不变）。
///
/// 配置写入器在写 live 配置前调用：拿到 Some(url) 就把 provider 的 base_url 换成这个，
/// 请求过代理时按 path 前缀识别 app_type，走脱敏 pipeline。
pub fn desensitize_proxy_base_url(app: &str) -> Option<String> {
    let db = db()?;
    if !mapper::get_enabled(db) {
        return None;
    }
    if !mapper::get_app_enabled(db, app) {
        return None;
    }
    let (address, port) = db.get_proxy_client_endpoint_sync();
    let port = if port == 0 {
        crate::proxy::types::ProxyConfig::default().listen_port
    } else {
        port
    };
    let origin = crate::services::proxy::proxy_origin(&address, port);
    Some(format!("{origin}/{app}/v1"))
}

/// 把 provider config JSON 里的 base_url 类字段改写成脱敏代理地址。
/// 覆盖常见字段名：`baseURL` / `base_url` / `baseUrl`，以及 `options.baseURL`（OpenCode 风格）。
/// 返回是否改写过。全局/应用开关没开时直接返回 false，不动原配置。
pub fn rewrite_base_url_for_proxy(config: &mut serde_json::Value, app: &str) -> bool {
    let Some(proxy_url) = desensitize_proxy_base_url(app) else {
        return false;
    };
    let mut rewritten = false;
    if let Some(obj) = config.as_object_mut() {
        for key in ["baseURL", "base_url", "baseUrl"] {
            if let Some(v) = obj.get_mut(key) {
                if v.is_string() {
                    *v = serde_json::Value::String(proxy_url.clone());
                    rewritten = true;
                }
            }
        }
        if let Some(options) = obj.get_mut("options").and_then(|o| o.as_object_mut()) {
            for key in ["baseURL", "base_url", "baseUrl"] {
                if let Some(v) = options.get_mut(key) {
                    if v.is_string() {
                        *v = serde_json::Value::String(proxy_url.clone());
                        rewritten = true;
                    }
                }
            }
        }
    }
    rewritten
}

/// fork 统一 hook：在写入 additive 应用 live 配置前，按需把 provider 的 base_url 指回本地脱敏代理。
/// 返回 owned Provider（非 additive 应用原样 clone）。
/// 所有 additive 应用（OpenCode/OpenClaw/Hermes/Mcode/Pi）共用同一个改写入口，
/// 后续上游改各自的写入分支时不会再产生多处冲突。
pub fn prepare_provider_for_additive(
    original: &crate::provider::Provider,
    app_type: &crate::app_config::AppType,
) -> crate::provider::Provider {
    let mut provider = original.clone();
    if app_type.is_additive_mode() {
        rewrite_base_url_for_proxy(&mut provider.settings_config, app_type.as_str());
    }
    provider
}
