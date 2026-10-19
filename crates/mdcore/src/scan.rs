//! 行级结构扫描 —— 为规则与 Diff 提供「哪些行属于哪种结构」的确定答案。
//!
//! 为什么不直接用解析器事件？因为「空行、缩进、行尾空格」这类格式层信息
//! 在 Markdown AST 里**根本不存在**，必须回到行视图。
//!
//! 这里的分类是保守的，唯一硬性要求：**不得把代码块内部的行误判为正文**，
//! 否则「代码不可修改」会被破坏。分类错误只会导致少优化（安全方向），
//! 绝不会导致改内容（由 Content Guard 兜底）。

use crate::text::Lines;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// 空行（仅空白字符）
    Blank,
    /// ATX 标题 `# ..`
    AtxHeading,
    /// Setext 标题下划线（`===` / `---`）
    SetextUnderline,
    /// 无序列表项
    UlItem,
    /// 有序列表项
    OlItem,
    /// 引用块
    Quote,
    /// 表格行
    Table,
    /// 表格分隔行 `|---|---|`
    TableDelimiter,
    /// 分隔线 `---` / `***` / `___`
    ThematicBreak,
    /// 围栏代码块（含围栏行与内部所有行）
    FencedCode,
    /// 缩进代码块
    IndentedCode,
    /// HTML 块
    Html,
    /// 块级数学公式区（`$$ … $$`），内部一切行都不可当作 Markdown 结构
    Math,
    /// Front matter（`---` 包裹的 YAML 区）
    FrontMatter,
    /// 普通段落行
    Paragraph,
}

