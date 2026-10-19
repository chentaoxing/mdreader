//! Readability Engine —— 本地阅读性分析与评分（规格第十七、十八、二十五章）。
//!
//! 设计约束：
//! - **纯整数运算**：不用浮点、不用平台 locale、不用 HashMap 迭代顺序。
//!   五平台同输入必得同输出（规格第三十二章）。
//! - 评分只是辅助信息，**不参与任何改写决策**，也绝不允许「为刷分而改内容」。
//! - 所有计数基于 [`crate::scan::Scan`]（行视图）与 [`crate::model::Document`]（块视图），
//!   两者共用同一解析选项，保证分析口径一致。

use crate::fingerprint::tokenize;
use crate::model::{BlockKind, Document};
use crate::rules::heading_blocks;
use crate::scan::{LineKind, Scan};

/// 单项得分：0..=100，整数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Score {
    pub value: u8,
    pub issues: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Report {
    /// 标题结构
    pub heading: Score,
    /// 段落结构
    pub paragraph: Score,
    /// 列表结构
    pub list: Score,
    /// 空白分布
    pub whitespace: Score,
    /// 信息密度
    pub density: Score,
    /// 综合评分（整数加权平均）
    pub overall: u8,

    // ── 明细指标（UI 与 Diff 直接展示，不重新计算）──
    pub headings: u32,
    pub heading_level_skips: u32,
    pub heading_too_deep: u32,
    pub heading_starved: u32,
    pub duplicate_headings: u32,
    pub headings_per_100_lines: u32,

    pub paragraphs: u32,
    pub long_paragraphs: u32,
    pub tiny_paragraphs: u32,
    pub consecutive_single_line: u32,

    pub list_items: u32,
    pub max_list_depth: u32,
    pub parallel_runs_unsplit: u32,

    pub blank_runs_over_limit: u32,
    pub trailing_ws_lines: u32,

    pub content_tokens: u32,
    pub code_lines: u32,
    pub total_lines: u32,
    /// 平均段落显示宽度（整数，CJK 记 2）
    pub avg_paragraph_width: u32,

    /// 人类可读问题清单，按严重度排序
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub line: usize,
    pub dimension: &'static str,
    pub detail: String,
    /// 0 最严重
    pub severity: u8,
}

/// 显示宽度：CJK 与全角记 2，其余记 1。整数、确定性。
pub fn display_width(s: &str) -> usize {
    let mut w = 0usize;
    for ch in s.chars() {
        w += if is_wide(ch) { 2 } else { 1 };
    }
    w
}

fn is_wide(ch: char) -> bool {
    let cp = ch as u32;
    crate::fingerprint::is_cjk_char(ch)
        || matches!(cp,
            0x1100..=0x115F      // Hangul Jamo init
          | 0x2E80..=0x303E      // CJK Radicals .. 标点
          | 0x3041..=0x33FF      // 假名/注音/CJK 兼容
          | 0x3400..=0x4DBF
          | 0x4E00..=0x9FFF
          | 0xA000..=0xA4CF
          | 0xAC00..=0xD7A3
          | 0xF900..=0xFAFF
          | 0xFE10..=0xFE19 | 0xFE30..=0xFE6F
          | 0xFF00..=0xFF60      // 全角形式
          | 0xFFE0..=0xFFE6
          | 0x1F300..=0x1FAFF    // emoji 主要区段
          | 0x20000..=0x3FFFD)
}

