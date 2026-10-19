//! Document Model —— 在解析器事件流之上建立可查询的文档结构。
//!
//! 规格第十五章要求「不要自行重新实现完整 Markdown 解析标准」，因此这里**不造 AST**，
//! 而是在 pulldown-cmark 事件流上叠一层轻量结构（顶层块序列 + 大纲），供分析、
//! 阅读性评分与 UI 导航使用。区间一律是语义层（LF 归一化正文）字节坐标。

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};

use crate::error::{CoreError, CoreResult};
use crate::fingerprint::PARSE_OPTIONS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading,
    Code,
    List,
    Table,
    Quote,
    Html,
    /// 分隔线 `---`
    ThematicBreak,
    FootnoteDefinition,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    pub range: Range<usize>,
    /// 标题层级（仅 Heading 有意义）
    pub level: Option<u8>,
    /// 块内文字扁平化结果，供大纲显示（最多 60 字符）
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct Document {
    pub blocks: Vec<Block>,
}

struct Frame {
    kind: BlockKind,
    start: usize,
    level: Option<u8>,
    label: String,
    /// 代码块内部文字不进 label（避免把代码当标题摘要）
    verbatim: bool,
}

impl Document {
    /// 解析出顶层块序列。解析选项与指纹层共用同一常量，保证判定一致。
    pub fn parse(src: &str) -> CoreResult<Document> {
        let mut blocks: Vec<Block> = Vec::new();
        let mut stack: Vec<Frame> = Vec::new();
        let parser = Parser::new_ext(src, PARSE_OPTIONS);

        for (event, span) in parser.into_offset_iter() {
            match event {
                Event::Start(tag) => {
                    let (kind, level, verbatim) = classify_start(&tag);
                    stack.push(Frame { kind, start: span.start, level, label: String::new(), verbatim });
                }
                Event::End(_tag) => {
                    let Some(frame) = stack.pop() else { continue };
                    let end = content_end(src, span.end);
                    let block = Block {
                        kind: frame.kind,
                        range: frame.start..end,
                        level: frame.level,
                        label: trim_label(&frame.label),
                    };
                    match stack.last_mut() {
                        // 子块并入父容器，不单独作为顶层块
                        Some(parent) => {
                            if !parent.verbatim {
                                parent.label.push(' ');
                            }
                        }
                        None => blocks.push(block),
                    }
                }
                Event::Text(t) => {
                    if let Some(top) = stack.last_mut() {
                        if !top.verbatim {
                            top.label.push_str(t.as_ref());
                        }
                    }
                }
                Event::Code(c) => {
                    if let Some(top) = stack.last_mut() {
                        if !top.verbatim {
                            top.label.push_str(c.as_ref());
                        }
                    }
                }
                Event::Rule => blocks.push(Block {
                    kind: BlockKind::ThematicBreak,
                    range: span.start..content_end(src, span.end),
                    level: None,
                    label: String::new(),
                }),
                _ => {}
            }
        }

        if !stack.is_empty() {
            return Err(CoreError::Parse("块结构未正常闭合".into()));
        }
        blocks.sort_by(|a, b| a.range.start.cmp(&b.range.start));
        Ok(Document { blocks })
    }

    /// 大纲（仅标题）：(层级, 文字, 偏移)。
    pub fn outline(&self) -> Vec<(u8, String, usize)> {
        self.blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Heading)
            .map(|b| (b.level.unwrap_or(1), b.label.clone(), b.range.start))
            .collect()
    }

    pub fn heading_count(&self) -> usize {
        self.count_of(BlockKind::Heading)
    }

    pub fn block_of(&self, offset: usize) -> Option<&Block> {
        self.blocks.iter().find(|b| b.range.contains(&offset))
    }

    pub fn count_of(&self, kind: BlockKind) -> usize {
        self.blocks.iter().filter(|b| b.kind == kind).count()
    }

    /// 代码块覆盖的区间，供 UI 标记「不可修改区域」。
    pub fn code_ranges(&self) -> Vec<Range<usize>> {
        self.blocks.iter().filter(|b| b.kind == BlockKind::Code).map(|b| b.range.clone()).collect()
    }
}

fn classify_start(tag: &Tag) -> (BlockKind, Option<u8>, bool) {
    match tag {
        Tag::Paragraph => (BlockKind::Paragraph, None, false),
        Tag::Heading { level, .. } => (BlockKind::Heading, Some(heading_level(level)), false),
        Tag::CodeBlock(_) => (BlockKind::Code, None, true),
        Tag::List(_) | Tag::Item => (BlockKind::List, None, false),
        Tag::Table(_) | Tag::TableHead | Tag::TableRow | Tag::TableCell => (BlockKind::Table, None, false),
        Tag::BlockQuote(_) => (BlockKind::Quote, None, false),
        Tag::HtmlBlock => (BlockKind::Html, None, true),
        Tag::FootnoteDefinition(_) => (BlockKind::FootnoteDefinition, None, false),
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Link { .. } | Tag::Image { .. } => {
            (BlockKind::Paragraph, None, false)
        }
        _ => (BlockKind::Paragraph, None, false),
    }
}

