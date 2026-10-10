//! [desensitize] 内置 PII 规则（确定性通道）
//!
//! 6 类 27 条：身份信息 / 联系方式 / 财务数据 / 网络与系统 / 凭据与密钥 / 位置与其他。
//! 全部为确定性强的格式规则（正则 + 校验位 / Luhn / 前缀长度校验），命中即脱敏。
//! 模糊规则（人名/公司名/项目名/账号类/地址类等）不在此列，由语义通道（Phase 2）接管。
//! 规则开关持久化在 `desensitize_rules` 表（category 列分组）；未初始化时使用内置默认开关。
//!
//! 注意：Rust regex crate 不支持 look-around，所有规则使用无断言正则；
//! 片段边界（前后紧邻字符不为 ASCII 字母/数字）由 `scan_regex_filtered` 统一检查，
//! 保留段排除（内网 IP）用 `validate` 校验函数完成。
#![allow(
    clippy::all,
    dead_code,
    unused,
    unreachable_patterns,
    private_interfaces
)]

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

/// 内置规则定义
pub struct BuiltinRule {
    pub name: &'static str,
    pub entity_type: &'static str,
    /// 分组：identity / contact / financial / network / secrets / location
    pub category: &'static str,
    pub pattern: &'static str,
    /// 上下文正则时取捕获组（如「驾驶证：110xxx」只替换号码部分）
    pub capture: Option<usize>,
    pub enabled_default: bool,
    /// 命中后的确定性校验（Luhn / 身份证校验位 / 公网 IP 排除保留段）；None = 格式即命中
    pub validate: Option<fn(&str) -> bool>,
}

/// 类别显示顺序与中文名（前端分组用）
pub const CATEGORIES: &[(&str, &str)] = &[
    ("identity", "身份信息"),
    ("contact", "联系方式"),
    ("financial", "财务数据"),
    ("network", "网络与系统"),
    ("secrets", "凭据与密钥"),
    ("location", "位置与其他"),
];