#[derive(Debug, Clone)]
pub struct LineInfo {
    pub kind: LineKind,
    /// 行首空白字节数（tab 记 1）
    pub indent: usize,
    /// 行尾空白字节数（用于安全删除）
    pub trailing_ws: usize,
    /// 行文本（不含换行符）
    pub text: String,
    /// 语义层字节区间（不含换行符）
    pub range: std::ops::Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Scan {
    pub lines: Vec<LineInfo>,
}

/// 只把 ASCII 空白（空格/制表/回车）视为「可增删的空白」。
///
/// 全角空格 `U+3000`、不间断空格 `U+00A0` 在中日韩文本里是**排版内容**（姓名间隔、
/// 段首缩进、表格对齐），一旦被 `str::trim()` 当作空白删掉就是真实内容损失，
/// 而 Content Guard 看不见它（`tokenize` 同样把它们当分隔符）。实测回归：
/// `正文　　\n` 曾被 R02 清成 `正文\n`，且 `check` 报「正文 0」。
#[inline]
fn ascii_trim(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r'])
}

#[inline]
fn ascii_trim_end_len(s: &str) -> usize {
    s.trim_end_matches([' ', '\t', '\r']).len()
}

impl Scan {
    pub fn new(src: &str) -> Scan {
        let view = Lines::new(src);
        let total = view.count();
        let raw: Vec<&str> = (0..total).filter_map(|n| view.line(n).map(|(t, _)| t)).collect();
        let ranges: Vec<std::ops::Range<usize>> =
            (0..total).filter_map(|n| view.line(n).map(|(_, r)| r)).collect();

        let fm = front_matter_range(&raw);
        let mut kinds = vec![LineKind::Paragraph; total];
        let mut in_fence: Option<char> = None;
        let mut in_math = false;
        let mut html_open = false;

        for n in 0..total {
            let trimmed = raw[n].trim();

            // front matter 区间整体受保护
            if let Some((s, e)) = fm {
                if n >= s && n <= e {
                    kinds[n] = LineKind::FrontMatter;
                    continue;
                }
            }

            // 围栏代码块：内部一切皆代码，绝不做任何语义判断
            if let Some(fence) = in_fence {
                kinds[n] = LineKind::FencedCode;
                if is_closing_fence(trimmed, fence) {
                    in_fence = None;
                }
                continue;
            }

            // 显示数学块 `$$ … $$`：内部既不是代码也不是正文，
            // 而是**公式**。旧实现不认识它，于是公式里单独成行的 `=`
            // 被当成 setext 标题下划线、周围的空行被 R01 压缩（实测 11-math-special.md）。
            if in_math {
                kinds[n] = LineKind::Math;
                if trimmed == "$$" {
                    in_math = false;
                }
                continue;
            }
            if trimmed == "$$" {
                kinds[n] = LineKind::Math;
                in_math = true;
                continue;
            }

            // HTML 块：从 <tag 开到底，到空行结束（保守：闭合标签或空行）
            if html_open {
                kinds[n] = LineKind::Html;
                if trimmed.is_empty() || trimmed.contains("</") {
                    html_open = false;
                }
                continue;
            }

            if ascii_trim(raw[n]).is_empty() {
                kinds[n] = LineKind::Blank;
                continue;
            }

            if let Some(c) = opening_fence_char(trimmed) {
                kinds[n] = LineKind::FencedCode;
                in_fence = Some(c);
                continue;
            }

            if is_html_block_start(trimmed) {
                kinds[n] = LineKind::Html;
                html_open = !trimmed.contains("</");
                continue;
            }

            if is_atx_heading(trimmed) {
                kinds[n] = LineKind::AtxHeading;
                continue;
            }

            // Setext 下划线必须紧跟段落行
            if n > 0
                && is_setext_underline(trimmed)
                && matches!(kinds[n - 1], LineKind::Paragraph)
            {
                kinds[n] = LineKind::SetextUnderline;
                continue;
            }

            if trimmed.starts_with('>') {
                kinds[n] = LineKind::Quote;
                continue;
            }

            // 分隔线优先于表格分隔行判定：`---` 单独成行是 ThematicBreak，
            // 只有出现在表头之后且含 `|` 或相邻表格行时才是 TableDelimiter。
            if is_thematic_break(trimmed) && !table_context(n, &raw, &kinds) {
                kinds[n] = LineKind::ThematicBreak;
                continue;
            }

            if is_table_delimiter(trimmed) {
                kinds[n] = LineKind::TableDelimiter;
                // 上一行若是含 | 的段落，则是表头
                if n > 0 && matches!(kinds[n - 1], LineKind::Paragraph) && raw[n - 1].contains('|') {
                    kinds[n - 1] = LineKind::Table;
                }
                continue;
            }

            // 表格行：紧邻分隔行才算表格，避免把含 `|` 的普通句子当表格
            if trimmed.contains('|')
                && ((n + 1 < total && is_table_delimiter(raw[n + 1].trim()))
                    || (n > 0 && matches!(kinds[n - 1], LineKind::Table | LineKind::TableDelimiter)))
            {
                kinds[n] = LineKind::Table;
                continue;
            }

            if is_ul_item(trimmed) {
                kinds[n] = LineKind::UlItem;
                continue;
            }
            if is_ol_item(trimmed) {
                kinds[n] = LineKind::OlItem;
                continue;
            }
            if is_thematic_break(trimmed) {
                kinds[n] = LineKind::ThematicBreak;
                continue;
            }

            // 缩进 4 空格 = 缩进代码（保守，不改动）
            if raw[n].len() - raw[n].trim_start().len() >= 4 {
                kinds[n] = LineKind::IndentedCode;
                continue;
            }

            kinds[n] = LineKind::Paragraph;
        }

        // 表格体内的后续行：分隔行之后的连续 | 行都标为 Table
        for n in 0..total {
            if kinds[n] == LineKind::TableDelimiter {
                let mut m = n + 1;
                while m < total && raw[m].trim().contains('|') && !raw[m].trim().is_empty() {
                    if kinds[m] == LineKind::Paragraph {
                        kinds[m] = LineKind::Table;
                    }
                    m += 1;
                }
            }
        }

        let lines = (0..total)
            .map(|n| LineInfo {
                kind: kinds[n],
                indent: raw[n].len() - raw[n].trim_start().len(),
                trailing_ws: raw[n].len() - ascii_trim_end_len(raw[n]),
                text: raw[n].to_string(),
                range: ranges[n].clone(),
            })
            .collect();
        Scan { lines }
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.lines.len()
    }

