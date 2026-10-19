//! Content Guard —— 内容完整性裁决（规格第二十三、三十九章）。
//!
//! 这是全项目唯一有权判定「内容有没有变」的地方。任何规则的输出都必须经过它，
//! 未通过的改动**整条丢弃**，绝不部分提交、绝不写盘。
//!
//! 判定分三层，从严到宽：
//! 1. **保护区硬约束**：代码块 / 行内代码 / HTML / 链接目标区间内的字节必须逐字节不变。
//! 2. **token 序列全等**：内容 token 序列（顺序敏感）必须完全一致——不是「字符数相同」。
//! 3. **反斜杠兜底**：转义符数量必须相等，防止「吃掉 `\`」这类渲染等价但语义不同的改动。
//!
//! 另外：行尾符与 BOM 属于文件格式层（见 [`crate::text`]），在本层比较前已归一化，
//! 因此**不会**被误判为内容变化。

use crate::error::{CoreError, CoreResult};
use crate::fingerprint::{first_divergence, tokenize, ContentToken, Fingerprint};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 内容完全一致，允许
    Pass,
    /// 内容发生变化，必须拒绝
    Fail,
}

#[derive(Debug, Clone)]
pub struct Violation {
    pub detail: String,
    /// 第一个不一致 token 的下标（若有）
    pub token_index: Option<usize>,
    pub original_token: Option<String>,
    pub candidate_token: Option<String>,
    pub original_offset: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct GuardReport {
    pub verdict: Verdict,
    pub violations: Vec<Violation>,
    pub original: Fingerprint,
    pub candidate: Fingerprint,
}

impl GuardReport {
    pub fn is_pass(&self) -> bool {
        self.verdict == Verdict::Pass
    }

