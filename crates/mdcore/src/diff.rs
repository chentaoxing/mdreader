//! Diff 引擎（规格第二十四章）。
//!
//! 展示口径固定为「结构变化 + 正文内容变化」两类：
//! - 标题调整 / 段落调整 / 列表调整 / 表格调整 / 引用调整 / 分隔线调整 / 空白调整：逐类计数
//! - **正文内容变化：恒为 0**（由 Content Guard 的 token 序列比对给出，非估算）
//!
//! 换行符与 BOM 属于文件格式层，不计入内容变化；若一次改动只在格式层，
//! `content_changes` 仍为 0，同时 `format_only` 标记为 true，UI 可据此显示「仅换行风格差异」。

use std::ops::Range;

use crate::edits::ChangeScope;
use crate::guard::GuardReport;
use crate::pipeline::Outcome;

/// Diff 中的一行改动明细。
#[derive(Debug, Clone)]
pub struct DiffEntry {
    pub rule_id: String,
    pub rule_name: String,
    pub scope: ChangeScope,
    /// 定位区间。注意：坐标是「该规则提交时那一轮文本」的语义层字节偏移，
    /// 跨轮次不保证可直接映射到最终文本，仅用于 UI 定位与排序。
    pub range: Range<usize>,
    /// 用该区间在原始正文上估算的行号（1 起），仅供展示
    pub line: usize,
    pub before: String,
    pub after: String,
    pub reason: String,
}

/// 按类别聚合的计数行。
#[derive(Debug, Clone)]
pub struct CategoryCount {
    pub label: &'static str,
    pub count: usize,
}

/// 完整 Diff 视图，UI 直接渲染，不自行统计。
#[derive(Debug, Clone)]
pub struct Diff {
    pub categories: Vec<CategoryCount>,
    /// 正文内容变化数，必须为 0；不为 0 说明上游有 bug，UI 应拒绝「接受」
    pub content_changes: usize,
    pub entries: Vec<DiffEntry>,
    /// 本次差异是否全部落在格式层（BOM / 换行风格 / 结尾换行）
    pub format_only: bool,
    pub passes: usize,
    pub structural_changes: usize,
}

impl Diff {
    /// 从一次优化结果构造 Diff。
    pub fn from_outcome(o: &Outcome) -> Diff {
        let mut entries: Vec<DiffEntry> = Vec::new();
        let olines = crate::text::Lines::new(&o.original_body);
        for applied in &o.applied {
            for (idx, range) in applied.ranges.iter().enumerate() {
                let ln = olines.line_at(range.start);
                let before = olines.line(ln).map(|(s, _)| clip(s)).unwrap_or_default();
                let after = applied
                    .after_snippets
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| clip(&before));
                entries.push(DiffEntry {
                    rule_id: applied.rule_id.clone(),
                    rule_name: applied.rule_name.clone(),
                    scope: applied.scope,
                    range: range.clone(),
                    line: ln + 1,
                    before,
                    after,
                    reason: applied.reason.clone(),
                });
            }
        }

        let mut categories: Vec<CategoryCount> = Vec::new();
        for applied in &o.applied {
            let label = applied.scope.label();
            match categories.iter_mut().find(|c| c.label == label) {
                Some(c) => c.count += applied.edit_count,
                None => categories.push(CategoryCount { label, count: applied.edit_count }),
            }
        }
        categories.sort_by(|a, b| a.label.cmp(b.label));

        Diff {
            categories,
            content_changes: o.guard.violations.len(),
            entries,
            format_only: false,
            passes: o.passes,
            structural_changes: o.edit_total(),
        }
    }

    /// 格式层比较专用 Diff：只回答「换行/BOM 差异算不算内容变化」。
    ///
    /// 规格第三十一章：答案必须是否定的。
    pub fn format_layer(original: &str, candidate: &str) -> Diff {
        let report: GuardReport = crate::guard::verify(
            &strip_newlines(original),
            &strip_newlines(candidate),
        );
        let content_changes = report.violations.len();
        Diff {
            categories: Vec::new(),
            content_changes,
            entries: Vec::new(),
            format_only: content_changes == 0,
            passes: 1,
            structural_changes: 0,
        }
    }

    /// 规格第二十四章的硬约束：正文内容变化为 0 才允许「接受」。
    pub fn acceptable(&self) -> bool {
        self.content_changes == 0
    }

    /// 文本形式的 Diff 摘要，CLI 与报告共用。
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("结构变化\n");
        if self.categories.is_empty() {
            out.push_str("  （无）\n");
        }
        for c in &self.categories {
            out.push_str(&format!("  {} {}\n", c.label, c.count));
        }
        out.push_str(&format!("  合计 {}\n", self.structural_changes));
        out.push_str(&format!("正文内容变化 {}\n", self.content_changes));
        out
    }
}