    #[inline]
    pub fn kind_at(&self, n: usize) -> Option<LineKind> {
        self.lines.get(n).map(|l| l.kind)
    }

    #[inline]
    pub fn line(&self, n: usize) -> Option<&LineInfo> {
        self.lines.get(n)
    }

    /// 是否属于「不可修改区域」。
    pub fn is_protected(&self, n: usize) -> bool {
        matches!(
            self.kind_at(n),
            Some(
                LineKind::FencedCode
                    | LineKind::IndentedCode
                    | LineKind::Html
                    | LineKind::FrontMatter
                    | LineKind::Math
                    | LineKind::TableDelimiter
            )
        )
    }

    /// 把行号区间 [a, b] 映射为语义层字节区间（含行间换行）。
    pub fn span(&self, a: usize, b: usize) -> Option<std::ops::Range<usize>> {
        let start = self.lines.get(a)?.range.start;
        let end = self.lines.get(b)?.range.end;
        Some(start..end)
    }
}

/// 判断这一行是否处在表格上下文中（上一行是含 `|` 的行）。
/// 用于区分 `---` 分隔线与单列表格的 `---` 分隔行。
fn table_context(n: usize, raw: &[&str], kinds: &[LineKind]) -> bool {
    n > 0 && raw[n - 1].contains('|') && !matches!(kinds[n - 1], LineKind::Blank)
}

/// 判断文件开头的 front matter 区间（必须成对闭合，否则不算）。
///
/// 两种形态都支持：YAML `--- … ---|...` 与 TOML `+++ … +++`。
/// 旧实现只认 `---`，于是 Hugo/Zola 风格的 `+++` 元数据被当成普通段落，
/// R01/R02/R11 会往 `title = "说明：甲、乙、丙"` 里插换行 —— 元数据直接损坏
/// （实测：60 字节语料优化后 61 字节，Guard 仍报「正文 0」）。
fn front_matter_range(raw: &[&str]) -> Option<(usize, usize)> {
    let first = raw.first().map(|s| s.trim())?;
    let toml = first == "+++";
    if !toml && first != "---" {
        return None;
    }
    for n in 1..raw.len() {
        let t = raw[n].trim();
        let closed = if toml { t == "+++" } else { t == "---" || t == "..." };
        if closed {
            return Some((0, n));
        }
        // front matter 内部不应出现 markdown 结构；超 60 行视为误判
        if n > 60 {
            return None;
        }
    }
    None
}

fn opening_fence_char(trimmed: &str) -> Option<char> {
    let c = trimmed.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let run = c.to_string().repeat(3);
    trimmed.starts_with(&run).then_some(c)
}

/// 结束围栏：只有围栏字符与空白，且没有信息串。
fn is_closing_fence(trimmed: &str, c: char) -> bool {
    let run = c.to_string().repeat(3);
    trimmed.starts_with(&run) && trimmed.chars().all(|ch| ch == c || ch.is_whitespace())
}

fn is_html_block_start(trimmed: &str) -> bool {
    let rest = trimmed.strip_prefix("<").unwrap_or("");
    let name: String = rest.chars().take_while(|ch| ch.is_ascii_alphabetic()).collect();
    !name.is_empty()
}

fn is_atx_heading(trimmed: &str) -> bool {
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return false;
    }
    let rest = &trimmed[hashes..];
    // `#hashtag` 不是标题；`# ` / `#\t` / `#` 行尾才是
    rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')
}

fn is_setext_underline(trimmed: &str) -> bool {
    !trimmed.is_empty() && (trimmed.chars().all(|c| c == '=') || trimmed.chars().all(|c| c == '-'))
}

fn is_thematic_break(trimmed: &str) -> bool {
    let compact: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && (compact.chars().all(|c| c == '-') || compact.chars().all(|c| c == '*') || compact.chars().all(|c| c == '_'))
}

