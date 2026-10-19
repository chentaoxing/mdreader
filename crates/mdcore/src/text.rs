//! 语义内容层 vs 文件格式层的分离（对应产品规格第三十三、三十四章）。
//!
//! 核心思想：
//! - **格式层**：BOM、行尾符（LF / CRLF / CR）、文件末尾换行。这些属于文件编码形式，
//!   改变它们**不算内容变化**，但也不能被误判为内容变化。
//! - **语义层**：正文 token。一个字符都不能变。
//!
//! 规则一律在 [`TextLayer::body`]（已归一化为 LF、无 BOM）的字节坐标上工作，
//! 输出时再按 [`NewlineStyle`] 还原。这样同一段 Markdown 在五个平台上得到的字节级
//! 分析结果完全一致，不会因为 Windows CRLF / Linux LF 而给出不同判断。
//!
//! 重要：**绝不做 Unicode normalization**（NFC/NFD）。归一化会改变「é」「ﾊﾝｸｦ」
//! 等字符的码位，属于修改内容。我们只保留原始码位。

use crate::error::{CoreError, CoreResult};

/// 输出行尾风格。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewlineStyle {
    /// 保持输入探测到的风格（默认）。输入风格混杂时降级为 LF。
    #[default]
    Preserve,
    /// 统一输出 LF（Unix / Android / iOS / Linux 友好）。
    Lf,
    /// 统一输出 CRLF（Windows 友好）。
    CrLf,
    /// 统一输出 CR（极少见，保留兼容能力）。
    Cr,
}

/// 文件的格式层视图。
#[derive(Debug, Clone)]
pub struct TextLayer {
    /// 归一化后的正文：无 BOM、行尾统一为 `\n`。规则的坐标系。
    body: String,
    bom: bool,
    /// 探测到的输入风格，供 `NewlineStyle::Preserve` 使用。
    detected: NewlineStyle,
    /// 是否只有一种行尾风格。混杂时为 false，Preserve 降级 LF。
    uniform: bool,
    /// 原文件是否以换行结尾（决定输出是否补末尾换行）。
    trailing_newline: bool,
}

impl TextLayer {
    /// 从原始字符串探测格式层并构造语义层。
    pub fn detect(source: &str) -> Self {
        const BOM: char = '\u{feff}';
        let bom = source.starts_with(BOM);
        let without_bom = if bom { &source[BOM.len_utf8()..] } else { source };

        let (mut lf, mut crlf, mut cr) = (0usize, 0usize, 0usize);
        let bytes = without_bom.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'\r' => {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                        crlf += 1;
                        i += 2;
                    } else {
                        cr += 1;
                        i += 1;
                    }
                }
                b'\n' => {
                    lf += 1;
                    i += 1;
                }
                _ => i += 1,
            }
        }

        let present = [lf > 0, crlf > 0, cr > 0].into_iter().filter(|on| *on).count();
        let uniform = present <= 1;
        let detected = if crlf > 0 && lf == 0 && cr == 0 {
            NewlineStyle::CrLf
        } else if cr > 0 && lf == 0 && crlf == 0 {
            NewlineStyle::Cr
        } else {
            NewlineStyle::Lf
        };

        let trailing_newline = without_bom.ends_with('\n') || without_bom.ends_with('\r');

        // 归一化：CRLF / CR -> LF。使用字符流避免二次替换错误。
        let mut body = String::with_capacity(without_bom.len());
        let mut iter = without_bom.chars().peekable();
        while let Some(c) = iter.next() {
            if c == '\r' {
                if iter.peek() == Some(&'\n') {
                    iter.next();
                }
                body.push('\n');
            } else {
                body.push(c);
            }
        }

        Self { body, bom, detected, uniform, trailing_newline }
    }

    /// 从字节切片构造；非 UTF-8 直接报错，绝不猜测编码。
    pub fn from_bytes(bytes: &[u8]) -> CoreResult<Self> {
        let s = std::str::from_utf8(bytes).map_err(|_| CoreError::InvalidUtf8)?;
        Ok(Self::detect(s))
    }

    /// 规则工作的坐标系（LF、无 BOM）。
    #[inline]
    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn has_bom(&self) -> bool {
        self.bom
    }

    /// 输入是否只有一种行尾风格。混杂时上层应明确告知用户。
    pub fn line_endings_uniform(&self) -> bool {
        self.uniform
    }

    pub fn detected_style(&self) -> NewlineStyle {
        self.detected
    }

    pub fn ends_with_newline(&self) -> bool {
        self.trailing_newline
    }

    /// 把语义层结果渲染回文件层字节。
    ///
    /// 保证：只改行尾与 BOM，不触碰任何正文字符。
    pub fn render(&self, optimized_body: &str, style: NewlineStyle) -> Vec<u8> {
        let style = match style {
            NewlineStyle::Preserve if self.uniform => self.detected,
            NewlineStyle::Preserve => NewlineStyle::Lf,
            other => other,
        };

        let mut out = String::with_capacity(optimized_body.len() + 8);
        if self.bom {
            out.push('\u{feff}');
        }
        // 先去掉尾部多余换行，再按需补，避免 CRLF 文件出现 \r\r\n。
        let trimmed = optimized_body.trim_end_matches(['\n', '\r']);
        let sep: &str = match style {
            NewlineStyle::Lf => "\n",
            NewlineStyle::CrLf => "\r\n",
            NewlineStyle::Cr => "\r",
            NewlineStyle::Preserve => unreachable!("resolved above"),
        };
        let mut first = true;
        for line in trimmed.split('\n') {
            if !first {
                out.push_str(sep);
            }
            first = false;
            // 正文里残留的 \r 属于内容字节，只在格式层重写时丢弃。
            out.push_str(line.trim_end_matches('\r'));
        }
        if self.trailing_newline {
            out.push_str(sep);
        }
        out.into_bytes()
    }
}

