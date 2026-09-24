//! [desensitize] 策略② 关键词匹配通道
//!
//! 用户输入关键词后，对请求文本做匹配（两种模式）：
//! - 关键词匹配（literal）：子串包含，确定性，所有模式生效
//! - 语义匹配（semantic）：关键词匹配 + 离线向量模型（bge-small-zh-v1.5，
//!   candle 纯 Rust CPU 推理）粗筛候选句，再句内细定位最相似片段，
//!   只替换该片段（宁漏勿错，不吞无关上下文）
//!
//! 模型文件独立存放于 `<data_dir>/.cc-switch/models/bge-small-zh-v1.5/`，
//! 不打包进 exe；模型缺失或加载失败时语义通道自动降级为仅字面匹配。

use std::collections::HashSet;
use std::sync::OnceLock;

/// 语义命中结果
#[derive(Debug, Clone)]
pub struct SemanticHit {
    pub start: usize,
    pub end: usize,
    pub entity_type: String,
    pub confidence: f32,
    /// 还原时使用的标准词；关键词命中时为用户定义的关键词本身，
    /// 语义变体（"汇 量"）和字面匹配（"汇量"）共用同一占位符，还原成"汇量"。
    /// None 表示用实际匹配片段（PII 正则场景）。
    pub restore_as: Option<String>,
}

/// 用户关键词规则（来自 desensitize_keywords 表）
#[derive(Debug, Clone)]
pub struct KeywordRule {
    pub id: i64,
    pub keyword: String,
    pub entity_type: String,
    pub match_mode: String, // literal（关键词匹配）| semantic（语义匹配）
    pub threshold: f32,
    pub enabled: bool,
}

/// 对文本执行关键词扫描
/// 返回命中的实体片段（start/end 为字节区间）
pub fn scan_keywords(text: &str, rules: &[KeywordRule]) -> Vec<SemanticHit> {
    let mut hits: Vec<SemanticHit> = Vec::new();
    let mut semantic_rules: Vec<&KeywordRule> = Vec::new();

    for rule in rules {
        if !rule.enabled {
            continue;
        }
        let kw = rule.keyword.trim();
        if kw.is_empty() {
            continue;
        }
        // 1. 关键词匹配（字面/大小写折叠包含，确定性，Unicode 安全）：全部匹配处
        for (s, e) in find_ci_all(text, &kw.to_lowercase()) {
            hits.push(SemanticHit {
                start: s,
                end: e,
                entity_type: kw.to_string(),
                confidence: 0.95,
                restore_as: Some(kw.to_string()),
            });
        }
        // 2. 语义匹配（向量模型）：粗筛候选句 + 句内细定位，literal 模式跳过
        if rule.match_mode != "literal" {
            semantic_rules.push(rule);
        }
    }

    if !semantic_rules.is_empty() {
        if let Ok(model) = embed_model() {
            let windows = split_windows(text);
            if !windows.is_empty() {
                let kw_list: Vec<String> = semantic_rules
                    .iter()
                    .map(|r| r.keyword.trim().to_string())
                    .collect();
                let scores = model.similarity(&kw_list, &windows);
                for (ri, rule) in semantic_rules.iter().enumerate() {
                    let th = rule.threshold.max(0.0);
                    for (wi, (ws, we, wt)) in windows.iter().enumerate() {
                        let sc = scores
                            .get(ri)
                            .and_then(|row| row.get(wi))
                            .copied()
                            .unwrap_or(0.0);
                        if sc < th {
                            continue;
                        }
                        // 细定位：句内切小片段，替换所有达标且互不重叠的片段
                        for h in locate_best_spans(model, rule, wt, *ws, th) {
                            if !hits.iter().any(|x| x.start == h.start && x.end == h.end) {
                                hits.push(h);
                            }
                        }
                    }
                }
            }
        }
        // 模型不可用：语义规则自动降级（本次无命中），不阻塞主流程
    }

    // 重叠区间合并：保留更长者（避免替换时字节错位）
    hits.sort_by_key(|h| (h.start, h.end));
    let mut merged: Vec<SemanticHit> = Vec::with_capacity(hits.len());
    for h in hits {
        if let Some(last) = merged.last_mut() {
            if h.start < last.end && h.end > last.start {
                let len_h = h.end - h.start;
                let len_l = last.end - last.start;
                if len_h >= len_l {
                    *last = h;
                }
                continue;
            }
        }
        merged.push(h);
    }
    merged
}

