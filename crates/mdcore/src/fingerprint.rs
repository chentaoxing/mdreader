//! Content Fingerprint —— 内容 token 化与指纹（规格第二十三章）。
//!
//! 关键设计：**不能只比较字符数量**，必须比较 **内容 token 序列**。
//!
//! 我们把 Markdown 拆成两类：
//! - 结构（structure）：`#`、`-`、`*`、`>`、`1.`、表格分隔行、`---`、空白、行尾、缩进
//!   —— 允许改变，因此**不进入** token 序列。
//! - 内容（content）：文字 token、代码、URL、路径、命令、数字、标点、HTML 片段
//!   —— 一个都不能增删改，全部进入 token 序列，且顺序敏感。
//!
//! ## 「内容」的精确口径（实测 pulldown-cmark 0.13.4 后确定）
//!
//! 内容 = **读者感知到的渲染语义文本**，即解析器 Text/Code 事件给出的**已解码文本**，
//! 而不是原始字节。原因：反斜杠转义在 Markdown 里是语法标记，不是正文。
//!
//! - `50\%` → Text("50") + Text("%…")：`\` 不在任何 span 内，属于结构。渲染结果
//!   仍是 `50%`，读者看到的东西没变 → 允许。
//! - `\*not em\*` → Text("*not em") + Text("*…")：反斜杠**承载语法含义**。
//!   若删掉它，解析器改产出 Emphasis，token 序列变成 `not`/`em`（少两个 `*`），
//!   序列不一致 → Guard 拒绝。
//!
//! 校验和用自实现 FNV-1a 64（不引入哈希 crate、不依赖平台 HashMap 随机种子），
//! 保证五平台同输入同输出。

use std::ops::Range;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// 内容 token 类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// 单词 / 连续文字片段（CJK 按字符切，拉丁按词切）
    Word,
    /// 标点符号（含中文全角标点）。标点属于内容，不得删除。
    Punct,
    /// 代码块正文 / 行内代码（逐字节保留，不切词）
    Code,
    /// 链接 / 图片的目标地址（URL、路径、锚点）
    Url,
    /// 原始 HTML / 内联 HTML
    Html,
    /// 引用键、脚注标签等标识符
    Ref,
}

#[derive(Debug, Clone)]
pub struct ContentToken {
    pub kind: TokenKind,
    pub text: String,
    /// 在归一化正文中的位置（仅诊断用）
    pub offset: usize,
}

/// 相等性**只取决于 kind 与 text**。
///
/// offset 是诊断信息：任何结构改写都会让后续 token 位置移动，
/// 若让 offset 参与比较，所有合法优化都会被误判为内容变化。
impl PartialEq for ContentToken {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.text == other.text
    }
}

impl Eq for ContentToken {}

/// 内容指纹。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub tokens: Vec<ContentToken>,
    /// FNV-1a 64 over the token stream（不是 over 原始字节，避免结构变化影响）
    pub checksum: u64,
    pub counts: TokenCounts,
    /// 源文本中反斜杠总个数——独立于 token 序列的兜底检查。
    ///
    /// token 序列基于解码文本，无法区分 `50\%` 与 `50%`（渲染相同），
    /// 因此额外要求 `\` 数量完全相等，杜绝「悄悄吃掉转义符」。
    pub backslashes: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenCounts {
    pub words: usize,
    pub puncts: usize,
    pub code_bytes: usize,
    pub urls: usize,
    pub html_bytes: usize,
    pub refs: usize,
}

/// 解析选项。五平台一致：写死，不接受平台相关默认值。
///
/// 刻意不开启 `ENABLE_SMART_PUNCTUATION`——它会把 `--` 变成 `–`，属于改内容。
pub const PARSE_OPTIONS: Options = Options::empty()
    .union(Options::ENABLE_TABLES)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
    .union(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);