/// 按 LF 行切分的行视图（半开区间字节坐标，不含 `\n`）。
#[derive(Debug, Clone)]
pub struct Lines<'a> {
    src: &'a str,
    /// 每行起始字节偏移。
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut starts = vec![0usize];
        for (i, b) in src.bytes().enumerate() {
            if b == b'\n' && i + 1 <= src.len() {
                starts.push(i + 1);
            }
        }
        if starts.last().copied() == Some(src.len()) && !src.is_empty() {
            starts.pop();
        }
        Self { src, starts }
    }

    pub fn count(&self) -> usize {
        self.starts.len()
    }

    /// 第 n 行（0-based）的文本与字节区间（不含行尾换行符）。
    pub fn line(&self, n: usize) -> Option<(&'a str, std::ops::Range<usize>)> {
        let start = *self.starts.get(n)?;
        let end = if n + 1 < self.starts.len() {
            self.starts[n + 1] - 1
        } else {
            // 末行：若源以换行结尾，starts 已弹出该空段，这里需排除换行本身
            self.src.len() - usize::from(self.src.ends_with('\n'))
        };
        if start > end {
            return Some(("", end..end));
        }
        Some((&self.src[start..end], start..end))
    }

    pub fn offset_of_line(&self, n: usize) -> Option<usize> {
        self.starts.get(n).copied()
    }

    /// 包含给定字节偏移的行号。
    pub fn line_at(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    /// 区间 [start, end) 覆盖的行号范围 (first, last_inclusive)。
    pub fn line_range(&self, start: usize, end: usize) -> (usize, usize) {
        (self.line_at(start), self.line_at(end.saturating_sub(1).max(start)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_is_format_only() {
        let a = TextLayer::detect("x\r\ny\r\n");
        assert_eq!(a.body(), "x\ny\n");
        assert_eq!(a.detected_style(), NewlineStyle::CrLf);
        assert!(a.ends_with_newline());
    }

    #[test]
    fn cr_only_and_bom() {
        let a = TextLayer::detect("\u{feff}x\ry\r");
        assert!(a.has_bom());
        assert_eq!(a.body(), "x\ny\n");
        assert_eq!(a.detected_style(), NewlineStyle::Cr);
    }

    #[test]
    fn mixed_line_endings_are_not_uniform() {
        let a = TextLayer::detect("x\r\ny\nz\r");
        assert!(!a.line_endings_uniform());
        // Preserve 在混杂时降级为 LF；
        // 输入以换行结尾（"x\r\ny\nz\r"），因此末尾换行必须保留。
        let out = a.render("x\ny\nz", NewlineStyle::Preserve);
        assert_eq!(std::str::from_utf8(&out).unwrap(), "x\ny\nz\n");
    }

    #[test]
    fn render_preserves_bom_and_trailing() {
        let a = TextLayer::detect("\u{feff}hello\r\n");
        let out = a.render("hello\nworld", NewlineStyle::Preserve);
        assert_eq!(std::str::from_utf8(&out).unwrap(), "\u{feff}hello\r\nworld\r\n");
    }

    #[test]
    fn render_no_trailing_when_input_had_none() {
        let a = TextLayer::detect("hello\nworld");
        let out = a.render("hello\nworld", NewlineStyle::Lf);
        assert_eq!(std::str::from_utf8(&out).unwrap(), "hello\nworld");
    }

    #[test]
    fn invalid_utf8_rejected() {
        assert_eq!(TextLayer::from_bytes(&[0xff, 0xfe, 0x00]).err(), Some(CoreError::InvalidUtf8));
    }

    #[test]
    fn lines_indexing() {
        let src = "aaa\nbbbb\ncc";
        let l = Lines::new(src);
        assert_eq!(l.count(), 3);
        assert_eq!(l.line(1).unwrap().0, "bbbb");
        assert_eq!(l.line(1).unwrap().1, 4..8);
        assert_eq!(l.line_at(5), 1);
        assert_eq!(l.line(3), None);
    }

    #[test]
    fn unicode_not_normalized() {
        // NFD 分解序列必须原样保留，不得合成
        let nfd = "e\u{301}"; // é as e + combining acute
        let a = TextLayer::detect(nfd);
        assert_eq!(a.body(), "e\u{301}");
        let out = a.render(nfd, NewlineStyle::Lf);
        assert_eq!(out, nfd.as_bytes());
    }
}