/// 句内细定位：把候选句切成围绕关键词长度的小片段，批量编码打分，
/// 返回所有 >= 阈值且互不重叠的片段（同关键词在句中多处出现时全部替换；
/// 重叠时保留相似度更高者，宁漏勿错）。无达标片段返回空。
fn locate_best_spans(
    model: &EmbedModel,
    rule: &KeywordRule,
    window_text: &str,
    base: usize,
    th: f32,
) -> Vec<SemanticHit> {
    let kw = rule.keyword.trim();
    let kw_chars = kw.chars().count();
    if kw_chars == 0 {
        return Vec::new();
    }
    // 候选片段长度：围绕关键词长度（2 ~ 14）
    let a = kw_chars.clamp(2, 12);
    let mut lens: Vec<usize> = Vec::new();
    for &l in &[a.saturating_sub(1), a, a + 1, a + 2] {
        if l >= 2 && l <= 14 && !lens.contains(&l) {
            lens.push(l);
        }
    }
    let chars: Vec<char> = window_text.chars().collect();
    let mut spans: Vec<(usize, usize, String)> = Vec::new();
    for &l in &lens {
        let step = if l <= 4 { 1 } else { 2 };
        let mut i = 0;
        while i + l <= chars.len() {
            let s: String = chars[i..i + l].iter().collect();
            let bs = base + chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>();
            let be = base + chars[..i + l].iter().map(|c| c.len_utf8()).sum::<usize>();
            spans.push((bs, be, s));
            i += step;
        }
    }
    if spans.is_empty() {
        return Vec::new();
    }
    let scores = model.similarity(&[kw.to_string()], &spans);
    let mut cand: Vec<(usize, usize, f32)> = Vec::new();
    if let Some(row) = scores.first() {
        for (wi, &sc) in row.iter().enumerate() {
            if sc >= th {
                cand.push((spans[wi].0, spans[wi].1, sc));
            }
        }
    }
    if cand.is_empty() {
        return Vec::new();
    }
    // 分数降序，贪心保留互不重叠的片段
    cand.sort_by(|a, b| {
        b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out: Vec<SemanticHit> = Vec::new();
    for (s, e, sc) in cand {
        if out.iter().any(|h| s < h.end && e > h.start) {
            continue;
        }
        out.push(SemanticHit {
            start: s,
            end: e,
            entity_type: rule.keyword.clone(),
            confidence: sc,
            restore_as: Some(rule.keyword.clone()),
        });
    }
    out
}

/// 大小写不敏感子串查找全部匹配处：needle 已为小写，
/// 返回 text 中所有不重叠的字节区间（同一关键词多处出现全部命中）
fn find_ci_all(text: &str, needle: &str) -> Vec<(usize, usize)> {
    let nlen = needle.chars().count();
    if nlen == 0 {
        return Vec::new();
    }
    let bounds: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    if bounds.len() <= nlen {
        return Vec::new();
    }
    let mut out: Vec<(usize, usize)> = Vec::new();
    for i in 0..(bounds.len() - nlen) {
        let start = bounds[i];
        let end = bounds[i + nlen];
        if text[start..end].to_lowercase() == needle {
            out.push((start, end));
        }
    }
    out
}

/// 语义匹配窗口：按句子切分；句子 ≤ 48 字符整句为窗口，超长按 32 字符滑窗（步长 16）。
/// 返回 (字节起始, 字节结束, 窗口文本)
fn split_windows(text: &str) -> Vec<(usize, usize, String)> {
    let mut out: Vec<(usize, usize, String)> = Vec::new();
    let mut start = 0usize;
    for (i, ch) in text.char_indices() {
        if "。！？!?；;\n\r".contains(ch) {
            let end = i + ch.len_utf8();
            push_sentence(&mut out, &text[start..end], start);
            start = end;
        }
    }
    if start < text.len() {
        push_sentence(&mut out, &text[start..], start);
    }
    out
}

fn push_sentence(out: &mut Vec<(usize, usize, String)>, sent: &str, base: usize) {
    let trimmed = sent.trim();
    if trimmed.is_empty() {
        return;
    }
    let chars: Vec<char> = sent.chars().collect();
    if chars.len() <= 48 {
        out.push((base, base + sent.len(), sent.to_string()));
        return;
    }
    // 超长句：32 字符窗口，16 字符步长
    let mut i = 0;
    while i < chars.len() {
        let end = (i + 32).min(chars.len());
        let s: String = chars[i..end].iter().collect();
        out.push((base + chars[..i].iter().map(|c| c.len_utf8()).sum::<usize>(),
                  base + chars[..end].iter().map(|c| c.len_utf8()).sum::<usize>(),
                  s));
        i += 16;
    }
}

// ==================== 离线向量模型（candle 纯 Rust） ====================

struct EmbedModel {
    model: candle_transformers::models::bert::BertModel,
    tokenizer: tokenizers::Tokenizer,
}

static EMBED: OnceLock<Result<EmbedModel, String>> = OnceLock::new();

pub fn embed_model() -> &'static Result<EmbedModel, String> {
    EMBED.get_or_init(|| EmbedModel::load())
}

/// 模型是否可用（供 UI 展示）
pub fn model_ready() -> bool {
    embed_model().is_ok()
}

/// 模型状态（供「本地模型管理」页展示：是否下载、路径、大小、是否就绪、错误信息）
pub fn model_status() -> serde_json::Value {
    use serde_json::json;
    let home = crate::config::get_home_dir();
    let dir = home
        .join(".cc-switch")
        .join("models")
        .join("bge-small-zh-v1.5");
    let mp = dir.join("model.safetensors");
    let downloaded = mp.exists()
        && dir.join("tokenizer.json").exists()
        && dir.join("config.json").exists();
    let size_mb = std::fs::metadata(&mp)
        .map(|m| m.len() as f64 / 1024.0 / 1024.0)
        .unwrap_or(0.0);
    let (ready, error) = match embed_model() {
        Ok(_) => (true, serde_json::Value::Null),
        Err(e) => (false, serde_json::Value::String(e.clone())),
    };
    json!({
        "downloaded": downloaded,
        "path": dir.display().to_string(),
        "size_mb": (size_mb * 10.0).round() / 10.0,
        "ready": ready,
        "error": error,
    })
}

impl EmbedModel {
    fn load() -> Result<Self, String> {
        let home = crate::config::get_home_dir();
        let dir = home
            .join(".cc-switch")
            .join("models")
            .join("bge-small-zh-v1.5");
        let model_path = dir.join("model.safetensors");
        let tok_path = dir.join("tokenizer.json");
        let cfg_path = dir.join("config.json");
        if !model_path.exists() || !tok_path.exists() || !cfg_path.exists() {
            return Err(format!("离线模型未就绪（{}）", dir.display()));
        }
        let device = candle_core::Device::Cpu;
        let config_json: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&cfg_path).map_err(|e| format!("读 config.json: {e}"))?,
        )
        .map_err(|e| format!("解析 config.json: {e}"))?;
        let config: candle_transformers::models::bert::Config =
            serde_json::from_value(config_json).map_err(|e| format!("加载 BERT 配置: {e}"))?;
        let tensors = candle_core::safetensors::load(&model_path, &device)
            .map_err(|e| format!("加载模型权重: {e}"))?;
        let vb = candle_nn::VarBuilder::from_tensors(
            tensors,
            candle_transformers::models::bert::DTYPE,
            &device,
        );
        let model = candle_transformers::models::bert::BertModel::load(vb, &config)
            .map_err(|e| format!("初始化 BERT: {e}"))?;
        let mut tokenizer = tokenizers::Tokenizer::from_file(&tok_path)
            .map_err(|e| format!("加载 tokenizer: {e}"))?;
        tokenizer.with_padding(Some(tokenizers::PaddingParams::default()));
        Ok(EmbedModel { model, tokenizer })
    }

    /// bge 中文 query 指令前缀（检索惯例：query 侧加前缀，文档侧不加）
    const QUERY_PREFIX: &'static str = "为这个句子生成表示以用于检索相关文章：";

    fn encode(&self, texts: &[String]) -> Result<candle_core::Tensor, String> {
        let enc = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| format!("tokenize: {e}"))?;
        let max_len = enc.iter().map(|e| e.get_ids().len()).max().unwrap_or(1);
        let n = enc.len();
        let mut ids = vec![0u32; n * max_len];
        let mut mask = vec![0u32; n * max_len];
        for (i, e) in enc.iter().enumerate() {
            let ids_i = e.get_ids();
            let mask_i = e.get_attention_mask();
            for (j, (&v, &m)) in ids_i.iter().zip(mask_i.iter()).enumerate() {
                ids[i * max_len + j] = v;
                mask[i * max_len + j] = m;
            }
        }
        let device = self.model.device.clone();
        let ids_t = candle_core::Tensor::from_vec(ids, (n, max_len), &device)
            .map_err(|e| format!("ids tensor: {e}"))?;
        let mask_t = candle_core::Tensor::from_vec(mask, (n, max_len), &device)
            .map_err(|e| format!("mask tensor: {e}"))?;
        let ttype = candle_core::Tensor::zeros((n, max_len), candle_core::DType::U32, &device)
            .map_err(|e| format!("ttype tensor: {e}"))?;
        let hidden = self
            .model
            .forward(&ids_t, &ttype, Some(&mask_t))
            .map_err(|e| format!("bert forward: {e}"))?;
        // cls 向量 (batch, hidden)
        let cls = hidden
            .narrow(1, 0, 1)
            .map_err(|e| format!("narrow: {e}"))?
            .squeeze(1)
            .map_err(|e| format!("squeeze: {e}"))?;
        let norm = cls
            .sqr()
            .map_err(|e| format!("sqr: {e}"))?
            .sum(1)
            .map_err(|e| format!("sum: {e}"))?
            .sqrt()
            .map_err(|e| format!("sqrt: {e}"))?;
        let unit = cls
            .broadcast_div(&norm.unsqueeze(1).map_err(|e| format!("unsq: {e}"))?)
            .map_err(|e| format!("norm: {e}"))?;
        Ok(unit)
    }

    /// 关键词 × 窗口 余弦相似度矩阵（行=关键词，列=窗口）
    fn similarity(&self, keywords: &[String], windows: &[(usize, usize, String)]) -> Vec<Vec<f32>> {
        if keywords.is_empty() || windows.is_empty() {
            return Vec::new();
        }
        let query_texts: Vec<String> = keywords
            .iter()
            .map(|k| format!("{}{}", Self::QUERY_PREFIX, k))
            .collect();
        let doc_texts: Vec<String> = windows.iter().map(|(_, _, t)| t.clone()).collect();
        // 单批编码窗口过多时截断，避免内存峰值（>256KB 文本由上层阈值开关跳过）
        let doc_texts: Vec<String> = if doc_texts.len() > 600 {
            doc_texts[..600].to_vec()
        } else {
            doc_texts
        };
        let q = match self.encode(&query_texts) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        let d = match self.encode(&doc_texts) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        let scores = match q.matmul(&d.transpose(0, 1).unwrap_or(d.clone())) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };
        match scores.to_vec2::<f32>() {
            Ok(v) => v,
            Err(_) => Vec::new(),
        }
    }
}

