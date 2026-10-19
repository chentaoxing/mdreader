//! 受审计的编辑引擎。
//!
//! 规则**不直接改写字符串**，只提交 `Edit { range, replacement }`。这样每个操作都能：
//! - 记录 Rule ID / 影响区间 / 理由（规格第二十八章：所有优化必须可逆）
//! - 在执行前做区间校验（重叠、非字符边界、触及保护区）
//! - 通过 Content Guard 后才落盘，失败则整条规则丢弃，绝不部分提交
//! - 逐条 undo / redo

use std::ops::Range;

use crate::error::{CoreError, CoreResult};

/// 编辑类型，用于 Diff 分类统计（规格第二十四章）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// 删除一段纯空白（空行、行尾空格、缩进）
    WhitespaceRemove,
    /// 插入纯空白
    WhitespaceInsert,
    /// 替换结构标记（标题 `#`、列表符号、引用 `>`、分隔线）
    Marker,
    /// 触及正文内容——正常情况下永远不该出现，出现即视为违规
    Content,
}

/// 一次改动的作用域，供 Diff 按类别聚合。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeScope {
    Heading,
    Paragraph,
    List,
    Table,
    Blockquote,
    Code,
    Rule,
    Whitespace,
    Other,
}

impl ChangeScope {
    pub fn label(self) -> &'static str {
        match self {
            ChangeScope::Heading => "标题调整",
            ChangeScope::Paragraph => "段落调整",
            ChangeScope::List => "列表调整",
            ChangeScope::Table => "表格调整",
            ChangeScope::Blockquote => "引用调整",
            ChangeScope::Code => "代码块调整",
            ChangeScope::Rule => "分隔线调整",
            ChangeScope::Whitespace => "空白调整",
            ChangeScope::Other => "其他调整",
        }
    }
}

/// 规则提交的单个编辑。区间是语义层（LF 归一化正文）的字节偏移，半开区间。
#[derive(Debug, Clone)]
pub struct Edit {
    pub range: Range<usize>,
    pub replacement: String,
    pub kind: EditKind,
    pub scope: ChangeScope,
    /// 人类可读理由，进入 Diff 明细，便于逐条接受/拒绝。
    pub reason: String,
}

impl Edit {
    pub fn insert(at: usize, text: impl Into<String>, scope: ChangeScope, reason: impl Into<String>) -> Self {
        let replacement = text.into();
        let kind = if is_whitespace(&replacement) { EditKind::WhitespaceInsert } else { EditKind::Marker };
        Edit { range: at..at, replacement, kind, scope, reason: reason.into() }
    }

    /// 构造替换。`replacement` 只允许是「空白」或「结构标记」；
    /// 是否真的没动内容，最终由 Content Guard 的 token 序列比对裁决。
    pub fn replace(
        range: Range<usize>,
        text: impl Into<String>,
        scope: ChangeScope,
        reason: impl Into<String>,
    ) -> Self {
        let replacement: String = text.into();
        let kind = if replacement.is_empty() || is_whitespace(&replacement) {
            EditKind::WhitespaceRemove
        } else {
            EditKind::Marker
        };
        Edit { range, replacement, kind, scope, reason: reason.into() }
    }

    /// 仅允许改写「纯空白」区间的安全删除。
    pub fn delete_whitespace(range: Range<usize>, src: &str, reason: impl Into<String>) -> CoreResult<Self> {
        let slice = src
            .get(range.clone())
            .ok_or_else(|| CoreError::Parse("whitespace delete out of bounds".into()))?;
        if !slice.is_empty() && !is_whitespace(slice) {
            return Err(CoreError::GuardRejected {
                rule: "edits".into(),
                detail: "试图删除非空白区间".into(),
            });
        }
        Ok(Edit {
            range,
            replacement: String::new(),
            kind: EditKind::WhitespaceRemove,
            scope: ChangeScope::Whitespace,
            reason: reason.into(),
        })
    }

    #[inline]
    pub fn is_noop(&self) -> bool {
        self.range.is_empty() && self.replacement.is_empty()
    }
}

// `replace` 无法在构造时看到原文，因此用「替换文本是否全为空白」启发式分类；
// 是否真的动到正文，最终由 Content Guard 的 token 序列比对裁决。
fn is_whitespace(s: &str) -> bool {
    s.chars().all(|c| c == ' ' || c == '\t' || c == '\n' || c == '\r')
}

/// 规则的一次提交：一批编辑 + 该规则元信息。
#[derive(Debug, Clone)]
pub struct RuleEdit {
    pub rule_id: String,
    pub rule_name: String,
    pub scope: ChangeScope,
    pub edits: Vec<Edit>,
}

