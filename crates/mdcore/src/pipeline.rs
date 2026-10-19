//! 优化管道（Core First 的唯一编排入口）。
//!
//! 规格第七章要求核心可无头运行：`Markdown String -> Core Engine -> Optimized Markdown String`。
//! 本模块就是这个 Core Engine，不依赖任何 IO、UI、网络。
//!
//! 执行纪律（对应规格第九、二十八、三十一章）：
//! 1. 规则按 `(priority 降序, id 升序)` 顺序执行，五平台完全一致；
//! 2. 每条规则**独立提交**：apply → validate → apply_edits → Content Guard 校验，
//!    任一环节失败则该条规则整体丢弃，绝不部分提交；
//! 3. 每次提交都留下完整记录（Rule ID / 影响区间 / 理由 / 改前改后），供 Undo、Rollback、Diff 使用；
//! 4. 多轮重复直到没有任何规则改动（收敛）或达到 `max_passes`；
//! 5. 最后对「原始正文 → 最终正文」再做一次全量 Guard 校验，作为兜底铁证。
//!    若此处失败，管道直接回退原文并返回错误 —— 宁可输出原文，也不输出改坏的内容。

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::edits::{apply_edits, ChangeScope, EditKind, RuleEdit};
use crate::error::{CoreError, CoreResult};
use crate::fingerprint::{protected_ranges, tokenize};
use crate::guard::{self, GuardReport};
use crate::model::Document;
use crate::readability::{self, Report};
use crate::rules::{Profile, Registry};
use crate::scan::Scan;
use crate::text::{NewlineStyle, TextLayer};