/// 行号定位用：把 \r\n / \r 归一化为 \n（仅用于比较，不回写文件）。
fn strip_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

fn clip(s: &str) -> String {
    if s.chars().count() <= 80 {
        s.to_string()
    } else {
        let mut v: String = s.chars().take(79).collect();
        v.push('…');
        v
    }
}

/// 接受/拒绝/撤销的决策记录（规格第二十八章：所有优化必须可逆、可选择）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Reject,
    Undo,
    Redo,
    RestoreOriginal,
}

/// 按规则粒度的决策：拒绝某条规则 = 从当前文本回滚该规则的编辑。
#[derive(Debug, Clone)]
pub struct DecisionLog {
    pub items: Vec<(Decision, String)>,
}

impl Default for DecisionLog {
    fn default() -> Self {
        DecisionLog { items: Vec::new() }
    }
}

impl DecisionLog {
    pub fn record(&mut self, d: Decision, rule_id: &str) {
        self.items.push((d, rule_id.to_string()));
    }

    /// 逐条拒绝后重建文本：跳过被 Reject / Undo 的规则，其余按原顺序重新提交。
    ///
    /// 关键纪律：重建后的文本必须再次通过全量 Guard，否则返回 Err，绝不输出可疑结果。
    pub fn rebuild(&self, original: &str, o: &Outcome, registry: &crate::rules::Registry, profile: &crate::rules::Profile) -> crate::error::CoreResult<String> {
        use crate::rules::Ctx;
        let mut skip: Vec<&str> = Vec::new();
        for (d, id) in &self.items {
            match d {
                Decision::Reject | Decision::Undo | Decision::RestoreOriginal => {
                    if !skip.contains(&id.as_str()) {
                        skip.push(id);
                    }
                }
                Decision::Accept | Decision::Redo => {
                    if let Some(pos) = skip.iter().position(|s| *s == id.as_str()) {
                        skip.remove(pos);
                    }
                }
            }
        }
        let mut body = original.to_string();
        for applied in &o.applied {
            if skip.iter().any(|s| *s == applied.rule_id.as_str()) {
                continue;
            }
            let rule = match registry.find(&applied.rule_id) {
                Some(r) => r,
                None => continue,
            };
            let scan = crate::scan::Scan::new(&body);
            let ctx = Ctx { src: &body, scan: &scan, params: &profile.params };
            if let Some(Some(re)) = rule.apply(&ctx).ok() {
                let protected = crate::fingerprint::protected_ranges(&body);
                let next = crate::edits::apply_edits(&body, &re, &protected)?;
                if crate::guard::verify(&body, &next).is_pass() {
                    body = next;
                }
            }
        }
        let report = crate::guard::verify(original, &body);
        if !report.is_pass() {
            return Err(crate::error::CoreError::GuardRejected {
                rule: "diff.rebuild".into(),
                detail: report.violations.iter().map(|v| v.detail.clone()).collect::<Vec<_>>().join("; "),
            });
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::optimize;

    #[test]
    fn content_changes_are_always_zero() {
        let o = optimize("# A\n\n\n\n正文\n\n\n\n正文二\n").unwrap();
        let d = Diff::from_outcome(&o);
        assert_eq!(d.content_changes, 0);
        assert!(d.acceptable());
        assert!(d.structural_changes > 0);
    }

    #[test]
    fn categories_use_spec_labels() {
        let o = optimize("# 标题\n\n\n\n正文一\n\n\n\n正文二\n").unwrap();
        let d = Diff::from_outcome(&o);
        let labels: Vec<&str> = d.categories.iter().map(|c| c.label).collect();
        for l in &labels {
            assert!(
                ["标题调整", "段落调整", "列表调整", "表格调整", "引用调整", "分隔线调整", "空白调整", "代码块调整", "其他调整"].contains(l),
                "未知类别 {l}"
            );
        }
        let total: usize = d.categories.iter().map(|c| c.count).sum();
        assert_eq!(total, d.structural_changes);
    }

    #[test]
    fn newline_style_difference_is_not_a_content_change() {
        let lf = "# 标题\n\n正文\n";
        let crlf = lf.replace('\n', "\r\n");
        let d = Diff::format_layer(lf, &crlf);
        assert_eq!(d.content_changes, 0, "换行符差异不得计为内容变化");
        assert!(d.format_only);
    }

    #[test]
    fn punctuation_loss_is_a_content_change() {
        // 规格第二十章的陷阱：字段转表格会吃掉「：」
        let a = "名称：产品\n";
        let b = "| 名称 | 产品 |\n| --- | --- |\n";
        let d = Diff::format_layer(a, b);
        assert_ne!(d.content_changes, 0, "丢标点必须计为内容变化");
        assert!(!d.acceptable(), "此类改动 UI 不允许接受");
    }

    #[test]
    fn word_order_swap_is_a_content_change_not_char_count() {
        let d = Diff::format_layer("甲乙\n", "乙甲\n");
        assert_ne!(d.content_changes, 0, "字数相同但顺序不同，仍必须拒绝");
    }

    #[test]
    fn diff_entries_point_at_real_lines() {
        let o = optimize("# 标题\n\n\n\n正文一段\n").unwrap();
        let d = Diff::from_outcome(&o);
        assert!(!d.entries.is_empty());
        for e in &d.entries {
            assert!(e.line >= 1);
            assert!(!e.rule_id.is_empty());
            assert!(!e.reason.is_empty(), "{:?}", e);
        }
        // 行号必须落在文档实际行范围内
        let total = crate::text::Lines::new(&o.original_body).count();
        for e in &d.entries {
            assert!(e.line <= total + 1, "{:?} 超出行数 {total}", e);
        }
    }

    #[test]
    fn reject_a_rule_restores_that_part_only() {
        let src = "# 标题   \n\n\n\n正文   \n";
        let registry = crate::rules::Registry::new();
        let profile = crate::rules::Profile::default();
        let o = crate::pipeline::Optimizer::new(&registry, &profile).run(src).unwrap();
        let mut log = DecisionLog::default();
        // 拒绝「行尾空格」这条规则
        log.record(Decision::Reject, "R02.trailing-whitespace");
        let body = log.rebuild(src, &o, &registry, &profile).unwrap();
        assert!(body.contains("# 标题   "), "被拒绝的规则不应生效");
        assert!(!body.contains("\n\n\n\n"), "其它规则仍应生效");
        // 重建结果必须仍通过全量 Guard
        assert!(crate::guard::verify(src, &body).is_pass());
    }

    #[test]
    fn undo_then_redo_round_trips() {
        let src = "# 标题\n\n\n\n正文\n";
        let registry = crate::rules::Registry::new();
        let profile = crate::rules::Profile::default();
        let o = crate::pipeline::Optimizer::new(&registry, &profile).run(src).unwrap();
        let full = o.body.clone();
        let mut log = DecisionLog::default();
        log.record(Decision::Undo, "R01.blank-lines");
        let undone = log.rebuild(src, &o, &registry, &profile).unwrap();
        assert_ne!(undone, full);
        log.record(Decision::Redo, "R01.blank-lines");
        let redone = log.rebuild(src, &o, &registry, &profile).unwrap();
        assert_eq!(redone, full, "redo 后应回到完整优化结果");
    }

    #[test]
    fn restore_original_is_always_available() {
        let src = "# 标题\n\n\n\n正文\n";
        let registry = crate::rules::Registry::new();
        let profile = crate::rules::Profile::default();
        let o = crate::pipeline::Optimizer::new(&registry, &profile).run(src).unwrap();
        let mut log = DecisionLog::default();
        for a in &o.applied {
            log.record(Decision::RestoreOriginal, &a.rule_id);
        }
        let back = log.rebuild(src, &o, &registry, &profile).unwrap();
        assert_eq!(back, src, "全部回滚必须逐字节等于原文");
    }

    #[test]
    fn render_lists_both_sections() {
        let o = optimize("# A\n\n\n\n文\n").unwrap();
        let text = Diff::from_outcome(&o).render();
        assert!(text.contains("结构变化"));
        assert!(text.contains("正文内容变化 0"));
    }
}