/// 分析一份正文（语义层，LF 归一化）。
pub fn analyze(src: &str, scan: &Scan, doc: &Document, blank_limit: usize, width_limit: usize) -> Report {
    let mut r = Report::default();
    let total = scan.count() as u32;
    r.total_lines = total;

    // ── 标题 ──
    let blocks = heading_blocks(scan);
    r.headings = doc.count_of(BlockKind::Heading) as u32;
    let mut last_level: Option<u8> = None;
    for (first, _last) in &blocks {
        if let Some(lv) = atx_level(&scan.lines[*first].text) {
            if let Some(prev) = last_level {
                if lv > prev + 1 {
                    r.heading_level_skips += 1;
                    r.findings.push(Finding {
                        line: *first,
                        dimension: "标题结构",
                        detail: format!("层级跳跃：H{prev} → H{lv}"),
                        severity: 1,
                    });
                }
            }
            last_level = Some(lv);
            if lv >= 5 {
                r.heading_too_deep += 1;
            }
        }
    }

    // 标题下内容过少
    for (idx, (first, last)) in blocks.iter().enumerate() {
        let next_start = blocks.get(idx + 1).map(|(f, _)| *f).unwrap_or(scan.count());
        let body = next_start.saturating_sub(last + 1) as u32;
        if body < 2 && *last + 1 < scan.count() {
            r.heading_starved += 1;
            r.findings.push(Finding {
                line: *first,
                dimension: "标题结构",
                detail: "标题下几乎没有正文".to_string(),
                severity: 2,
            });
        }
    }

    // 重复标题
    let mut labels: Vec<String> = doc.outline().into_iter().map(|(_, l, _)| l).collect();
    labels.sort();
    let mut i = 0;
    while i + 1 < labels.len() {
        if labels[i] == labels[i + 1] && !labels[i].is_empty() {
            r.duplicate_headings += 1;
            i += 2;
        } else {
            i += 1;
        }
    }

    r.headings_per_100_lines = if total > 0 { r.headings * 100 / total } else { 0 };

    // ── 段落 ──
    let paras = paragraph_groups(scan);
    r.paragraphs = paras.len() as u32;
    let mut width_sum = 0usize;
    let mut single_run = 0usize;
    let mut max_single_run = 0usize;
    for (start, end) in &paras {
        let text = paragraph_text(scan, *start, *end);
        let w = display_width(&text);
        width_sum += w;
        if w > width_limit {
            r.long_paragraphs += 1;
            r.findings.push(Finding {
                line: *start,
                dimension: "段落结构",
                detail: format!("段落过长（显示宽度 {w}，建议 ≤ {width_limit}）"),
                severity: 1,
            });
        }
        if w > 0 && w < 12 {
            r.tiny_paragraphs += 1;
        }
        if end == start {
            single_run += 1;
            max_single_run = max_single_run.max(single_run);
        } else {
            single_run = 0;
        }
    }
    r.consecutive_single_line = max_single_run.min(9) as u32;
    r.avg_paragraph_width = if r.paragraphs > 0 { (width_sum / r.paragraphs as usize) as u32 } else { 0 };

    // ── 列表 ──
    r.list_items = (0..scan.count())
        .filter(|n| matches!(scan.kind_at(*n), Some(LineKind::UlItem | LineKind::OlItem)))
        .count() as u32;
    r.max_list_depth = max_list_indent_depth(scan) as u32;
    r.parallel_runs_unsplit = count_unsplit_parallel(scan) as u32;

    // ── 空白 ──
    // inside_code 需要 O(1)，否则空行多的文档会退化成 O(n²)
    let code_mask = code_line_mask(scan);
    let mut run = 0usize;
    for n in 0..scan.count() {
        if scan.kind_at(n) == Some(LineKind::Blank) && !code_mask[n] {
            run += 1;
        } else {
            if run > blank_limit {
                r.blank_runs_over_limit += 1;
            }
            run = 0;
        }
    }
    if run > blank_limit {
        r.blank_runs_over_limit += 1;
    }
    r.trailing_ws_lines =
        (0..scan.count()).filter(|n| !scan.is_protected(*n) && scan.lines[*n].trailing_ws > 0).count() as u32;

    // ── 密度 / 代码占比 ──
    r.content_tokens = tokenize(src).tokens.len() as u32;
    r.code_lines = (0..scan.count())
        .filter(|n| matches!(scan.kind_at(*n), Some(LineKind::FencedCode) | Some(LineKind::IndentedCode)))
        .count() as u32;

    // ── 打分 ──
    r.heading = score_heading(&r);
    r.paragraph = score_paragraph(&r);
    r.list = score_list(&r);
    r.whitespace = score_whitespace(&r);
    r.density = score_density(&r);
    // 权重：标题 25、段落 25、列表 15、空白 15、密度 20（整数，和为 100）
    r.overall = ((r.heading.value as u32 * 25
        + r.paragraph.value as u32 * 25
        + r.list.value as u32 * 15
        + r.whitespace.value as u32 * 15
        + r.density.value as u32 * 20)
        / 100) as u8;

    r.findings.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.line.cmp(&b.line)));
    r.findings.truncate(40);
    r
}