/// 取消信号：UI 层持有 `Arc<AtomicBool>`，核心只在规则边界检查一次，保证可中断且不改变结果。
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Cancel {
        Cancel(Arc::new(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
    pub fn cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// 一条规则的成功提交记录（可回滚的完整凭据）。
#[derive(Debug, Clone)]
pub struct AppliedRule {
    pub rule_id: String,
    pub rule_name: String,
    pub scope: ChangeScope,
    pub pass: usize,
    /// 该规则包含的编辑数
    pub edit_count: usize,
    /// 影响区间（语义层字节偏移），按出现顺序，已合并相邻
    pub ranges: Vec<Range<usize>>,
    /// 规则给出的改动理由（取首个编辑的理由，供 Diff 明细展示）
    pub reason: String,
    /// 改后文本片段（仅供 UI 展示，不回写）
    pub after_snippets: Vec<String>,
}

/// 一条被拒绝的规则及原因，UI 必须如实展示，不做静默失败。
#[derive(Debug, Clone)]
pub struct RejectedRule {
    pub rule_id: String,
    pub rule_name: String,
    pub reason: String,
}

/// 一次优化的完整结果。
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 原始语义层正文（LF 归一化，不含 BOM）
    pub original_body: String,
    /// 优化后的语义层正文
    pub body: String,
    pub applied: Vec<AppliedRule>,
    pub rejected: Vec<RejectedRule>,
    pub passes: usize,
    /// 原始 -> 最终 的全量 Guard 报告（铁证）
    pub guard: GuardReport,
    /// 优化前阅读性评分
    pub before: Report,
    /// 优化后阅读性评分
    pub after: Report,
    /// 是否因取消信号提前结束（已提交的改动仍然有效且已通过 Guard）
    pub cancelled: bool,
}

impl Outcome {
    /// 正文内容变化数。规格第二十四章要求这一项恒为 0。
    pub fn content_changes(&self) -> usize {
        self.guard.violations.len()
    }

    pub fn changed(&self) -> bool {
        self.body != self.original_body
    }

    pub fn edit_total(&self) -> usize {
        self.applied.iter().map(|a| a.edit_count).sum()
    }

    /// 按作用域聚合的编辑数，供 Diff 直接展示。
    pub fn scope_counts(&self) -> Vec<(&'static str, usize)> {
        let mut out: Vec<(&'static str, usize)> = Vec::new();
        for a in &self.applied {
            let label = a.scope.label();
            if let Some(slot) = out.iter_mut().find(|(l, _)| *l == label) {
                slot.1 += a.edit_count;
            } else {
                out.push((label, a.edit_count));
            }
        }
        // 固定排序：标签升序，保证五平台展示顺序一致
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}

/// 单条规则的干跑预览（规格第二十九章 Rule 接口的 Preview 能力）。
#[derive(Debug, Clone)]
pub struct Preview {
    pub rule_id: String,
    pub rule_name: String,
    pub description: String,
    pub scope: ChangeScope,
    pub edit_count: usize,
    /// 预览后的文本；Guard 不通过时为 None（并给出原因）
    pub result: Option<String>,
    pub reject_reason: Option<String>,
}

/// 无头优化器。`Optimizer::new(&registry, &profile).run(body)` 即可。
pub struct Optimizer<'a> {
    registry: &'a Registry,
    profile: &'a Profile,
    cancel: Cancel,
}

impl<'a> Optimizer<'a> {
    pub fn new(registry: &'a Registry, profile: &'a Profile) -> Self {
        Self { registry, profile, cancel: Cancel::default() }
    }

    pub fn with_cancel(mut self, cancel: Cancel) -> Self {
        self.cancel = cancel;
        self
    }

    /// 从任意 Markdown 文本出发优化，返回语义层结果 + 全量 Guard 证据。
    pub fn run(&self, original_body: &str) -> CoreResult<Outcome> {
        let before = self.analyze(original_body)?;
        let mut body = original_body.to_string();
        let mut applied: Vec<AppliedRule> = Vec::new();
        let mut rejected: Vec<RejectedRule> = Vec::new();
        let mut passes = 0usize;
        let mut cancelled = false;

        for pass in 0..self.profile.max_passes.max(1) {
            passes = pass + 1;
            let mut changed_this_pass = false;
            for rule in self.registry.all() {
                if !self.profile.is_active(rule.as_ref()) {
                    continue;
                }
                if self.cancel.cancelled() {
                    cancelled = true;
                    break;
                }
                let scan = Scan::new(&body);
                let ctx = crate::rules::Ctx { src: &body, scan: &scan, params: &self.profile.params };
                let re = match rule.apply(&ctx) {
                    Ok(Some(re)) => re,
                    Ok(None) => continue,
                    Err(CoreError::Cancelled) => {
                        cancelled = true;
                        break;
                    }
                    Err(e) => {
                        rejected.push(RejectedRule {
                            rule_id: rule.id().to_string(),
                            rule_name: rule.name().to_string(),
                            reason: e.to_string(),
                        });
                        continue;
                    }
                };
                match self.commit(&body, &re) {
                    Ok(next) => {
                        applied.push(record_of(&re, pass, &body, &next));
                        body = next;
                        changed_this_pass = true;
                    }
                    Err(reason) => rejected.push(RejectedRule {
                        rule_id: re.rule_id.clone(),
                        rule_name: re.rule_name.clone(),
                        reason,
                    }),
                }
            }
            if cancelled || !changed_this_pass {
                break;
            }
        }

        // 兜底铁证：原始正文 -> 最终正文 必须逐 token 相同。
        let guard = guard::verify(original_body, &body);
        if !guard.is_pass() {
            let detail = guard
                .violations
                .iter()
                .map(|v| v.detail.clone())
                .collect::<Vec<_>>()
                .join("; ");
            return Err(CoreError::GuardRejected { rule: "pipeline".into(), detail });
        }

        let after = if body == original_body { before.clone() } else { self.analyze(&body)? };
        Ok(Outcome { original_body: original_body.to_string(), body, applied, rejected, passes, guard, before, after, cancelled })
    }

    /// 端到端：字节流进、字节流出，格式层（BOM / CRLF / 结尾换行）原样保留。
    ///
    /// 规格第三十一章：换行符差异属于文件格式层，不计入内容变化。
    pub fn run_bytes(&self, bytes: &[u8], style: NewlineStyle) -> CoreResult<(Vec<u8>, Outcome)> {
        let layer = TextLayer::from_bytes(bytes)?;
        let outcome = self.run(layer.body())?;
        Ok((layer.render(&outcome.body, style), outcome))
    }

    /// 只做分析，不改一字（规格第二十五章：评分只提示，不驱动内容改动）。
    pub fn analyze(&self, body: &str) -> CoreResult<Report> {
        let scan = Scan::new(body);
        let doc = Document::parse(body)?;
        Ok(readability::analyze(
            body,
            &scan,
            &doc,
            self.profile.params.max_blank_lines,
            self.profile.params.long_paragraph_width,
        ))
    }

    /// 逐条规则干跑，供「单规则测试」与预览面板使用。
    pub fn preview_all(&self, body: &str) -> CoreResult<Vec<Preview>> {
        let mut out = Vec::new();
        for rule in self.registry.all() {
            let mut p = Preview {
                rule_id: rule.id().to_string(),
                rule_name: rule.name().to_string(),
                description: rule.description().to_string(),
                scope: rule.scope(),
                edit_count: 0,
                result: None,
                reject_reason: None,
            };
            if self.profile.is_active(rule.as_ref()) {
                let scan = Scan::new(body);
                let ctx = crate::rules::Ctx { src: body, scan: &scan, params: &self.profile.params };
                match rule.apply(&ctx) {
                    Ok(Some(re)) => {
                        p.edit_count = re.edits.iter().filter(|e| !e.is_noop()).count();
                        match self.commit(body, &re) {
                            Ok(next) => p.result = Some(next),
                            Err(r) => p.reject_reason = Some(r),
                        }
                    }
                    Ok(None) => {}
                    Err(e) => p.reject_reason = Some(e.to_string()),
                }
            }
            out.push(p);
        }
        Ok(out)
    }

    /// 单条规则测试（规格第二十九章：可单独测试某条规则）。
    pub fn preview_rule(&self, body: &str, id: &str) -> CoreResult<Option<Preview>> {
        let rule = self
            .registry
            .find(id)
            .ok_or_else(|| CoreError::UnknownRule(id.to_string()))?;
        let scan = Scan::new(body);
        let ctx = crate::rules::Ctx { src: body, scan: &scan, params: &self.profile.params };
        let mut p = Preview {
            rule_id: rule.id().to_string(),
            rule_name: rule.name().to_string(),
            description: rule.description().to_string(),
            scope: rule.scope(),
            edit_count: 0,
            result: None,
            reject_reason: None,
        };
        match rule.apply(&ctx) {
            Ok(Some(re)) => {
                p.edit_count = re.edits.iter().filter(|e| !e.is_noop()).count();
                match self.commit(body, &re) {
                    Ok(next) => p.result = Some(next),
                    Err(r) => p.reject_reason = Some(r),
                }
            }
            Ok(None) => {}
            Err(e) => p.reject_reason = Some(e.to_string()),
        }
        Ok(Some(p))
    }

    /// 提交一条规则：应用 + 静态自检已由 `package` 完成 + 动态 Guard 校验，失败即整条丢弃。
    fn commit(&self, body: &str, re: &RuleEdit) -> Result<String, String> {
        if re.edits.iter().any(|e| e.kind == EditKind::Content) {
            return Err("规则提交了 Content 类型编辑，已拒绝".into());
        }
        let protected = protected_ranges(body);
        let next = apply_edits(body, re, &protected).map_err(|e| e.to_string())?;
        if next == body {
            return Err("编辑未产生变化（等价空操作）".into());
        }
        // 逐规则动态校验：token 序列必须完全一致
        let report = guard::verify(body, &next);
        if !report.is_pass() {
            let d = report.violations.iter().map(|v| v.detail.clone()).collect::<Vec<_>>().join("; ");
            return Err(format!("Content Guard 拒绝：{d}"));
        }
        Ok(next)
    }
}

fn record_of(re: &RuleEdit, pass: usize, before: &str, after: &str) -> AppliedRule {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut e = RuleEdit {
        rule_id: re.rule_id.clone(),
        rule_name: re.rule_name.clone(),
        scope: re.scope,
        edits: re.edits.clone(),
    };
    e.edits.sort_by(|a, b| a.range.start.cmp(&b.range.start));
    for edit in &e.edits {
        if edit.is_noop() {
            continue;
        }
        push_merged(&mut ranges, edit.range.clone());
    }
    // 行索引各建一次：逐区间重建会把大文档拖成 O(n²)
    let idx_after = crate::text::Lines::new(after);
    let idx_before = crate::text::Lines::new(before);
    let after_snippets = ranges.iter().map(|r| snippet(&idx_after, r, &idx_before)).collect();
    let reason = re
        .edits
        .iter()
        .find(|x| !x.is_noop())
        .map(|x| x.reason.clone())
        .unwrap_or_default();
    AppliedRule {
        rule_id: re.rule_id.clone(),
        rule_name: re.rule_name.clone(),
        scope: re.scope,
        pass,
        edit_count: re.edits.iter().filter(|x| !x.is_noop()).count(),
        ranges,
        after_snippets,
        reason,
    }
}

fn push_merged(ranges: &mut Vec<Range<usize>>, r: Range<usize>) {
    // 同点插入（start == end）也要能合并展示
    if let Some(last) = ranges.last_mut() {
        if r.start <= last.end {
            last.end = last.end.max(r.end);
            return;
        }
    }
    ranges.push(r);
}

fn snippet(after: &crate::text::Lines<'_>, range: &Range<usize>, before: &crate::text::Lines<'_>) -> String {
    let cur = after
        .line(after.line_at(range.start))
        .map(|(s, _)| s.to_string())
        .unwrap_or_default();
    let old = before
        .line(before.line_at(range.start))
        .map(|(s, _)| s.to_string())
        .unwrap_or_default();
    if cur == old {
        trim(cur)
    } else {
        trim(format!("{old} ⇒ {cur}"))
    }
}

fn trim(mut s: String) -> String {
    if s.chars().count() > 80 {
        let keep: String = s.chars().take(79).collect();
        s = keep;
        s.push('…');
    }
    s
}

/// 便捷入口：默认注册表 + 默认配置，一次跑完。
pub fn optimize(body: &str) -> CoreResult<Outcome> {
    let registry = Registry::new();
    let profile = Profile::default();
    Optimizer::new(&registry, &profile).run(body)
}

/// 内容 token 数（供 UI 显示「正文规模」，不参与任何改写决策）。
pub fn content_token_count(body: &str) -> usize {
    tokenize(body).tokens.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optimizes_without_touching_content() {
        let src = "# 标题\n\n\n\n正文一段。\n\n\n\n正文二段。\n";
        let o = optimize(src).unwrap();
        assert!(o.changed(), "空行应被压缩");
        assert_eq!(o.content_changes(), 0);
        assert_eq!(o.body, "# 标题\n\n正文一段。\n\n正文二段。\n");
    }

    #[test]
    fn final_guard_is_the_backstop() {
        // 原始 -> 最终 的 token 序列必须一致
        let src = "价格 50\\% 浮动，见 [链接](https://example.com/a_b)。\n\n\n\n结束。\n";
        let o = optimize(src).unwrap();
        assert_eq!(o.guard.original.checksum, o.guard.candidate.checksum);
        assert!(o.body.contains("50\\%"));
        assert!(o.body.contains("[链接](https://example.com/a_b)"));
    }

    #[test]
    fn code_blocks_survive_byte_for_byte() {
        let src = "段落\n\n```python\nx = 1\n\n\n\ny = 2   \n```\n\n段落二\n";
        let o = optimize(src).unwrap();
        let start = o.body.find("```python").unwrap();
        let end = o.body[start + 3..].find("\n```").unwrap() + start + 3 + 4;
        let code = &o.body[start..end];
        assert!(code.contains("y = 2   "), "代码块内行尾空格不得动：{code:?}");
        assert!(code.contains("x = 1\n\n\n\ny = 2"), "代码块内空行不得动：{code:?}");
    }

    #[test]
    fn convergence_is_idempotent() {
        let src = "# A\n\n\n\n文\n\n\n\n\n文\n";
        let first = optimize(src).unwrap();
        let second = optimize(&first.body).unwrap();
        assert!(!second.changed(), "第二次跑不应再有改动：{:?}", second.applied);
        assert_eq!(second.applied.len(), 0);
    }

    #[test]
    fn format_layer_is_preserved() {
        let bytes = "\u{feff}# 标题\r\n\r\n\r\n\r\n正文\r\n".to_string().into_bytes();
        let registry = Registry::new();
        let profile = Profile::default();
        let (out, o) = Optimizer::new(&registry, &profile).run_bytes(&bytes, NewlineStyle::Preserve).unwrap();
        assert_eq!(o.content_changes(), 0);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with('\u{feff}'), "BOM 应保留");
        assert!(text.contains("\r\n"), "CRLF 应保留");
        assert!(!text.contains("\r\n\r\n\r\n"), "空行应压缩");
    }

    #[test]
    fn only_and_disabled_are_honoured() {
        let src = "# 标题\n\n\n\n正文\n";
        let registry = Registry::new();
        let mut profile = Profile::default();
        profile.only = vec!["R01.blank-lines".to_string()];
        let o = Optimizer::new(&registry, &profile).run(src).unwrap();
        assert!(o.applied.iter().all(|a| a.rule_id == "R01.blank-lines"));
    }

    #[test]
    fn rejected_rules_are_reported_not_silenced() {
        let registry = Registry::new();
        let profile = Profile::default();
        // 已规范化的文本：所有规则都应返回 None，不应有 applied
        let src = "# 标题\n\n正文\n\n- 项目\n\n```rust\nfn a() {}\n```\n";
        let o = Optimizer::new(&registry, &profile).run(src).unwrap();
        assert!(!o.changed(), "{:?}", o.applied);
    }

    #[test]
    fn cancel_stops_but_keeps_committed_work_valid() {
        let registry = Registry::new();
        let profile = Profile::default();
        let cancel = Cancel::new();
        cancel.cancel();
        let o = Optimizer::new(&registry, &profile).with_cancel(cancel).run("# A\n\n\n\nB\n").unwrap();
        assert!(o.cancelled);
        assert_eq!(o.content_changes(), 0);
    }

    #[test]
    fn preview_is_a_dry_run() {
        let src = "正文\n\n\n\n正文二\n";
        let registry = Registry::new();
        let profile = Profile::default();
        let before = src.to_string();
        let ps = Optimizer::new(&registry, &profile).preview_all(&before).unwrap();
        assert_eq!(ps.len(), registry.all().len());
        assert_eq!(src, before.as_str(), "预览不得改原文");
        let r01 = ps.iter().find(|p| p.rule_id == "R01.blank-lines").unwrap();
        assert_eq!(r01.edit_count, 2, "两个多余空行");
        assert!(r01.result.as_deref() == Some("正文\n\n正文二\n"), "{:?}", r01.result);
    }

    #[test]
    fn scope_counts_sum_to_edit_total() {
        let o = optimize("# A\n\n\n\n正文\n\n\n\n正文二\n").unwrap();
        let total: usize = o.scope_counts().iter().map(|(_, c)| c).sum();
        assert_eq!(total, o.edit_total());
    }

    #[test]
    fn scores_improve_but_never_gate_content() {
        let src = "# 一\n\n\n\n\n正文一\n\n\n\n\n正文二\n\n\n\n结束\n";
        let o = optimize(src).unwrap();
        assert!(o.after.whitespace.value >= o.before.whitespace.value);
        // 评分变化不影响 token 序列
        assert_eq!(o.guard.original.checksum, o.guard.candidate.checksum);
    }

    #[test]
    fn deterministic_across_repeated_runs() {
        let src = "# A\n### C\n\n\n\n正文：一、二、三、四。\n\n- b\n- a\n\n\n";
        let registry = Registry::new();
        let profile = Profile::default();
        let a = Optimizer::new(&registry, &profile).run(src).unwrap();
        let b = Optimizer::new(&registry, &profile).run(src).unwrap();
        assert_eq!(a.body, b.body);
        let ids_a: Vec<&str> = a.applied.iter().map(|x| x.rule_id.as_str()).collect();
        let ids_b: Vec<&str> = b.applied.iter().map(|x| x.rule_id.as_str()).collect();
        assert_eq!(ids_a, ids_b);
    }
}