fn heading_level(l: &HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// 结束事件的 span 常含尾随空行；裁到实际内容末尾，避免相邻块区间重叠。
fn content_end(src: &str, raw_end: usize) -> usize {
    let end = raw_end.min(src.len());
    let cut = src[..end].trim_end_matches(['\n', ' ', '\t']).len();
    cut.max(0).min(end)
}

fn trim_label(s: &str) -> String {
    let flat: String = s.chars().map(|c| if c == '\n' || c == '\t' { ' ' } else { c }).collect();
    let t = flat.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = t.chars().take(60).collect();
    if t.chars().count() > 60 {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_headings_and_paragraphs() {
        let src = "# 一级\n\n正文段落\n\n## 二级\n\n- 项目\n";
        let doc = Document::parse(src).unwrap();
        assert_eq!(doc.count_of(BlockKind::Heading), 2);
        assert_eq!(doc.count_of(BlockKind::Paragraph), 1);
        assert_eq!(doc.count_of(BlockKind::List), 1);
        let outline = doc.outline();
        assert_eq!(outline.len(), 2);
        assert_eq!((outline[0].0, outline[0].1.as_str()), (1, "一级"));
        assert_eq!((outline[1].0, outline[1].1.as_str()), (2, "二级"));
    }

    #[test]
    fn code_block_is_its_own_block_and_label_free() {
        let src = "前段\n\n```rust\nfn a() {}\n```\n\n后段\n";
        let doc = Document::parse(src).unwrap();
        assert_eq!(doc.count_of(BlockKind::Code), 1);
        let code = doc.blocks.iter().find(|b| b.kind == BlockKind::Code).unwrap();
        assert!(src[code.range.clone()].contains("fn a()"), "区间应覆盖代码正文");
        assert!(code.label.is_empty(), "代码不应进入大纲摘要");
    }

    #[test]
    fn block_ranges_are_sorted_and_non_overlapping() {
        let src = "# A\n\n段落\n\n> 引用\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n```\nx\n```\n\n- 列表\n";
        let doc = Document::parse(src).unwrap();
        assert!(doc.blocks.len() >= 6, "样本应有多个顶层块，实际 {:?}", doc.blocks);
        for w in doc.blocks.windows(2) {
            assert!(w[0].range.end <= w[1].range.start, "块区间重叠：{:?} vs {:?}", w[0].range, w[1].range);
        }
        assert!(doc.blocks.iter().any(|b| b.kind == BlockKind::ThematicBreak));
        assert!(doc.blocks.iter().any(|b| b.kind == BlockKind::Table));
    }

    #[test]
    fn nested_list_is_one_top_level_block() {
        let src = "- 甲\n  - 乙\n  - 丙\n- 丁\n";
        let doc = Document::parse(src).unwrap();
        assert_eq!(doc.count_of(BlockKind::List), 1, "嵌套列表应为一个顶层块");
    }

    #[test]
    fn block_lookup_by_offset() {
        let src = "# 标题\n\n段落内容\n";
        let doc = Document::parse(src).unwrap();
        let at = src.find("段落内容").unwrap();
        assert_eq!(doc.block_of(at).map(|b| b.kind), Some(BlockKind::Paragraph));
        assert_eq!(doc.block_of(0).map(|b| b.kind), Some(BlockKind::Heading));
    }

    #[test]
    fn whitespace_only_source_has_no_blocks() {
        let doc = Document::parse("\n\n").unwrap();
        assert!(doc.blocks.is_empty());
    }

    #[test]
    fn outline_label_collapses_whitespace_and_truncates() {
        let long = format!("# {}标题", "很长很长很长很长很长很长很长很长很长");
        let doc = Document::parse(&long).unwrap();
        let label = &doc.outline()[0].1;
        assert!(label.chars().count() <= 61, "标签应被截断：{label}");
    }

    #[test]
    fn code_ranges_available_for_ui() {
        let src = "```\nx\n```\n\n段落\n\n~~~\ny\n~~~\n";
        let doc = Document::parse(src).unwrap();
        assert_eq!(doc.code_ranges().len(), 2);
    }

    #[test]
    fn deterministic_on_same_input() {
        let src = "# A\n\n段落\n\n- 一\n- 二\n\n| a |\n|---|\n| 1 |\n";
        let a = Document::parse(src).unwrap();
        let b = Document::parse(src).unwrap();
        assert_eq!(a.blocks.len(), b.blocks.len());
        for (x, y) in a.blocks.iter().zip(&b.blocks) {
            assert_eq!((x.kind, x.range.clone(), x.level, &x.label), (y.kind, y.range.clone(), y.level, &y.label));
        }
    }
}