fn score_heading(r: &Report) -> Score {
    let issues = r.heading_level_skips + r.heading_starved * 2 + r.duplicate_headings + r.heading_too_deep * 2;
    let mut v = 100i32 - issues as i32 * 12;
    // 标题密度：每百行超过 30 视为碎片化
    if r.headings_per_100_lines > 30 {
        v -= 10;
    }
    if r.headings == 0 && r.total_lines > 40 {
        v -= 15; // 长文无标题，导航困难
    }
    Score { value: clamp(v), issues }
}

fn score_paragraph(r: &Report) -> Score {
    let issues = r.long_paragraphs + r.tiny_paragraphs / 3 + r.consecutive_single_line;
    let v = 100i32 - issues as i32 * 9;
    Score { value: clamp(v), issues }
}

fn score_list(r: &Report) -> Score {
    let mut issues = 0i32;
    if r.parallel_runs_unsplit > 0 {
        issues += r.parallel_runs_unsplit.min(4) as i32;
    }
    if r.max_list_depth > 4 {
        issues += 2;
    }
    let mut v = 100 - issues * 10;
    if r.list_items == 0 && r.paragraphs > 6 {
        v -= 8; // 全文无列表，结构信息少
    }
    Score { value: clamp(v), issues: issues as u32 }
}

fn score_whitespace(r: &Report) -> Score {
    let issues = r.blank_runs_over_limit as i32 + (r.trailing_ws_lines / 4) as i32;
    let v = 100 - issues * 8;
    Score { value: clamp(v), issues: issues as u32 }
}

fn score_density(r: &Report) -> Score {
    let mut v = 100i32;
    if r.avg_paragraph_width > 300 {
        v -= 20;
    } else if r.avg_paragraph_width > 200 {
        v -= 10;
    }
    if r.total_lines > 0 {
        let code_pct = r.code_lines as i32 * 100 / r.total_lines as i32;
        if code_pct > 60 {
            v -= 10; // 代码为主，正文节奏受限（仅提示，不改动）
        }
    }
    if r.content_tokens < 30 && r.total_lines > 20 {
        v -= 10;
    }
    Score { value: clamp(v), issues: ((100 - v).max(0) / 10) as u32 }
}

fn clamp(v: i32) -> u8 {
    v.clamp(0, 100) as u8
}

fn atx_level(text: &str) -> Option<u8> {
    let h = text.chars().take_while(|c| *c == '#').count();
    (1..=6).contains(&h).then_some(h as u8)
}

/// 段落组：连续的 Paragraph 行（含硬换行折行）视为一个段落。
pub fn paragraph_groups(scan: &Scan) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for n in 0..scan.count() {
        let is_para = scan.kind_at(n) == Some(LineKind::Paragraph);
        match (start, is_para) {
            (None, true) => start = Some(n),
            (Some(s), true) => {
                // 行首缩进或空行分隔的相邻段落行仍属同一段落（软换行）
                let _ = s;
            }
            (Some(s), false) => {
                out.push((s, n - 1));
                start = None;
            }
            (None, false) => {}
        }
    }
    if let Some(s) = start {
        out.push((s, scan.count() - 1));
    }
    out
}