/// 应用一组编辑，返回新文本。
///
/// 校验（顺序稳定，五平台一致）：
/// 1. 区间必须落在字符边界；
/// 2. 区间不得重叠（含同点插入的排序规则）；
/// 3. 从后往前应用，前面的偏移保持有效。
pub fn apply_edits(src: &str, rule: &RuleEdit, protected: &[Range<usize>]) -> CoreResult<String> {
    let edits: Vec<&Edit> = rule.edits.iter().filter(|e| !e.is_noop()).collect();
    if edits.is_empty() {
        return Ok(src.to_string());
    }

    for e in &edits {
        if !src.is_char_boundary(e.range.start) || !src.is_char_boundary(e.range.end) {
            return Err(CoreError::OverlappingEdits { rule: rule.rule_id.clone() });
        }
        if e.range.end > src.len() || e.range.start > e.range.end {
            return Err(CoreError::OverlappingEdits { rule: rule.rule_id.clone() });
        }
        // 规则不得改写保护区（代码、URL、路径、命令……）
        if e.kind == EditKind::Content {
            return Err(CoreError::GuardRejected {
                rule: rule.rule_id.clone(),
                detail: "规则提交了 Content 类型编辑".into(),
            });
        }
        for p in protected {
            let overlaps = e.range.start < p.end && p.start < e.range.end;
            let touches_boundary = e.range.start == e.range.end
                && (e.range.start > p.start && e.range.start < p.end);
            if overlaps || touches_boundary {
                return Err(CoreError::GuardRejected {
                    rule: rule.rule_id.clone(),
                    detail: format!(
                        "编辑触及保护区 [{}, {})",
                        p.start, p.end
                    ),
                });
            }
        }
    }

    // 按 (start, end, 提交下标) 升序排序后正向遍历：
    // 同点多次插入保持提交顺序，且整体结果与遍历顺序无关（确定性）。
    let mut indexed: Vec<(usize, &Edit)> = edits.into_iter().enumerate().collect();
    indexed.sort_by(|(_, a), (_, b)| {
        a.range
            .start
            .cmp(&b.range.start)
            .then_with(|| a.range.end.cmp(&b.range.end))
    });

    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for (_, e) in &indexed {
        if e.range.start < cursor {
            return Err(CoreError::OverlappingEdits { rule: rule.rule_id.clone() });
        }
        out.push_str(&src[cursor..e.range.start]);
        out.push_str(&e.replacement);
        cursor = e.range.end;
    }
    out.push_str(&src[cursor..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(edits: Vec<Edit>) -> RuleEdit {
        RuleEdit { rule_id: "test".into(), rule_name: "test".into(), scope: ChangeScope::Other, edits }
    }

    #[test]
    fn applies_back_to_front() {
        let src = "hello world";
        let r = rule(vec![
            Edit::insert(0, "A", ChangeScope::Other, "x"),
            Edit::replace(6..11, "there", ChangeScope::Other, "y"),
        ]);
        assert_eq!(apply_edits(src, &r, &[]).unwrap(), "Ahello there");
    }

    #[test]
    fn rejects_overlap() {
        let src = "abcdef";
        let r = rule(vec![
            Edit::replace(1..4, "X", ChangeScope::Other, "a"),
            Edit::replace(2..5, "Y", ChangeScope::Other, "b"),
        ]);
        assert!(matches!(apply_edits(src, &r, &[]), Err(CoreError::OverlappingEdits { .. })));
    }

    #[test]
    fn rejects_protected_range() {
        let src = "a ```b``` c";
        let r = rule(vec![Edit::replace(3..6, "X", ChangeScope::Other, "a")]);
        let res = apply_edits(src, &r, &[3..8]);
        assert!(matches!(res, Err(CoreError::GuardRejected { .. })));
    }

    #[test]
    fn rejects_non_char_boundary() {
        let src = "中文字";
        let r = rule(vec![Edit::replace(1..3, "X", ChangeScope::Other, "a")]);
        assert!(matches!(apply_edits(src, &r, &[]), Err(CoreError::OverlappingEdits { .. })));
    }

    #[test]
    fn whitespace_delete_refuses_content() {
        let src = "abc  def";
        // 3..5 是两个空格，属于空白；0..3 是正文
        assert!(Edit::delete_whitespace(3..5, src, "x").is_ok());
        assert!(Edit::delete_whitespace(4..6, src, "x").is_err(), "区间含正文必须拒绝");
        assert!(Edit::delete_whitespace(0..3, src, "x").is_err());
    }

    #[test]
    fn multiple_inserts_at_same_point_keep_order() {
        let src = "ab";
        let r = rule(vec![
            Edit::insert(1, "1", ChangeScope::Other, "aaa"),
            Edit::insert(1, "2", ChangeScope::Other, "bbb"),
        ]);
        // reason 排序保证确定性
        assert_eq!(apply_edits(src, &r, &[]).unwrap(), "a12b");
    }
}
