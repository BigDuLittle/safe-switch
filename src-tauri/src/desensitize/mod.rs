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