/// 把 Markdown 正文切成内容 token 序列。
///
/// 结构标记（`#`、`-`、`|`、`>`、反引号、括号）天然不出现在 Text/Code/Html
/// 事件文本里，因此无需手工剥离。
pub fn tokenize(src: &str) -> Fingerprint {
    let mut tokens: Vec<ContentToken> = Vec::new();
    let mut counts = TokenCounts::default();
    let parser = Parser::new_ext(src, PARSE_OPTIONS);

    // CodeBlock 内部的 Text 是代码正文，逐字节原样保留，不切词。
    let mut in_code_block = false;

    for (event, span) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => in_code_block = true,
            Event::End(TagEnd::CodeBlock) => in_code_block = false,
            Event::Start(Tag::Link { dest_url, .. }) | Event::Start(Tag::Image { dest_url, .. }) => {
                if !dest_url.is_empty() {
                    push_verbatim(&mut tokens, &mut counts, TokenKind::Url, &dest_url, span.start);
                    counts.urls += 1;
                }
            }
            Event::Code(code) => {
                push_verbatim(&mut tokens, &mut counts, TokenKind::Code, code.as_ref(), span.start);
            }
            Event::Text(text) if in_code_block => {
                push_verbatim(&mut tokens, &mut counts, TokenKind::Code, text.as_ref(), span.start);
            }
            Event::Text(text) => push_text_tokens(&mut tokens, &mut counts, text.as_ref(), span.start),
            Event::Html(html) | Event::InlineHtml(html) => {
                push_verbatim(&mut tokens, &mut counts, TokenKind::Html, html.as_ref(), span.start);
            }
            // 脚注引用 `[^1]` 与脚注定义标签 `[^note]:`：解析器把它们放进事件里而不是 Text 里，
            // 早先它们对 Guard 完全隐形——规则吃掉一个引用编号，校验和照样"前后一致"。
            // 现在按源文本的样子还原成 Ref token，编号没了就是内容变了。
            Event::FootnoteReference(name) => {
                let t = format!("[^{}]", name.as_ref());
                push_verbatim(&mut tokens, &mut counts, TokenKind::Ref, &t, span.start);
            }
            Event::Start(Tag::FootnoteDefinition(name)) => {
                let t = format!("[^{}]", name.as_ref());
                push_verbatim(&mut tokens, &mut counts, TokenKind::Ref, &t, span.start);
            }
            // 任务列表的 `[ ]` / `[x]` 同理：被解析器消化成独立事件，不出现在任何 Text 里。
            // 勾选状态是读者要的信息（这件事做没做），丢了就等于改内容。
            Event::TaskListMarker(checked) => {
                let t = if checked { "[x]" } else { "[ ]" };
                push_verbatim(&mut tokens, &mut counts, TokenKind::Ref, t, span.start);
            }
            Event::SoftBreak | Event::HardBreak => {
                // 换行属于结构层，不进入内容 token。
            }
            _ => {}
        }
    }

    let checksum = fnv1a64(&tokens);
    Fingerprint {
        tokens,
        checksum,
        counts,
        backslashes: src.bytes().filter(|b| *b == b'\\').count(),
    }
}

/// 原文类 token（代码 / HTML / URL）：不切词、不 trim，保留全部字节。
fn push_verbatim(
    tokens: &mut Vec<ContentToken>,
    counts: &mut TokenCounts,
    kind: TokenKind,
    s: &str,
    offset: usize,
) {
    if s.is_empty() {
        return;
    }
    match kind {
        TokenKind::Code => counts.code_bytes += s.len(),
        TokenKind::Html => counts.html_bytes += s.len(),
        TokenKind::Ref => counts.refs += 1,
        _ => {}
    }
    tokens.push(ContentToken { kind, text: s.to_string(), offset });
}