fn is_ul_item(trimmed: &str) -> bool {
    let mut chars = trimmed.chars();
    match chars.next() {
        Some('-' | '*' | '+') => matches!(chars.next(), Some(' ') | Some('\t') | None),
        _ => false,
    }
}

fn is_ol_item(trimmed: &str) -> bool {
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return false;
    }
    let rest = &trimmed[digits..];
    rest.starts_with(". ")
        || rest.starts_with(".\t")
        || rest.starts_with(") ")
        || rest.starts_with(")\t")
        || rest.starts_with(".") && rest.len() == 1
}

fn is_table_delimiter(trimmed: &str) -> bool {
    let s = trimmed.trim();
    if !s.contains('-') {
        return false;
    }
    let cells: Vec<&str> = s.trim_start_matches('|').trim_end_matches('|').split('|').collect();
    !cells.is_empty()
        && cells.iter().all(|c| {
            let c = c.trim();
            !c.is_empty() && c.contains('-') && c.chars().all(|ch| ch == '-' || ch == ':')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<LineKind> {
        let s = Scan::new(src);
        (0..s.count()).map(|i| s.kind_at(i).unwrap()).collect()
    }

    #[test]
    fn basic_classification() {
        let src = "# 标题\n\n正文第一段\n\n- 项目一\n- 项目二\n";
        assert_eq!(
            kinds(src),
            vec![
                LineKind::AtxHeading,
                LineKind::Blank,
                LineKind::Paragraph,
                LineKind::Blank,
                LineKind::UlItem,
                LineKind::UlItem,
            ]
        );
    }

    #[test]
    fn fenced_code_never_misread_as_content() {
        let src = "说明\n\n```python\n# 这行看着像标题\ncode = True\n```\n\n正文\n";
        let s = Scan::new(src);
        assert_eq!(kinds(src), vec![
            LineKind::Paragraph,
            LineKind::Blank,
            LineKind::FencedCode,
            LineKind::FencedCode,
            LineKind::FencedCode,
            LineKind::FencedCode,
            LineKind::Blank,
            LineKind::Paragraph,
        ]);
        let hash_line = src.lines().position(|l| l.contains("# 这行")).unwrap();
        assert_eq!(s.kind_at(hash_line), Some(LineKind::FencedCode));
        assert!(s.is_protected(hash_line));
    }

    #[test]
    fn tilde_fence_and_nested_backticks() {
        let src = "~~~\n```\nnot a fence start\n~~~\nafter\n";
        let s = Scan::new(src);
        assert_eq!(s.kind_at(3), Some(LineKind::FencedCode));
        assert_eq!(s.kind_at(4), Some(LineKind::Paragraph));
    }

    #[test]
    fn table_rows_and_delimiter() {
        let src = "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n";
        assert_eq!(
            kinds(src),
            vec![
                LineKind::Table,
                LineKind::TableDelimiter,
                LineKind::Table,
                LineKind::Table,
            ]
        );
    }

    #[test]
    fn fullwidth_space_is_content_not_blank() {
        // 全角空格行 / 行尾全角空格都属于中文排版内容
        assert_eq!(kinds("第一段\n　\n第二段\n"), vec![
            LineKind::Paragraph,
            LineKind::Paragraph,
            LineKind::Paragraph,
        ]);
        let s = Scan::new("正文　　\n");
        assert_eq!(s.lines[0].trailing_ws, 0, "行尾全角空格不得计入可删除空白");
        let s2 = Scan::new("正文\u{00a0}\n");
        assert_eq!(s2.lines[0].trailing_ws, 0, "NBSP 不得计入可删除空白");
        let s3 = Scan::new("正文   \n");
        assert_eq!(s3.lines[0].trailing_ws, 3, "半角空格仍照常清理");
    }

    #[test]
    fn toml_front_matter_is_protected() {
        let src = "+++\ntitle = \"说明：甲、乙\"\n+++";
        let s = Scan::new(src);
        assert!(s.is_protected(0) && s.is_protected(1) && s.is_protected(2), "TOML 元数据区必须整块受保护");
    }

    #[test]
    fn front_matter_protected_and_not_confused_with_break() {
        let src = "+++\ntitle = \"x\"\n\n\n\n正文\n";
        let s = Scan::new(src);
        // 没有闭合的 +++ 不算 front matter（保持保守：当成普通内容）
        assert_ne!(s.kind_at(0), Some(LineKind::FrontMatter));
    }

    #[test]
    fn display_math_block_is_protected() {
        // 真实回归：公式里单独成行的 `=` 曾被当成 setext 标题下划线
        let src = "$$\n\\begin{pmatrix}\nx \\\\\ny\n\\end{pmatrix}\n=\n\\begin{pmatrix}\na\n\\end{pmatrix}\n$$\n\n正文\n";
        let s = Scan::new(src);
        assert!(s.is_protected(0) && s.is_protected(5) && s.is_protected(9), "公式块整块受保护");
        assert_ne!(s.kind_at(5), Some(LineKind::SetextUnderline), "公式里的 = 不是标题下划线");
        assert_eq!(s.kind_at(11), Some(LineKind::Paragraph), "块结束后恢复正常");
    }

    #[test]
    fn thematic_break_not_confused() {
        assert_eq!(kinds("---\n"), vec![LineKind::ThematicBreak]);
        assert_eq!(kinds("***\n"), vec![LineKind::ThematicBreak]);
        assert_eq!(kinds("- x\n"), vec![LineKind::UlItem]);
        assert_eq!(kinds("标题\n===\n"), vec![LineKind::Paragraph, LineKind::SetextUnderline]);
        // `#` 井号标签不是标题
        assert_eq!(kinds("#hashtag\n"), vec![LineKind::Paragraph]);
    }

    #[test]
    fn front_matter_protected_and_not_when_unclosed() {
        let src = "---\ntitle: x\ntags: [a, b]\n---\n\n# 正文\n";
        let s = Scan::new(src);
        assert!(s.is_protected(1) && s.is_protected(2));
        assert_eq!(s.kind_at(3), Some(LineKind::FrontMatter));
        assert_eq!(s.kind_at(5), Some(LineKind::AtxHeading));

        // 未闭合：不当作 front matter
        let bad = "---\ntitle: x\nmore\n";
        assert_eq!(Scan::new(bad).kind_at(1), Some(LineKind::Paragraph));
    }

    #[test]
    fn ordered_items() {
        assert_eq!(kinds("1. 一\n2. 二\n"), vec![LineKind::OlItem, LineKind::OlItem]);
        assert_eq!(kinds("10) 十\n"), vec![LineKind::OlItem]);
    }

    #[test]
    fn indent_and_trailing_measured() {
        let s = Scan::new("  - 项目 \n");
        assert_eq!(s.lines[0].indent, 2);
        assert_eq!(s.lines[0].trailing_ws, 1);
    }

    #[test]
    fn span_maps_to_source() {
        let src = "# 标题\n正文\n第三\n";
        let s = Scan::new(src);
        let r = s.span(1, 2).unwrap();
        assert_eq!(&src[r.clone()], "正文\n第三");
        assert_eq!(&src[s.lines[0].range.clone()], "# 标题");
    }

    #[test]
    fn html_block() {
        let src = "<div>\ninner\n</div>\n\n正文\n";
        let s = Scan::new(src);
        assert_eq!(s.kind_at(1), Some(LineKind::Html));
        assert!(s.is_protected(1));
        assert_eq!(s.kind_at(4), Some(LineKind::Paragraph));
    }

    #[test]
    fn pipe_in_sentence_is_not_table() {
        let src = "命令 a | b 表示管道\n";
        assert_eq!(Scan::new(src).kind_at(0), Some(LineKind::Paragraph));
    }

    #[test]
    fn indented_code_detected() {
        let src = "    indented = True\n";
        assert_eq!(Scan::new(src).kind_at(0), Some(LineKind::IndentedCode));
    }

    #[test]
    fn empty_file_scan_is_empty() {
        assert_eq!(Scan::new("").count(), 1);
        assert_eq!(Scan::new("").kind_at(0), Some(LineKind::Blank));
    }
}