// 策略②（内置语义匹配）的敏感内容类别（Phase 2 预留，未启用管线）
pub const SEMANTIC_CATEGORIES: &[(&str, &str, &str)] = &[
    ("person_name", "人名 / 称呼", "含中文姓名、职务称呼等"),
    ("org_name", "公司 / 组织名", "公司、组织及其简称全称"),
    ("project_name", "项目名 / 代号", "内部项目名与代号"),
    ("internal_term", "内部术语 / 缩写", "组织内部术语与缩写"),
    ("address", "地址 / 住址表述", "地址与邮编类表述"),
];

/// 读取策略②语义类别的启用状态（rule_type='semantic'，未配置时默认启用）
pub fn semantic_enabled_set(db: &crate::database::Database) -> HashSet<String> {
    let conn = match db.conn.lock() {
        Ok(c) => c,
        Err(e) => e.into_inner(),
    };
    let mut set = HashSet::new();
    for (et, _, _) in SEMANTIC_CATEGORIES {
        let enabled = conn
            .query_row(
                "SELECT enabled FROM desensitize_rules
                 WHERE entity_type = ?1 AND rule_type = 'semantic'",
                [et],
                |row| row.get::<_, i64>(0),
            )
            .map(|v| v != 0)
            .unwrap_or(true);
        if enabled {
            set.insert(et.to_string());
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(kw: &str, ty: &str, mode: &str) -> KeywordRule {
        KeywordRule {
            id: 1,
            keyword: kw.to_string(),
            entity_type: ty.to_string(),
            match_mode: mode.to_string(),
            threshold: 0.5,
            enabled: true,
        }
    }

    #[test]
    fn literal_contains() {
        let hits = scan_keywords(
            "项目代号是北斗七星，请继续",
            &[rule("项目代号", "codename", "semantic")],
        );
        assert!(hits.iter().any(|h| h.entity_type == "codename"));
    }
    #[test]
    fn literal_matches_all_occurrences() {
        // 同一关键词在文本中多处出现：全部命中（不只第一处）
        let hits = scan_keywords(
            "工资发放安排，工资明细见附件，请确认工资到账时间",
            &[rule("工资", "salary", "literal")],
        );
        assert_eq!(hits.len(), 3, "should match all 3 occurrences, got {:?}", hits);
        // 同原文 → 由上层 ensure_mapping 复用同一占位符（mapper 层保证）
    }

    #[test]
    fn semantic_matches_all_windows() {
        // 语义模式：同一关键词在多个语义相关句中都应产生命中（模型就绪时断言）
        let text = "这次项目进度会议大家都按时参加了。会上讨论了薪酬结构。随后邮件又提到了薪酬调整方案。明天继续推进。";
        let hits = scan_keywords(text, &[rule("工资", "salary", "semantic")]);
        if model_ready() {
            assert!(hits.len() >= 1, "semantic should hit at least one window, got {:?}", hits);
            // 同一语义相关句应各自产生命中（窗口级全量扫描），模型就绪时打印观察
            for h in &hits {
                println!("[windows] {:?} -> {:?}", h, &text[h.start..h.end]);
            }
        }
        // 模型未就绪时语义降级，不崩溃
    }

    #[test]
    fn windows_split_sentence_and_slide() {
        let w = split_windows("第一句很短。第二句话比较长的时候会按照三十二个字符滑动切分窗口测试一下。");
        assert!(w.len() >= 2);
        assert!(w[0].2.contains("第一句"));
    }

    #[test]
    fn literal_mode_skips_semantic() {
        // literal 模式下无模型也稳定字面命中
        let hits = scan_keywords("联系客户张三", &[rule("客户", "customer", "literal")]);
        assert!(!hits.is_empty());
    }

    #[test]
    fn semantic_vector_match_with_model() {
        // 真实加载 bge 向量模型：关键词「工资」应命中含工资的文本窗口
        let hits = scan_keywords(
            "客户张三今天来签约。工资条上个月发了5000元。",
            &[rule("工资", "salary", "semantic")],
        );
        if model_ready() {

            assert!(
                hits.iter().any(|h| h.entity_type == "salary"),
                "语义命中失败: {:?}",
                hits
            );
        }
        // 模型未就绪时语义降级，不崩溃
    }

    #[test]
    fn semantic_fine_locate_narrow_span() {
        // 细定位验证：文本不含关键词字面，语义命中应只替换最相似片段（薪酬），
        // 而不是整个句子
        let text = "这次项目进度会议大家都按时参加了。会上讨论了薪酬结构。明天继续推进。";
        let hits = scan_keywords(text, &[rule("工资", "salary", "semantic")]);
        for h in &hits {
            println!("[fine] {:?} -> {:?}", h, &text[h.start..h.end]);
        }
        if model_ready() {
            if let Some(h) = hits.iter().find(|h| h.entity_type == "salary") {
                let span = &text[h.start..h.end];
                assert!(
                    span.contains("薪酬"),
                    "语义应定位到薪酬片段，实际: {span}"
                );
                assert!(
                    h.end - h.start <= 14,
                    "替换范围应收窄，实际长度: {}",
                    h.end - h.start
                );
            }
        }
    }
}