/// 文字类 token：按「词 / 标点」切分。规则完全确定，不依赖 locale：
/// - CJK / 假名 / 谚文 → 每字符一个 Word（这些文字没有空格分词）
/// - 字母数字与 `_` `-` → 聚合成 Word
/// - 空白 → 丢弃（结构）
/// - 其余可打印字符 → 标点，逐字符独立成 token（全角「，。」一视同仁）
fn push_text_tokens(
    tokens: &mut Vec<ContentToken>,
    counts: &mut TokenCounts,
    s: &str,
    offset: usize,
) {
    let mut buf = String::new();
    let mut buf_start = offset;

    // 闭包会同时借用 tokens/counts，故用内部函数 + 显式传参。
    fn flush(
        buf: &mut String,
        start: usize,
        kind: TokenKind,
        tokens: &mut Vec<ContentToken>,
        counts: &mut TokenCounts,
    ) {
        if buf.is_empty() {
            return;
        }
        let text = std::mem::take(buf);
        tokens.push(ContentToken { kind, text, offset: start });
        match kind {
            TokenKind::Word => counts.words += 1,
            _ => counts.puncts += 1,
        }
    }

    for (i, ch) in s.char_indices() {
        let abs = offset + i;
        if ch.is_whitespace() {
            flush(&mut buf, buf_start, TokenKind::Word, tokens, counts);
            buf_start = abs + ch.len_utf8();
            continue;
        }
        if is_cjk_ideograph(ch) {
            flush(&mut buf, buf_start, TokenKind::Word, tokens, counts);
            tokens.push(ContentToken { kind: TokenKind::Word, text: ch.to_string(), offset: abs });
            counts.words += 1;
            buf_start = abs + ch.len_utf8();
        } else if is_word_char(ch) {
            if buf.is_empty() {
                buf_start = abs;
            }
            // 词字符不得接在标点后（否则会把 `。a` 合成一个 token，
            // 与拆成两个 token 的版本产生歧义）
            if !buf.is_empty() && !buf.chars().next().map(is_word_char).unwrap_or(true) {
                flush(&mut buf, buf_start, TokenKind::Punct, tokens, counts);
                buf_start = abs;
            }
            buf.push(ch);
        } else {
            flush(&mut buf, buf_start, TokenKind::Word, tokens, counts);
            buf.push(ch);
            flush(&mut buf, abs, TokenKind::Punct, tokens, counts);
            buf_start = abs + ch.len_utf8();
        }
    }
    flush(&mut buf, buf_start, TokenKind::Word, tokens, counts);
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_' || ch == '\'' || ch == '’'
}

fn is_cjk_ideograph(ch: char) -> bool {
    is_cjk_char(ch)
}

/// CJK / 假名 / 谚文判定。规则自检也要用它，故公开。
pub fn is_cjk_char(ch: char) -> bool {
    matches!(ch as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF
      | 0x20000..=0x2A6DF | 0x2A700..=0x2EBEF
      | 0x3040..=0x30FF | 0x31F0..=0x31FF
      | 0xAC00..=0xD7AF | 0x1100..=0x11FF | 0xFF66..=0xFF9F)
}

/// FNV-1a 64，逐 token 混合，顺序敏感。
fn fnv1a64(tokens: &[ContentToken]) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for t in tokens {
        h = (h ^ (t.kind as u64)).wrapping_mul(PRIME);
        for b in t.text.as_bytes() {
            h = (h ^ (*b as u64)).wrapping_mul(PRIME);
        }
        // 分隔符参与混合，避免 "ab"+"c" 与 "a"+"bc" 同哈希
        h = (h ^ 0x1f).wrapping_mul(PRIME);
    }
    h
}

/// token 序列差异定位：返回第一个不一致的下标与两边长度。
pub fn first_divergence(a: &[ContentToken], b: &[ContentToken]) -> Option<(usize, usize, usize)> {
    let min = a.len().min(b.len());
    for i in 0..min {
        if a[i].text != b[i].text || a[i].kind != b[i].kind {
            return Some((i, a.len(), b.len()));
        }
    }
    if a.len() != b.len() {
        Some((min, a.len(), b.len()))
    } else {
        None
    }
}