fn paragraph_text(scan: &Scan, start: usize, end: usize) -> String {
    let mut s = String::new();
    for n in start..=end {
        if let Some(l) = scan.line(n) {
            s.push_str(l.text.trim());
            s.push(' ');
        }
    }
    s
}

/// 每行是否「处于代码上下文」：忽略空行后，最近的上/下非空行都是代码行。
///
/// 单次扫描建表，供空白统计以 O(1) 查询；逐行双向搜索会让空行多的文档退化为 O(n²)。
fn code_line_mask(scan: &Scan) -> Vec<bool> {
    let n = scan.count();
    let mut mask = vec![false; n];
    let is_code = |k: Option<LineKind>| matches!(k, Some(LineKind::FencedCode) | Some(LineKind::IndentedCode));
    let mut prev: Option<LineKind> = None;
    let mut above: Vec<Option<LineKind>> = vec![None; n];
    for i in 0..n {
        let k = scan.kind_at(i);
        if k != Some(LineKind::Blank) {
            prev = k;
        }
        above[i] = prev;
    }
    let mut below: Option<LineKind> = None;
    for i in (0..n).rev() {
        let k = scan.kind_at(i);
        if k == Some(LineKind::Blank) && is_code(above[i]) && is_code(below) {
            mask[i] = true;
        }
        if k != Some(LineKind::Blank) {
            below = k;
        }
    }
    mask
}

/// 列表最大嵌套深度：按行首缩进每 2 空格一层，上限 8。
fn max_list_indent_depth(scan: &Scan) -> usize {
    let mut max_d = 0usize;
    for n in 0..scan.count() {
        if matches!(scan.kind_at(n), Some(LineKind::UlItem | LineKind::OlItem)) {
            let d = (scan.lines[n].indent / 2).min(8);
            max_d = max_d.max(d + 1);
        }
    }
    max_d
}

/// 统计「A：x、y、z」这类未分行的长并列句数量（仅用于评分，不做改写）。
fn count_unsplit_parallel(scan: &Scan) -> usize {
    let mut c = 0;
    for n in 0..scan.count() {
        if scan.kind_at(n) != Some(LineKind::Paragraph) || scan.is_protected(n) {
            continue;
        }
        let t = &scan.lines[n].text;
        if t.contains(['：', ':']) {
            // 与 R11 的可切点判定保持一致：首段 + 各并列段之间均可分行
            let seps = t.chars().filter(|ch| *ch == '、' || *ch == '；').count();
            if seps >= 2 && display_width(t) > 60 {
                c += 1;
            }
        }
    }
    c
}