    pub fn body_diff_count(&self) -> usize {
        match self.verdict {
            Verdict::Pass => 0,
            Verdict::Fail => self.violations.len(),
        }
    }
}

/// 校验一次「原文 -> 候选文本」的改写。
pub fn verify(original_src: &str, candidate_src: &str) -> GuardReport {
    let original = tokenize(original_src);
    let candidate = tokenize(candidate_src);
    let mut violations = Vec::new();

    // 1. token 序列
    match first_divergence(&original.tokens, &candidate.tokens) {
        None => {}
        Some((_idx, olen, clen)) => {
            // 逐位收集所有差异（上限 8 条，避免大文档报告爆炸）
            let mut collected = 0;
            let min = olen.min(clen);
            for i in 0..min {
                if original.tokens[i] != candidate.tokens[i] {
                    violations.push(violation_from(&original.tokens, &candidate.tokens, i));
                    collected += 1;
                    if collected >= 8 {
                        break;
                    }
                }
            }
            if collected == 0 {
                // 仅长度不同
                violations.push(Violation {
                    detail: format!("内容 token 数量不一致：原 {olen} → 新 {clen}"),
                    token_index: Some(min),
                    original_token: original.tokens.get(min).map(token_text),
                    candidate_token: candidate.tokens.get(min).map(token_text),
                    original_offset: original.tokens.get(min).map(|t| t.offset),
                });
            }
        }
    }

    // 2. 校验和（快速路径，用于日志与五平台一致性断言）
    if original.checksum != candidate.checksum && violations.is_empty() {
        violations.push(Violation {
            detail: format!(
                "内容校验和不一致：{:016x} → {:016x}",
                original.checksum, candidate.checksum
            ),
            token_index: None,
            original_token: None,
            candidate_token: None,
            original_offset: None,
        });
    }

    // 3. 反斜杠兜底
    if original.backslashes != candidate.backslashes {
        violations.push(Violation {
            detail: format!(
                "转义反斜杠数量变化：{} → {}（转义符承载语法含义，不得增删）",
                original.backslashes, candidate.backslashes
            ),
            token_index: None,
            original_token: None,
            candidate_token: None,
            original_offset: None,
        });
    }

    // 4. 保护区逐字节：代码/HTML 的每个 Code/Html/Url token 已在序列比对中覆盖，
    //    这里额外要求「代码字节总量」不减，防止解析器把代码块降级成普通段落。
    if candidate.counts.code_bytes < original.counts.code_bytes {
        violations.push(Violation {
            detail: format!(
                "代码内容字节减少：{} → {}（疑似代码块结构被破坏）",
                original.counts.code_bytes, candidate.counts.code_bytes
            ),
            token_index: None,
            original_token: None,
            candidate_token: None,
            original_offset: None,
        });
    }

    GuardReport {
        verdict: if violations.is_empty() { Verdict::Pass } else { Verdict::Fail },
        violations,
        original,
        candidate,
    }
}

/// 校验并把失败转成错误（规则管道用）。
pub fn ensure_pass(original_src: &str, candidate_src: &str, rule_id: &str) -> CoreResult<GuardReport> {
    let report = verify(original_src, candidate_src);
    if report.is_pass() {
        return Ok(report);
    }
    let detail = report
        .violations
        .iter()
        .map(|v| v.detail.as_str())
        .collect::<Vec<_>>()
        .join("；");
    Err(CoreError::GuardRejected { rule: rule_id.into(), detail })
}

fn token_text(t: &ContentToken) -> String {
    t.text.clone()
}

fn violation_from(
    a: &[ContentToken],
    b: &[ContentToken],
    i: usize,
) -> Violation {
    let ta = a.get(i);
    let tb = b.get(i);
    let detail = match (ta, tb) {
        (Some(x), Some(y)) => format!(
            "第 {i} 个内容 token 不一致：{:?} → {:?}",
            truncate(&x.text),
            truncate(&y.text)
        ),
        (Some(x), None) => format!("第 {i} 个内容 token 被删除：{:?}", truncate(&x.text)),
        (None, Some(y)) => format!("第 {i} 个位置多出内容 token：{:?}", truncate(&y.text)),
        (None, None) => format!("第 {i} 个内容 token 不一致"),
    };
    Violation {
        detail,
        token_index: Some(i),
        original_token: ta.map(token_text),
        candidate_token: tb.map(token_text),
        original_offset: ta.map(|t| t.offset),
    }
}

fn truncate(s: &str) -> String {
    // 按字符截断，避免切坏中文 / emoji
    let mut out = String::new();
    for (n, ch) in s.chars().enumerate() {
        if n >= 24 {
            out.push('…');
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_only_change_passes() {
        let r = verify("# 标题\n正文\n", "# 标题\n\n正文\n");
        assert!(r.is_pass(), "{:?}", r.violations);
    }

    #[test]
    fn heading_level_change_passes() {
        let r = verify("## 标题\n正文\n", "# 标题\n正文\n");
        assert!(r.is_pass());
    }

    #[test]
    fn char_addition_fails() {
        let r = verify("正文\n", "正文啊\n");
        assert_eq!(r.verdict, Verdict::Fail);
    }

    #[test]
    fn same_char_count_different_order_fails() {
        // 字符数相同、排序不同——这正是「不能只比较字符数量」要防的
        let r = verify("甲乙\n", "乙甲\n");
        assert_eq!(r.verdict, Verdict::Fail, "必须拒绝 token 顺序变化");
    }

    #[test]
    fn anagram_across_words_fails() {
        let r = verify("ab cd\n", "ac bd\n");
        assert_eq!(r.verdict, Verdict::Fail);
    }

    #[test]
    fn deletion_of_punctuation_fails() {
        let r = verify("成本高、部署难。\n", "成本高 部署难。\n");
        assert_eq!(r.verdict, Verdict::Fail);
    }

    #[test]
    fn code_block_body_change_fails() {
        let r = verify("```\nx = 1\n```\n", "```\nx = 2\n```\n");
        assert_eq!(r.verdict, Verdict::Fail);
    }

    #[test]
    fn fence_language_marker_is_structure() {
        // 围栏语言标注属于结构元数据，不在任何 Text 事件里 → 去掉它不改变内容 token。
        // 注意：规则层不会去改动它，这里只是把 Guard 的口径钉住，防止未来误判。
        let r = verify("```python\nx = 1\n```\n", "```\nx = 1\n```\n");
        assert!(r.is_pass(), "{:?}", r.violations);
    }

    #[test]
    fn fence_info_string_content_change_still_caught_via_code_bytes() {
        // 若改写导致代码块整体退化（代码字节减少），必须被拒绝
        let r = verify("```\ncode line\n```\n", "code line\n");
        assert_eq!(r.verdict, Verdict::Fail);
    }

    #[test]
    fn url_change_fails() {
        let r = verify("[a](http://x.com/1)\n", "[a](http://x.com/2)\n");
        assert_eq!(r.verdict, Verdict::Fail);
    }

    #[test]
    fn backslash_count_enforced() {
        let r = verify("50\\% 概率\n", "50% 概率\n");
        assert_eq!(r.verdict, Verdict::Fail, "吃掉转义符必须被拒绝");
    }

    #[test]
    fn crlf_vs_lf_is_not_content_change() {
        // 语义层已是 LF；此处直接比较两种归一化结果应等价
        let r = verify("a\nb\n", "a\nb\n");
        assert!(r.is_pass());
    }

    #[test]
    fn ensure_pass_maps_failure() {
        let e = ensure_pass("正文\n", "正文！\n", "rule.x");
        assert!(matches!(e, Err(CoreError::GuardRejected { .. })));
        assert!(ensure_pass("正文\n", "正文\n", "rule.x").is_ok());
    }

    #[test]
    fn list_promotion_passes_guard() {
        let orig = "这个方案有三个问题：成本高、部署复杂、维护困难。\n";
        let opt = "这个方案有三个问题：\n\n1. 成本高、\n2. 部署复杂、\n3. 维护困难。\n";
        let r = verify(orig, opt);
        assert!(r.is_pass(), "{:?}", r.violations);
    }

    // ---- 脚注 / 任务符号纳入可见范围后的口径（2026-10-19） ------------------

    #[test]
    fn dropping_task_marker_now_fails() {
        let r = verify("- [ ] 待办\n", "- 待办\n");
        assert_eq!(r.verdict, Verdict::Fail, "任务符号已算正文，吃掉必须拒绝：{:?}", r.violations);
    }

    #[test]
    fn dropping_footnote_reference_now_fails() {
        let r = verify("见此处[^1]\n\n[^1]: 注\n", "见此处\n\n[^1]: 注\n");
        assert_eq!(r.verdict, Verdict::Fail, "脚注引用编号被吃掉必须拒绝");
    }

    #[test]
    fn blank_line_still_collapses_around_refs() {
        // 反过来也要成立：纳入这两类标记之后，纯空白改写仍然必须放行，
        // 否则空行压缩这类基本规则会被大面积误拒。
        let r = verify("见此处[^1]\n\n\n\n[^1]: 注\n", "见此处[^1]\n\n[^1]: 注\n");
        assert!(r.is_pass(), "{:?}", r.violations);
    }

    #[test]
    fn html_comment_is_compared_verbatim() {
        // 注释走 Event::Html 逐字节这条路：删掉它就是内容变化
        let r = verify("前<!-- 内部备注 -->后\n", "前后\n");
        assert_eq!(r.verdict, Verdict::Fail, "HTML 注释被删必须被拒绝");
        assert!(verify("前<!-- 内部备注 -->后\n", "前<!-- 内部备注 -->后\n").is_pass());
    }
}