/// 保护区间：代码块、行内代码、HTML、链接目标。规则不得改写这些区间。
pub fn protected_ranges(src: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let parser = Parser::new_ext(src, PARSE_OPTIONS);
    for (event, span) in parser.into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) | Event::Code(_) | Event::InlineHtml(_) => out.push(span),
            Event::Start(Tag::HtmlBlock) => out.push(span),
            Event::Html(html) if !html.trim().is_empty() => out.push(span),
            Event::Start(Tag::Link { dest_url, .. }) | Event::Start(Tag::Image { dest_url, .. }) => {
                // 只保护目标文本所在区间；找不到时退化为整个链接区间。
                match src[span.clone()].find(dest_url.as_ref()) {
                    Some(pos) => {
                        let s = span.start + pos;
                        out.push(s..s + dest_url.len());
                    }
                    None => out.push(span),
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| a.start.cmp(&b.start).then(a.end.cmp(&b.end)));
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(out.len());
    for r in out {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => {
                if r.end > last.end {
                    last.end = r.end;
                }
            }
            _ => merged.push(r),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(fp: &Fingerprint) -> Vec<String> {
        fp.tokens.iter().map(|t| t.text.clone()).collect()
    }

    #[test]
    fn structure_markers_are_not_content() {
        let a = tokenize("# 标题\n\n正文内容\n");
        let b = tokenize("## 标题\n\n正文内容\n");
        assert_eq!(a.checksum, b.checksum, "标题层级变化不得影响内容指纹");
        assert_eq!(texts(&a), texts(&b));
    }

    #[test]
    fn list_conversion_preserves_tokens() {
        let a = tokenize("问题：成本高、部署复杂、维护困难。\n");
        let b = tokenize("问题：\n\n1. 成本高、\n2. 部署复杂、\n3. 维护困难。\n");
        assert_eq!(a.tokens, b.tokens, "转列表只改结构，token 序列必须一致");
    }

    #[test]
    fn punctuation_deletion_detected() {
        let a = tokenize("成本高、部署复杂");
        let b = tokenize("成本高 部署复杂");
        assert_ne!(a.checksum, b.checksum, "删除顿号必须被发现");
        assert!(first_divergence(&a.tokens, &b.tokens).is_some());
    }

    #[test]
    fn code_block_is_verbatim_token() {
        let a = tokenize("```python\nx = 1 +  2\n```\n");
        assert!(a
            .tokens
            .iter()
            .any(|t| t.kind == TokenKind::Code && t.text == "x = 1 +  2\n"));
    }

    #[test]
    fn code_whitespace_changes_are_detected() {
        let a = tokenize("```\nf(a, b)\n```\n");
        let b = tokenize("```\nf(a,b)\n```\n");
        assert_ne!(a.checksum, b.checksum, "代码内空格属于内容");
    }

    #[test]
    fn inline_code_body_is_content() {
        let a = tokenize("用 `git status` 查看");
        assert!(a.tokens.iter().any(|t| t.kind == TokenKind::Code && t.text == "git status"));
    }

    #[test]
    fn cjk_splits_per_character() {
        let a = tokenize("中文测试");
        assert_eq!(a.counts.words, 4);
    }

    #[test]
    fn emoji_and_fullwidth_survive() {
        let a = tokenize("好 😀 ｆｕｌｌ");
        assert!(a.tokens.iter().any(|t| t.text == "😀"));
        assert!(a.tokens.iter().any(|t| t.text == "ｆｕｌｌ"));
    }

    #[test]
    fn url_is_a_single_token() {
        let a = tokenize("[链接](https://example.com/a%20b#c)");
        assert!(a
            .tokens
            .iter()
            .any(|t| t.kind == TokenKind::Url && t.text == "https://example.com/a%20b#c"));
    }

    #[test]
    fn colon_field_to_table_would_drop_punctuation() {
        // 关键实证结论：规格第二十章示例
        //   名称：xxx  →  | 名称 | xxx |
        // 会删除全角冒号「：」，而标点属于内容（第 2.1 节「默认禁止改变：标点」）。
        // 因此该转换**必然**被 Content Guard 拒绝，v1 不得实现（除非保留冒号）。
        let a = tokenize("名称：xxx\n");
        let b = tokenize("| 名称 | xxx |\n|---|---|\n");
        assert!(first_divergence(&a.tokens, &b.tokens).is_some());
        let at: Vec<&str> = a.tokens.iter().map(|t| t.text.as_str()).collect();
        assert!(at.contains(&"："), "冒号原本是内容 token");
        let bt: Vec<&str> = b.tokens.iter().map(|t| t.text.as_str()).collect();
        assert!(!bt.contains(&"："), "表格化后冒号消失 → Guard 会拒绝");
    }

    #[test]
    fn table_cell_preserving_all_tokens_passes() {
        // 允许的形式：只做结构改写，冒号等内容原样保留
        let a = tokenize("名称：xxx\n版本：yyy\n");
        let b = tokenize("名称：xxx\n\n版本：yyy\n");
        assert_eq!(a.tokens, b.tokens);
    }

    #[test]
    fn backslash_count_is_recorded() {
        assert_eq!(tokenize("50\\%").backslashes, 1);
        assert_eq!(tokenize("50%").backslashes, 0);
        assert_eq!(tokenize("a\\\\b").backslashes, 2);
    }

    #[test]
    fn emphasis_star_deletion_is_detected() {
        // 删掉 ** 会改变内容 token（序列里出现 * ）
        let a = tokenize("\\*not em\\*");
        let b = tokenize("not em");
        assert_ne!(a.checksum, b.checksum);
        assert_ne!(a.backslashes, b.backslashes);
    }

    #[test]
    fn protected_ranges_cover_code_and_url() {
        let src = "文本 `inline` 与 [x](http://a.b/c) 还有\n\n```\ncode\n```\n";
        let rs = protected_ranges(src);
        for needle in ["inline", "http://a.b/c", "code"] {
            let at = src.find(needle).unwrap();
            assert!(rs.iter().any(|r| r.contains(&at)), "{needle} 未被保护");
        }
    }

    #[test]
    fn deterministic_across_calls() {
        let s = "# A\n\n- x\n- y\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        assert_eq!(tokenize(s), tokenize(s));
    }

    // ---- 2026-10-19 起：脚注标记与任务符号纳入正文可见范围 ------------------

    #[test]
    fn footnote_reference_is_content() {
        let a = tokenize("见此处[^1]\n\n[^1]: 注\n");
        let b = tokenize("见此处\n\n[^1]: 注\n");
        assert_ne!(a.checksum, b.checksum, "吃掉脚注引用编号必须被发现");
        assert!(first_divergence(&a.tokens, &b.tokens).is_some());
    }

    #[test]
    fn footnote_definition_label_is_content() {
        let a = tokenize("[^note]: 说明\n");
        let b = tokenize("说明\n");
        assert_ne!(a.checksum, b.checksum, "定义标签没了，引用就成了悬空");
    }

    #[test]
    fn task_marker_is_content() {
        let a = tokenize("- [ ] 待办\n");
        let b = tokenize("- 待办\n");
        assert_ne!(a.checksum, b.checksum, "任务符号承载「做没做」，属于内容");
        assert_ne!(
            tokenize("- [ ] 甲\n").checksum,
            tokenize("- [x] 甲\n").checksum,
            "勾选状态变化同样是内容变化"
        );
    }

    #[test]
    fn ref_tokens_are_counted_and_reproducible() {
        let s = "- [ ] 甲[^n]\n\n[^n]: 注\n";
        let a = tokenize(s);
        let refs: Vec<&str> = a
            .tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Ref)
            .map(|t| t.text.as_str())
            .collect();
        assert_eq!(refs, vec!["[ ]", "[^n]", "[^n]"], "实际 token 流：{:?}", refs);
        assert_eq!(a.counts.refs, refs.len());
        assert_eq!(a, tokenize(s), "同一输入两次解析必须逐位一致");
    }
}