pub const BUILTIN_RULES: &[BuiltinRule] = &[
    // ============ 身份信息 ============
    BuiltinRule {
        name: "中国大陆身份证号（含驾驶证号）",
        entity_type: "cn_id_card",
        category: "identity",
        pattern: r"[1-9]\d{5}(?:18|19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\d{3}[\dXx]",
        capture: None,
        enabled_default: true,
        validate: Some(cn_id_check),
    },
    BuiltinRule {
        name: "驾驶证号",
        entity_type: "cn_driver_license",
        category: "identity",
        pattern: r"(?:驾驶证号|驾驶证号码|驾照号|驾驶证)[:：\s]*([1-9]\d{16}[\dXx])",
        capture: Some(1),
        enabled_default: true,
        validate: Some(cn_id_check),
    },
    BuiltinRule {
        name: "中国大陆护照号",
        entity_type: "cn_passport",
        category: "identity",
        pattern: r"[EGD]\d{8}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "港澳通行证 / 台湾通行证",
        entity_type: "cn_entry_permit",
        category: "identity",
        pattern: r"[CHM]\d{8}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "国际社保号（美 SSN）",
        entity_type: "intl_ssn",
        category: "identity",
        pattern: r"\d{3}-\d{2}-\d{4}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    // ============ 联系方式 ============
    BuiltinRule {
        name: "中国大陆手机号",
        entity_type: "cn_phone",
        category: "contact",
        pattern: r"(?:\+?86[-\s]?)?1[3-9]\d{9}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "国际电话",
        entity_type: "intl_phone",
        category: "contact",
        pattern: r"\+\d{1,3}[\s.-]?\d{6,14}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "中国大陆固定电话",
        entity_type: "cn_tel",
        category: "contact",
        pattern: r"0\d{2,3}-?\d{7,8}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "电子邮箱",
        entity_type: "email",
        category: "contact",
        pattern: r"[\w.+-]+@[\w-]+\.[A-Za-z]{2,}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    // ============ 财务数据 ============
    BuiltinRule {
        name: "银行卡号",
        entity_type: "bank_card",
        category: "financial",
        pattern: r"\d{16,19}",
        capture: None,
        enabled_default: true,
        validate: Some(luhn_ok),
    },
    BuiltinRule {
        name: "国际信用卡（短卡）",
        entity_type: "credit_card",
        category: "financial",
        pattern: r"\d{13,15}",
        capture: None,
        enabled_default: true,
        validate: Some(luhn_ok),
    },
    BuiltinRule {
        name: "IBAN 国际银行账号",
        entity_type: "iban",
        category: "financial",
        pattern: r"[A-Z]{2}\d{2}[A-Z0-9]{11,30}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    // ============ 网络与系统 ============
    BuiltinRule {
        name: "内网 IPv4",
        entity_type: "intranet_ip",
        category: "network",
        pattern: r"(?:10|127)\.\d{1,3}\.\d{1,3}\.\d{1,3}|172\.(?:1[6-9]|2\d|3[01])\.\d{1,3}\.\d{1,3}|192\.168\.\d{1,3}\.\d{1,3}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "公网 IPv4",
        entity_type: "public_ip",
        category: "network",
        pattern: r"(?:(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)\.){3}(?:25[0-5]|2[0-4]\d|1\d\d|[1-9]?\d)",
        capture: None,
        enabled_default: true,
        validate: Some(is_public_ipv4),
    },
    BuiltinRule {
        name: "IPv6",
        entity_type: "ipv6",
        category: "network",
        pattern: r"(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}",
        capture: None,
        enabled_default: false,
        validate: None,
    },
    BuiltinRule {
        name: "内网域名",
        entity_type: "intranet_domain",
        category: "network",
        pattern: r"[\w-]+\.(?:corp|internal|lan|local|intranet)(?:\.(?:com|cn|net|local|top))?",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "MAC 地址",
        entity_type: "mac",
        category: "network",
        pattern: r"(?:[0-9a-fA-F]{2}[:-]){5}[0-9a-fA-F]{2}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "数据库连接串",
        entity_type: "db_uri",
        category: "network",
        pattern: r"(?:jdbc:[a-z0-9]+://|postgres(?:ql)?://|mysql://|mongodb(?:\+srv)?://|redis://|sqlite://)[\w:.@/?%&=+#-]*",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    // ============ 凭据与密钥 ============
    BuiltinRule {
        name: "OpenAI / 通用 API Key",
        entity_type: "openai_key",
        category: "secrets",
        pattern: r"sk-(?:proj-)?[A-Za-z0-9_\-]{20,}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "Anthropic Key",
        entity_type: "anthropic_key",
        category: "secrets",
        pattern: r"sk-ant-api[0-9]{2}-[A-Za-z0-9_\-]{20,}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "Google API Key",
        entity_type: "google_key",
        category: "secrets",
        pattern: r"AIza[0-9A-Za-z_\-]{35}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "GitHub Token",
        entity_type: "github_token",
        category: "secrets",
        pattern: r"gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "AWS Access Key",
        entity_type: "aws_key",
        category: "secrets",
        pattern: r"(?:AKIA|ASIA)[0-9A-Z]{16}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "JWT / Bearer Token",
        entity_type: "jwt",
        category: "secrets",
        pattern: r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "私钥块",
        entity_type: "private_key",
        category: "secrets",
        pattern: r"-----BEGIN [A-Z ]*PRIVATE KEY-----",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    BuiltinRule {
        name: "其他平台 Token",
        entity_type: "other_token",
        category: "secrets",
        pattern: r"(?:xox[baprs]-[A-Za-z0-9\-]{10,}|sk_live_[0-9a-zA-Z]{20,}|rk_live_[0-9a-zA-Z]{20,})",
        capture: None,
        enabled_default: true,
        validate: None,
    },
    // ============ 位置与其他 ============
    BuiltinRule {
        name: "中国车牌号",
        entity_type: "cn_plate",
        category: "location",
        pattern: r"[京津沪渝冀豫云辽黑湘皖鲁新苏浙赣鄂桂甘晋蒙陕吉闽贵粤青藏川宁琼][A-HJ-NP-Z][A-HJ-NP-Z0-9]{5}",
        capture: None,
        enabled_default: true,
        validate: None,
    },
];

/// 编译后的内置规则：Vec<(Regex, entity_type, name, capture, validate)>
pub static COMPILED_RULES: Lazy<
    Vec<(
        Regex,
        &'static str,
        &'static str,
        Option<usize>,
        Option<fn(&str) -> bool>,
    )>,
> = Lazy::new(|| {
    BUILTIN_RULES
        .iter()
        .filter(|r| r.enabled_default)
        .filter_map(|r| {
            Regex::new(r.pattern)
                .ok()
                .map(|re| (re, r.entity_type, r.name, r.capture, r.validate))
        })
        .collect()
});

/// Luhn 校验（银行卡 / 信用卡）
pub fn luhn_ok(s: &str) -> bool {
    let digits: Vec<u32> = s
        .chars()
        .filter(|c| c.is_ascii_digit())
        .filter_map(|c| c.to_digit(10))
        .collect();
    if digits.len() < 13 || digits.len() > 19 {
        return false;
    }
    let mut sum = 0u32;
    let mut double = false;
    for &d in digits.iter().rev() {
        if double {
            let dd = d * 2;
            sum += if dd > 9 { dd - 9 } else { dd };
        } else {
            sum += d;
        }
        double = !double;
    }
    sum % 10 == 0
}

/// 中国大陆身份证 18 位校验位验证
pub fn cn_id_check(s: &str) -> bool {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() != 18 {
        return false;
    }
    if !s[..17].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let ws: [u32; 17] = [7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2];
    let map: &[u8] = b"10X98765432";
    let sum: u32 = ws
        .iter()
        .zip(b[..17].iter())
        .map(|(w, &d)| w * (d - b'0') as u32)
        .sum();
    let expect = map[(sum % 11) as usize];
    let last = if b[17] == b'x' { b'X' } else { b[17] };
    last == expect
}

/// 公网 IPv4 判定：排除内网/回环保留段
pub fn is_public_ipv4(s: &str) -> bool {
    let octets: Vec<&str> = s.split('.').collect();
    if octets.len() != 4 {
        return false;
    }
    let nums: Vec<u32> = octets.iter().filter_map(|o| o.parse().ok()).collect();
    if nums.len() != 4 {
        return false;
    }
    let (a, b) = (nums[0], nums[1]);
    !(a == 10
        || a == 127
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 0)
        || a >= 224)
}

/// 正则通道扫描（按启用的 entity_type 集合过滤；集合为空 = 全部跳过）
/// 命中片段默认取整体；capture 指定时取捕获组（上下文正则只替换实体部分）；
/// 统一检查片段前后紧邻字符不为 ASCII 字母/数字（等价于词边界，规避 Rust regex 无 look-around）；
/// validate 失败（校验位 / Luhn / 保留段）的候选剔除。
pub fn scan_regex_filtered(
    text: &str,
    enabled: &HashSet<String>,
) -> Vec<(usize, usize, String, String, f32)> {
    if enabled.is_empty() {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut hits = Vec::new();
    for (re, entity_type, name, capture, validate) in COMPILED_RULES.iter() {
        if !enabled.contains(*entity_type) {
            continue;
        }
        for caps in re.captures_iter(text) {
            let m = match capture {
                Some(idx) => match caps.get(*idx) {
                    Some(m) => m,
                    None => continue,
                },
                None => caps.get(0).expect("group 0 exists"),
            };
            // 前后边界：紧邻字符不得为 ASCII 字母/数字
            let before_ok = m.start() == 0 || !bytes[m.start() - 1].is_ascii_alphanumeric();
            let after_ok = m.end() == bytes.len() || !bytes[m.end()].is_ascii_alphanumeric();
            if !before_ok || !after_ok {
                continue;
            }
            let seg = &text[m.start()..m.end()];
            if let Some(v) = validate {
                if !v(seg) {
                    continue;
                }
            }
            hits.push((
                m.start(),
                m.end(),
                entity_type.to_string(),
                name.to_string(),
                1.0,
            ));
        }
    }
    hits
}

/// 读取内置规则的最终启用状态：DB 有记录用记录值，无记录用内置默认
pub fn builtin_enabled_set(db: &crate::database::Database) -> HashSet<String> {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    let mut set = HashSet::new();
    for r in BUILTIN_RULES {
        let enabled = conn
            .query_row(
                "SELECT enabled FROM desensitize_rules
                 WHERE entity_type = ?1 AND rule_type = 'builtin'",
                [r.entity_type],
                |row| row.get::<_, i64>(0),
            )
            .map(|v| v != 0)
            .unwrap_or(r.enabled_default);
        if enabled {
            set.insert(r.entity_type.to_string());
        }
    }
    set
}

/// 返回默认启用（enabled_default=true）的全部 entity_type（测试用）
#[cfg(test)]
pub fn all_default_enabled() -> HashSet<String> {
    BUILTIN_RULES
        .iter()
        .filter(|r| r.enabled_default)
        .map(|r| r.entity_type.to_string())
        .collect()
}

/// 按 entity_type 查内置规则元信息（name/pattern/category），供开关持久化用
pub fn builtin_rule_meta(entity_type: &str) -> Option<(&'static str, &'static str, &'static str)> {
    BUILTIN_RULES
        .iter()
        .find(|r| r.entity_type == entity_type)
        .map(|r| (r.name, r.pattern, r.category))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(s: &str) -> String {
        s.to_string()
    }

    #[test]
    fn phone_matches() {
        let hits = scan_regex_filtered("我的手机号是13800138000，请处理", &all_default_enabled());
        assert!(!hits.is_empty());
        let (s, e, t, _, _) = &hits[0];
        assert_eq!(
            &text_of("我的手机号是13800138000，请处理")[*s..*e],
            "13800138000"
        );
        assert_eq!(t, "cn_phone");
    }

    #[test]
    fn cn_phone_with_prefix() {
        let hits = scan_regex_filtered("请致电 +86 13912345678 联系", &all_default_enabled());
        assert!(hits.iter().any(|h| h.2 == "cn_phone"));
    }

    #[test]
    fn id_card_valid_check() {
        // 11010519491231002X 为公开校验示例（校验位 X）
        assert!(cn_id_check("11010519491231002X"));
        // 校验位错误的 18 位号不命中
        let hits = scan_regex_filtered("证件 110105194912310021", &all_default_enabled());
        assert!(!hits.iter().any(|h| h.2 == "cn_id_card"));
    }

    #[test]
    fn driver_license_capture_only_number() {
        let hits = scan_regex_filtered("驾驶证号：11010519491231002X 请查", &all_default_enabled());
        let m = hits.iter().find(|h| h.2 == "cn_driver_license");
        assert!(m.is_some());
        let (s, e, _, _, _) = m.unwrap();
        assert_eq!(
            &text_of("驾驶证号：11010519491231002X 请查")[*s..*e],
            "11010519491231002X"
        );
    }

    #[test]
    fn email_matches() {
        let hits = scan_regex_filtered("联系 liwei@corp.com 即可", &all_default_enabled());
        assert!(hits.iter().any(|h| h.2 == "email"));
    }

    #[test]
    fn bank_card_luhn() {
        // 已知 Luhn 合法卡号示例：4111111111111111（Visa 测试号）
        assert!(luhn_ok("4111111111111111"));
        // 非法：连续 16 个 1 不通过
        assert!(!luhn_ok("1111111111111111"));
        let hits = scan_regex_filtered("卡号 4111111111111111 已录入", &all_default_enabled());
        assert!(hits.iter().any(|h| h.2 == "bank_card"));
        let hits2 = scan_regex_filtered("卡号 1111111111111111 已录入", &all_default_enabled());
        assert!(!hits2.iter().any(|h| h.2 == "bank_card"));
    }

    #[test]
    fn secret_keys_match() {
        let text =
            "配置 sk-proj-abcdef1234567890abcdef 与 ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let hits = scan_regex_filtered(text, &all_default_enabled());
        assert!(hits.iter().any(|h| h.2 == "openai_key"));
        assert!(hits.iter().any(|h| h.2 == "github_token"));
    }

    #[test]
    fn internal_ip_and_public_ip_distinct() {
        let hits = scan_regex_filtered("内网 10.12.34.56 公网 8.8.8.8", &all_default_enabled());
        assert!(hits.iter().any(|h| h.2 == "intranet_ip"
            && &text_of("内网 10.12.34.56 公网 8.8.8.8")[h.0..h.1] == "10.12.34.56"));
        assert!(hits.iter().any(|h| h.2 == "public_ip"
            && &text_of("内网 10.12.34.56 公网 8.8.8.8")[h.0..h.1] == "8.8.8.8"));
        // 10.x 不应命中 public_ip
        assert!(!hits.iter().any(|h| h.2 == "public_ip" && h.0 < 8));
    }

    #[test]
    fn cn_plate_matches() {
        let hits = scan_regex_filtered("车牌 京A12345 已备案", &all_default_enabled());
        assert!(hits.iter().any(|h| h.2 == "cn_plate"));
    }

    #[test]
    fn no_false_positive_on_placeholder() {
        let hits = scan_regex_filtered("已替换为 __SEC_ab12cd_0001__", &all_default_enabled());
        assert!(hits.is_empty());
    }

    #[test]
    fn disabled_rules_skipped() {
        let enabled: HashSet<String> = HashSet::from(["email".to_string()]);
        let hits = scan_regex_filtered("手机 13800138000 邮箱 a@b.com", &enabled);
        assert!(hits.iter().all(|h| h.2 == "email"));
    }

    #[test]
    fn boundary_prevents_partial_long_number() {
        // 20 位数字串：手机号规则不得命中其子串（前后为数字）
        let hits = scan_regex_filtered("编号123456789012345678901", &all_default_enabled());
        assert!(!hits.iter().any(|h| h.2 == "cn_phone"));
    }

    #[test]
    fn ipv6_off_by_default() {
        let hits = scan_regex_filtered("地址 fe80::1:2:3:4:5:6", &all_default_enabled());
        assert!(!hits.iter().any(|h| h.2 == "ipv6"), "ipv6 默认关闭");
    }
}