/// 文本形式：`标题结构 92 / 段落结构 87 / …`，UI 直接展示。
pub fn render_summary(r: &Report) -> String {
    let mut out = String::new();
    for (name, s) in [
        ("标题结构", r.heading),
        ("段落结构", r.paragraph),
        ("列表结构", r.list),
        ("空白分布", r.whitespace),
        ("信息密度", r.density),
    ] {
        out.push_str(&format!("{name} {}  ", s.value));
    }
    out.push_str(&format!("综合评分 {}", r.overall));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextLayer;

    fn report(src: &str) -> Report {
        let layer = TextLayer::detect(src);
        let body = layer.body().to_string();
        let scan = Scan::new(&body);
        let doc = Document::parse(&body).unwrap();
        analyze(&body, &scan, &doc, 1, 160)
    }

    #[test]
    fn display_width_counts_cjk_as_two() {
        assert_eq!(display_width("中文"), 4);
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("中a"), 3);
        assert_eq!(display_width("😀"), 2);
        assert_eq!(display_width("ｅ"), 2, "全角拉丁记 2");
    }

    #[test]
    fn clean_document_scores_high() {
        let src = "# 引言\n\n这是一段长度适中的中文正文，介绍了工具的目标和适用范围。\n\n## 安装\n\n按下面步骤操作即可：\n\n- 下载压缩包\n- 解压到本地目录\n- 双击启动应用\n\n## 使用\n\n再写一段正常长度的正文内容，保证信息密度合理可读。\n";
        let r = report(src);
        assert!(r.overall >= 85, "结构良好的文档应高分，实际 {r:?}");
        assert_eq!(r.heading_level_skips, 0);
        assert_eq!(r.headings, 3);
    }

    #[test]
    fn detects_level_skip() {
        let r = report("# 一\n\n正文内容\n\n#### 四\n\n正文内容\n");
        assert_eq!(r.heading_level_skips, 1, "H1 → H4 应记为一次跳跃");
        assert!(r.heading.value < 100);
    }

    #[test]
    fn detects_long_paragraph() {
        let long = "啊".repeat(200);
        let r = report(&format!("# 标题\n\n{long}\n"));
        assert_eq!(r.long_paragraphs, 1);
    }

    #[test]
    fn detects_blank_runs_and_trailing_ws() {
        let r = report("段落一\n\n\n\n段落二  \n");
        assert_eq!(r.blank_runs_over_limit, 1);
        assert_eq!(r.trailing_ws_lines, 1);
    }

    #[test]
    fn blank_lines_inside_code_do_not_count() {
        let r = report("```\na\n\n\n\nb\n```\n");
        assert_eq!(r.blank_runs_over_limit, 0, "代码块内空行不应计入空白问题");
    }

    #[test]
    fn detects_unsplit_parallel_run() {
        let r = report("这个方案有三个明显问题：成本非常高、部署相当复杂、后续维护困难。\n");
        assert_eq!(r.parallel_runs_unsplit, 1);
        assert!(r.list.value < 100);
    }

    #[test]
    fn duplicate_headings_counted() {
        let r = report("# 概述\n\n正文一\n\n# 概述\n\n正文二\n");
        assert_eq!(r.duplicate_headings, 1);
    }

    #[test]
    fn starved_heading_counted() {
        let r = report("# 一\n\n## 二\n\n## 三\n\n正文一段比较长的内容用来平衡密度数值。\n");
        assert!(r.heading_starved >= 1, "{:?}", r.findings);
    }

    #[test]
    fn scores_are_clamped_and_integer() {
        let long = "啊".repeat(400) + "\n\n\n\n\n" + &"哦".repeat(400);
        let r = report(&long);
        for s in [r.heading, r.paragraph, r.list, r.whitespace, r.density] {
            assert!(s.value <= 100);
        }
        assert!(r.overall <= 100);
    }

    #[test]
    fn analysis_is_deterministic_and_platform_independent() {
        // 同一内容，仅行尾不同 → 归一化后指标必须完全一致（规格第三十四）
        let lf = "# 标题\n\n正文内容\n\n- 项目\n";
        let crlf_src = lf.replace('\n', "\r\n");
        let a = TextLayer::detect(lf);
        let b = TextLayer::detect(&crlf_src);
        assert_eq!(a.body(), b.body());
        let sa = Scan::new(a.body());
        let da = Document::parse(a.body()).unwrap();
        let sb = Scan::new(b.body());
        let db = Document::parse(b.body()).unwrap();
        let ra = analyze(a.body(), &sa, &da, 1, 160);
        let rb = analyze(b.body(), &sb, &db, 1, 160);
        assert_eq!((ra.overall, ra.headings, ra.paragraphs, ra.list_items), (rb.overall, rb.headings, rb.paragraphs, rb.list_items));
    }

    #[test]
    fn summary_text_renders() {
        let r = report("# A\n\n正文\n");
        let s = render_summary(&r);
        assert!(s.contains("标题结构") && s.contains("综合评分"), "{s}");
    }

    #[test]
    fn empty_document_does_not_panic() {
        let r = report("");
        assert_eq!(r.total_lines, 1);
        assert_eq!(r.headings, 0);
    }
}
