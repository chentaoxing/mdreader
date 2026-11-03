//! Rule Engine —— 规则独立、可开关、可调参、可单测（规格第二十七、二十八章）。
//!
//! 规则契约（硬性）：
//! 1. `apply` 只提交编辑列表 [`RuleEdit`]，不得直接返回改写后的字符串；
//! 2. 替换文本**不得包含字母、数字或 CJK 字符**，只允许空白与结构标记。
//!    由 [`Rule::validate`] 静态自检 + Content Guard 动态兜底双重把关；
//! 3. 保护区（代码块 / 行内代码 / HTML / 链接目标）内的区间一律跳过；
//! 4. 管道逐条规则跑 Guard：通过的并入累积文本，失败的整条丢弃并记账。
//!
//! ## v1 取舍（实测驱动，重要）
//!
//! 规格第十九章希望 `A：x、y、z` 转成有序列表，第二十章希望 `名称：xxx` 转表格。
//! 实测证明这两类转换必然**删除标点 `：`、`、`** 或**新增表头文字**，
//! 与第 2.1 节「默认禁止改变标点」冲突。因此 v1：
//! - 列表：只做「并列项分行」（纯插入换行，零字符增删）；
//! - 表格：只规范化分隔行（`|---|` → `| --- |`），不做字段转表格。
//!
//! 这就是「无法安全保证内容不变，宁可暂不实现」的落实。

use std::ops::Range;

use crate::edits::{ChangeScope, Edit, EditKind, RuleEdit};
use crate::error::{CoreError, CoreResult};
use crate::fingerprint::is_cjk_char;
use crate::scan::{LineKind, Scan};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SafetyLevel {
    /// 只增删空白
    Conservative,
    /// 改写结构标记
    Standard,
    /// 需要增删内容 token —— v1 全部禁用
    Aggressive,
}

#[derive(Debug, Clone)]
pub struct Params {
    /// 连续空行超过该值则压缩
    pub max_blank_lines: usize,
    /// 段落超过该显示宽度记为超长（CJK 记 2，其余记 1）
    pub long_paragraph_width: usize,
    /// 标题下内容行少于该值记为「内容过少」
    pub min_lines_under_heading: usize,
    /// 允许并列项分行（R11）
    pub enable_parallel_split: bool,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            long_paragraph_width: 160,
            max_blank_lines: 1,
            min_lines_under_heading: 1,
            enable_parallel_split: true,
        }
    }
}

pub struct Ctx<'a> {
    pub src: &'a str,
    pub scan: &'a Scan,
    pub params: &'a Params,
}

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn priority(&self) -> i32;
    fn safety(&self) -> SafetyLevel;
    fn scope(&self) -> ChangeScope;
    fn enabled_by_default(&self) -> bool {
        true
    }

    /// 提交编辑；`None` 表示本规则无需改动。
    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>>;

    /// 静态自检：替换文本只能是空白或结构标记，禁止任何正文字符。
    fn validate(&self, re: &RuleEdit) -> CoreResult<()> {
        for e in &re.edits {
            if e.replacement.chars().any(|c| c.is_alphanumeric() || is_cjk_char(c)) {
                return Err(CoreError::GuardRejected {
                    rule: re.rule_id.clone(),
                    detail: format!("替换文本含正文字符嫌疑：{:?}", truncate(&e.replacement)),
                });
            }
            if e.range.is_empty() && e.replacement.is_empty() {
                return Err(CoreError::GuardRejected {
                    rule: re.rule_id.clone(),
                    detail: "空编辑".into(),
                });
            }
        }
        Ok(())
    }
}

pub(crate) fn truncate(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i >= 20 {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

/// 收尾：包装为 RuleEdit 并自检。edits 为空则返回 None。
fn package(rule: &dyn Rule, edits: Vec<Edit>) -> CoreResult<Option<RuleEdit>> {
    if edits.is_empty() {
        return Ok(None);
    }
    let re = RuleEdit {
        rule_id: rule.id().to_string(),
        rule_name: rule.name().to_string(),
        scope: rule.scope(),
        edits,
    };
    rule.validate(&re)?;
    Ok(Some(re))
}

/// 注册表：执行顺序 = (priority 降序, id 升序)，五平台完全一致。
#[derive(Default)]
pub struct Registry {
    rules: Vec<Box<dyn Rule>>,
}

impl Registry {
    pub fn new() -> Registry {
        let mut r = Registry { rules: Vec::new() };
        for rule in builtin_rules() {
            r.register(rule);
        }
        r
    }

    pub fn register(&mut self, rule: Box<dyn Rule>) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority().cmp(&a.priority()).then_with(|| a.id().cmp(b.id())));
    }

    pub fn all(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    pub fn find(&self, id: &str) -> Option<&dyn Rule> {
        self.rules.iter().find(|r| r.id() == id).map(|r| r.as_ref())
    }

    pub fn ids(&self) -> Vec<&'static str> {
        self.rules.iter().map(|r| r.id()).collect()
    }
}

/// 运行配置：规则开关、参数、收敛上限。
#[derive(Debug, Clone)]
pub struct Profile {
    pub params: Params,
    pub disabled: Vec<String>,
    /// 只运行这些规则（为空表示全部）
    pub only: Vec<String>,
    /// 规则集成最多重复几轮
    pub max_passes: usize,
}

impl Default for Profile {
    fn default() -> Self {
        Self { params: Params::default(), disabled: Vec::new(), only: Vec::new(), max_passes: 3 }
    }
}

impl Profile {
    pub fn is_active(&self, rule: &dyn Rule) -> bool {
        if !self.only.is_empty() && !self.only.iter().any(|id| id == rule.id()) {
            return false;
        }
        if self.disabled.iter().any(|id| id == rule.id()) {
            return false;
        }
        rule.enabled_by_default() && rule.safety() != SafetyLevel::Aggressive
    }
}

pub fn builtin_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(BlankLines),
        Box::new(TrailingWhitespace),
        Box::new(HeadingSpacing),
        Box::new(AtxCloseMarkerStrip),
        Box::new(SetextToAtx),
        Box::new(QuoteMarkerSpace),
        Box::new(ListMarkerUnify),
        Box::new(TableDelimiterNormalize),
        Box::new(ThematicBreakUnify),
        Box::new(FinalNewlines),
        Box::new(ParallelSplit),
    ]
}

// ══════════════════ R01 空行压缩 ══════════════════

pub struct BlankLines;

impl Rule for BlankLines {
    fn id(&self) -> &'static str {
        "R01.blank-lines"
    }
    fn name(&self) -> &'static str {
        "空行压缩"
    }
    fn description(&self) -> &'static str {
        "连续多个空行压缩为一个，消除大片空白"
    }
    fn priority(&self) -> i32 {
        100
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Conservative
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Whitespace
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let max = ctx.params.max_blank_lines.max(1);
        let mut edits = Vec::new();
        let mut run: Vec<usize> = Vec::new();

        for n in 0..ctx.scan.count() {
            match ctx.scan.kind_at(n) {
                Some(LineKind::Blank) => run.push(n),
                // 代码块内部完全跳过，不压缩
                Some(LineKind::FencedCode) | Some(LineKind::IndentedCode) => run.clear(),
                _ => {
                    if run.len() > max {
                        for &k in &run[max..] {
                            if let Some(pos) = blank_terminator(ctx, k) {
                                edits.push(delete_ws(ctx, pos..pos + 1, "压缩多余空行"));
                            }
                        }
                    }
                    run.clear();
                }
            }
        }
        if run.len() > max {
            for &k in &run[max..] {
                if let Some(pos) = blank_terminator(ctx, k) {
                    edits.push(delete_ws(ctx, pos..pos + 1, "压缩多余空行"));
                }
            }
        }
        package(self, edits)
    }
}

/// 空行自身的终止换行偏移；文件末无终止换行时返回 None。
fn blank_terminator(ctx: &Ctx<'_>, n: usize) -> Option<usize> {
    let line = ctx.scan.line(n)?;
    let pos = line.range.start;
    (ctx.src.as_bytes().get(pos) == Some(&b'\n')).then_some(pos)
}

/// 仅当区间确为纯空白时才生成删除编辑，否则丢弃（安全方向）。
fn delete_ws(ctx: &Ctx<'_>, range: Range<usize>, reason: &str) -> Edit {
    let slice = ctx.src.get(range.clone()).unwrap_or("");
    debug_assert!(slice.chars().all(|c| c == ' ' || c == '\t' || c == '\n'), "{slice:?}");
    Edit {
        range,
        replacement: String::new(),
        kind: EditKind::WhitespaceRemove,
        scope: ChangeScope::Whitespace,
        reason: reason.to_string(),
    }
}

// ══════════════════ R02 行尾空格 ══════════════════

pub struct TrailingWhitespace;

impl Rule for TrailingWhitespace {
    fn id(&self) -> &'static str {
        "R02.trailing-whitespace"
    }
    fn name(&self) -> &'static str {
        "行尾空格"
    }
    fn description(&self) -> &'static str {
        "清理行尾多余空格，保留硬换行所需的两个空格"
    }
    fn priority(&self) -> i32 {
        99
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Conservative
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Whitespace
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        for n in 0..ctx.scan.count() {
            if ctx.scan.is_protected(n) {
                continue;
            }
            let line = &ctx.scan.lines[n];
            if line.trailing_ws == 0 {
                continue;
            }
            let tail = &line.text[line.text.len() - line.trailing_ws..];
            // CommonMark 硬换行：**有内容的行**行尾两个或更多空格、且下一行继续同一段落。
            // v0.1.4 之前只认恰好两个空格（tail == "  "），3+ 空格的硬换行被整段删掉，
            // 与本规则自述「保留硬换行所需的两个空格」直接矛盾；
            // 下一行白名单也缺 Html（`甲  \n<b>x</b> 乙` 的硬换行曾丢失）。
            // 纯空白行（只有空格没有内容）不构成硬换行，照常清理。
            let hard_break = line.text.len() > line.trailing_ws
                && tail.ends_with("  ")
                && matches!(
                    ctx.scan.kind_at(n + 1),
                    Some(LineKind::Paragraph | LineKind::UlItem | LineKind::OlItem | LineKind::Quote | LineKind::Html)
                );
            if hard_break {
                continue;
            }
            let start = line.range.end - line.trailing_ws;
            edits.push(delete_ws(ctx, start..line.range.end, "清理行尾空格"));
        }
        package(self, edits)
    }
}

// ══════════════════ R03 标题前后空行 ══════════════════

pub struct HeadingSpacing;

impl Rule for HeadingSpacing {
    fn id(&self) -> &'static str {
        "R03.heading-spacing"
    }
    fn name(&self) -> &'static str {
        "标题间距"
    }
    fn description(&self) -> &'static str {
        "标题块前后各留一个空行，使章节边界清晰"
    }
    fn priority(&self) -> i32 {
        98
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Conservative
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Heading
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        for (first, last) in heading_blocks(ctx.scan) {
            if first > 0 && ctx.scan.kind_at(first - 1) != Some(LineKind::Blank) {
                let at = ctx.scan.lines[first - 1].range.end;
                if ctx.src.as_bytes().get(at) == Some(&b'\n') {
                    edits.push(insert_ws(at, ChangeScope::Heading, "标题前补空行"));
                }
            }
            if last + 1 < ctx.scan.count() && ctx.scan.kind_at(last + 1) != Some(LineKind::Blank) {
                let at = ctx.scan.lines[last].range.end;
                if ctx.src.as_bytes().get(at) == Some(&b'\n') {
                    edits.push(insert_ws(at, ChangeScope::Heading, "标题后补空行"));
                }
            }
        }
        dedup_by_pos(&mut edits);
        package(self, edits)
    }
}

fn insert_ws(at: usize, scope: ChangeScope, reason: &str) -> Edit {
    Edit {
        range: at..at,
        replacement: "\n".to_string(),
        kind: EditKind::WhitespaceInsert,
        scope,
        reason: reason.to_string(),
    }
}

/// 标题块 (首行, 尾行)，Setext 与上一段合并为一块。规则与评分共用，保证判定一致。
pub fn heading_blocks(scan: &Scan) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for n in 0..scan.count() {
        match scan.kind_at(n) {
            Some(LineKind::AtxHeading) => out.push((n, n)),
            Some(LineKind::SetextUnderline) => {
                if n > 0 && scan.kind_at(n - 1) == Some(LineKind::Paragraph) {
                    // setext 标题的正文可以跨多行（`标题一\n标题二\n===`）。
                    // 旧实现只把下划线**紧挨着的那一行**当作标题块，于是 R03 会在
                    // 标题中间插入空行，把一块标题劈成「段落 + 标题」——
                    // 大纲、层级、评分全都跟着错。这里回溯整段连续的 Paragraph 行。
                    let mut first = n - 1;
                    while first > 0 && scan.kind_at(first - 1) == Some(LineKind::Paragraph) {
                        first -= 1;
                    }
                    if out.last().map(|(_, e)| *e >= first).unwrap_or(false) {
                        out.last_mut().unwrap().1 = n;
                    } else {
                        out.push((first, n));
                    }
                }
            }
            _ => {}
        }
    }
    out
}

// ══════════════════ R04 ATX 尾部闭合标记 ══════════════════

pub struct AtxCloseMarkerStrip;

impl Rule for AtxCloseMarkerStrip {
    fn id(&self) -> &'static str {
        "R04.atx-close-marker"
    }
    fn name(&self) -> &'static str {
        "标题闭合标记"
    }
    fn description(&self) -> &'static str {
        "移除 `## 标题 ##` 这类尾部 `#`"
    }
    fn priority(&self) -> i32 {
        97
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Standard
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Heading
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        for n in 0..ctx.scan.count() {
            if ctx.scan.kind_at(n) != Some(LineKind::AtxHeading) || ctx.scan.is_protected(n) {
                continue;
            }
            let line = &ctx.scan.lines[n];
            if let Some(rel) = closing_hashes_offset(&line.text) {
                let mut e = Edit::replace(line.range.start + rel..line.range.end, "", ChangeScope::Heading, "移除标题尾部闭合标记");
                e.kind = EditKind::Marker;
                edits.push(e);
            }
        }
        package(self, edits)
    }
}

/// `## 标题 ##  ` → 返回尾部闭合序列（连同其前导空格）的**字节**偏移。
fn closing_hashes_offset(text: &str) -> Option<usize> {
    let body = text.trim_end_matches([' ', '\t']);
    if !body.ends_with('#') {
        return None;
    }
    let b = body.as_bytes();
    let mut i = body.len();
    while i > 0 && b[i - 1] == b'#' {
        i -= 1;
    }
    let before = &body[..i];
    if !(before.ends_with(' ') || before.ends_with('\t')) {
        return None;
    }
    let text_part = before.trim_end();
    // 标题本体（去掉前导 # 与空格）必须非空，否则整行只是标记
    if text_part.trim_start_matches('#').trim().is_empty() {
        return None;
    }
    Some(text_part.len())
}

// ══════════════════ R05 Setext → ATX ══════════════════

pub struct SetextToAtx;

impl Rule for SetextToAtx {
    fn id(&self) -> &'static str {
        "R05.setext-to-atx"
    }
    fn name(&self) -> &'static str {
        "Setext 转 ATX"
    }
    fn description(&self) -> &'static str {
        "把下划线式标题统一为 # 式，便于大纲与层级分析"
    }
    fn priority(&self) -> i32 {
        95
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Standard
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Heading
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        for n in 1..ctx.scan.count() {
            if ctx.scan.kind_at(n) != Some(LineKind::SetextUnderline) {
                continue;
            }
            let text_line = &ctx.scan.lines[n - 1];
            if text_line.kind != LineKind::Paragraph || text_line.text.trim().is_empty() {
                continue;
            }
            // 陷阱：setext 标题可以跨多行（`标题一\n标题二\n===`），它的 ATX 等价物
            // 需要把软换行拼成一行，而 ATX 只能单行。旧实现只给最后一行加 `#`，
            // 结果前面几行被降级成普通段落（大纲里整块标题丢了一半，token 不变 ⇒ Guard 看不见）。
            // 无法无损表达 ⇒ 整条跳过（保守方向）。
            if n >= 2 && matches!(ctx.scan.kind_at(n - 2), Some(LineKind::Paragraph)) {
                continue;
            }
            if ctx.scan.is_protected(n - 1) || ctx.scan.is_protected(n) {
                continue;
            }
            let under = &ctx.scan.lines[n];
            let level = if under.text.trim().starts_with('=') { "#" } else { "##" };

            let mut e1 = Edit::insert(text_line.range.start, format!("{level} "), ChangeScope::Heading, "Setext 转 ATX 标记");
            e1.kind = EditKind::Marker;
            edits.push(e1);

            // 删除「标题行行尾到下划线行尾」整段（含换行）
            let start = text_line.range.end;
            let end = under.range.end;
            if ctx.src.get(start..end).map(|s| s.starts_with('\n')).unwrap_or(false) {
                let mut e2 = Edit::replace(start..end, "", ChangeScope::Heading, "移除 Setext 下划线");
                e2.kind = EditKind::WhitespaceRemove;
                edits.push(e2);
            }
        }
        package(self, edits)
    }
}

// ══════════════════ R06 引用标记空格 ══════════════════

pub struct QuoteMarkerSpace;

impl Rule for QuoteMarkerSpace {
    fn id(&self) -> &'static str {
        "R06.quote-space"
    }
    fn name(&self) -> &'static str {
        "引用标记空格"
    }
    fn description(&self) -> &'static str {
        "统一 `>` 之后至少一个空格"
    }
    fn priority(&self) -> i32 {
        93
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Conservative
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Blockquote
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        for n in 0..ctx.scan.count() {
            if ctx.scan.kind_at(n) != Some(LineKind::Quote) || ctx.scan.is_protected(n) {
                continue;
            }
            let line = &ctx.scan.lines[n];
            let prefix_len = line.text.chars().take_while(|c| *c == '>').count();
            let prefix: String = line.text.chars().take(prefix_len).collect();
            let after = &line.text[prefix.len()..];
            if !prefix.is_empty() && !after.starts_with(' ') && !after.is_empty() {
                let at = line.range.start + prefix.len();
                edits.push(Edit {
                    range: at..at,
                    replacement: " ".to_string(),
                    kind: EditKind::WhitespaceInsert,
                    scope: ChangeScope::Blockquote,
                    reason: "引用标记后补空格".to_string(),
                });
            }
        }
        package(self, edits)
    }
}

// ══════════════════ R07 列表标记统一 ══════════════════

pub struct ListMarkerUnify;

impl Rule for ListMarkerUnify {
    fn id(&self) -> &'static str {
        "R07.list-marker"
    }
    fn name(&self) -> &'static str {
        "列表标记统一"
    }
    fn description(&self) -> &'static str {
        "同一列表内混用的项目符号统一为首次出现的一种"
    }
    fn priority(&self) -> i32 {
        91
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Standard
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::List
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        let mut group: Vec<(usize, char)> = Vec::new();

        for n in 0..ctx.scan.count() {
            match ctx.scan.kind_at(n) {
                Some(LineKind::UlItem) => {
                    let c = ctx.scan.lines[n].text.trim_start().chars().next().unwrap_or('-');
                    group.push((n, c));
                }
                // 单空行属松散列表，不断开；两连空行断开
                Some(LineKind::Blank) if ctx.scan.kind_at(n + 1) != Some(LineKind::Blank) => {}
                _ => {
                    flush_list_group(ctx, &mut group, &mut edits);
                }
            }
        }
        flush_list_group(ctx, &mut group, &mut edits);
        package(self, edits)
    }
}

fn flush_list_group(ctx: &Ctx<'_>, group: &mut Vec<(usize, char)>, edits: &mut Vec<Edit>) {
    if group.len() >= 2 {
        let first = group[0].1;
        if !group.iter().all(|(_, c)| *c == first) {
            for (n, c) in group.drain(..) {
                if c == first {
                    continue;
                }
                let line = &ctx.scan.lines[n];
                if ctx.scan.is_protected(n) {
                    continue;
                }
                let at = line.range.start + line.indent;
                if ctx.src.as_bytes().get(at).copied() != Some(c as u8) {
                    continue;
                }
                // 标记后必须是空格，否则不是列表项
                if ctx.src.as_bytes().get(at + 1) != Some(&b' ') {
                    continue;
                }
                let mut e = Edit::replace(at..at + 1, first.to_string(), ChangeScope::List, "统一项目符号");
                e.kind = EditKind::Marker;
                edits.push(e);
            }
        }
    }
    group.clear();
}

// ══════════════════ R08 表格分隔行 ══════════════════

pub struct TableDelimiterNormalize;

impl Rule for TableDelimiterNormalize {
    fn id(&self) -> &'static str {
        "R08.table-delimiter"
    }
    fn name(&self) -> &'static str {
        "表格分隔行"
    }
    fn description(&self) -> &'static str {
        "把分隔行规范为 `| --- |`，保留对齐标记，单元格文字不动"
    }
    fn priority(&self) -> i32 {
        89
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Standard
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Table
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        for n in 0..ctx.scan.count() {
            if ctx.scan.kind_at(n) != Some(LineKind::TableDelimiter) {
                continue;
            }
            let line = &ctx.scan.lines[n];
            let rebuilt = normalize_delimiter_row(&line.text);
            // 只替换首个非空白字符起的区间，保留列表内表格的缩进
            // （整行替换会把缩进一并吃掉，Guard 会拒掉整条编辑）。
            let indent_len = line.text.len() - line.text.trim_start_matches([' ', '\t']).len();
            if !rebuilt.is_empty() && rebuilt != &line.text[indent_len..] {
                let mut e = Edit::replace(
                    line.range.start + indent_len..line.range.end,
                    &rebuilt,
                    ChangeScope::Table,
                    "规范表格分隔行",
                );
                e.kind = EditKind::Marker;
                edits.push(e);
            }
        }
        package(self, edits)
    }
}

fn normalize_delimiter_row(text: &str) -> String {
    let s = text.trim();
    if s.is_empty() || !s.contains('-') {
        return String::new();
    }
    let cells: Vec<&str> = s.trim_start_matches('|').trim_end_matches('|').split('|').map(|c| c.trim()).collect();
    let mut parts: Vec<String> = Vec::with_capacity(cells.len());
    for c in &cells {
        if c.is_empty() || !c.chars().all(|ch| ch == '-' || ch == ':') {
            return String::new(); // 非常规内容，放弃
        }
        let left = c.starts_with(':');
        let right = c.ends_with(':');
        parts.push(match (left, right) {
            (true, true) => ":---:".to_string(),
            (true, false) => ":---".to_string(),
            (false, true) => "---:".to_string(),
            (false, false) => "---".to_string(),
        });
    }
    format!("| {} |", parts.join(" | "))
}

// ══════════════════ R09 分隔线统一 ══════════════════

pub struct ThematicBreakUnify;

impl Rule for ThematicBreakUnify {
    fn id(&self) -> &'static str {
        "R09.thematic-break"
    }
    fn name(&self) -> &'static str {
        "分隔线统一"
    }
    fn description(&self) -> &'static str {
        "把 `***` / `___` 统一为 `---`"
    }
    fn priority(&self) -> i32 {
        87
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Standard
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Rule
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let mut edits = Vec::new();
        for n in 0..ctx.scan.count() {
            if ctx.scan.kind_at(n) != Some(LineKind::ThematicBreak) {
                continue;
            }
            let line = &ctx.scan.lines[n];
            if line.text.trim() != "---" {
                // 陷阱：上一行是段落时，把 `***` / `___` 改成 `---` 会让它变成
                // **setext 下划线**——段落被"提升"成标题、分隔线本身消失。
                // 实测 `文字\n***\n正文` 会变成 `## 文字\n\n正文`，且 Guard 报「正文 0」
                // （星号算结构符，不计内容 token）。无法安全统一 ⇒ 跳过。
                if n > 0 && matches!(ctx.scan.kind_at(n - 1), Some(LineKind::Paragraph)) {
                    continue;
                }
                // v0.1.4 回归：整行替换会把缩进一并剥掉——`- 甲\n    ***\n乙` 曾被改成
                // 无缩进的 `---`，分隔线从列表项内部逃逸到列表外，且 Rule 不产内容
                // token，Guard 拦不住。只替换首个非空白字符起的区间。
                let indent_len = line.text.len() - line.text.trim_start_matches([' ', '\t']).len();
                let mut e = Edit::replace(
                    line.range.start + indent_len..line.range.end,
                    "---",
                    ChangeScope::Rule,
                    "统一分隔线写法",
                );
                e.kind = EditKind::Marker;
                edits.push(e);
            }
        }
        package(self, edits)
    }
}

// ══════════════════ R10 结尾多余空行 ══════════════════

pub struct FinalNewlines;

impl Rule for FinalNewlines {
    fn id(&self) -> &'static str {
        "R10.final-newlines"
    }
    fn name(&self) -> &'static str {
        "结尾空行"
    }
    fn description(&self) -> &'static str {
        "移除文件末尾多余空行，只保留一个结尾换行"
    }
    fn priority(&self) -> i32 {
        85
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Conservative
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::Whitespace
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        let src = ctx.src;
        if src.trim().is_empty() {
            return Ok(None); // 全空白文档不动，避免把内容清空
        }
        let body_end = src.trim_end_matches('\n').len();
        let trailing = src.len() - body_end;
        if trailing > 1 && body_end + 1 < src.len() {
            return package(self, vec![delete_ws(ctx, body_end + 1..src.len(), "移除结尾多余空行")]);
        }
        Ok(None)
    }
}

// ══════════════════ R11 并列项分行 ══════════════════

/// 把「X：a、b、c」这类长并列句在分隔符后换行。
///
/// 只插入换行，**零字符增删**，因此不产生序号也不删标点：
/// 转成带 `1.` 的列表会新增数字 token，v1 明确不做（见模块注释）。
pub struct ParallelSplit;

impl Rule for ParallelSplit {
    fn id(&self) -> &'static str {
        "R11.parallel-split"
    }
    fn name(&self) -> &'static str {
        "并列项分行"
    }
    fn description(&self) -> &'static str {
        "把长并列句按顿号/分号分行，降低单行阅读压力"
    }
    fn priority(&self) -> i32 {
        40
    }
    fn safety(&self) -> SafetyLevel {
        SafetyLevel::Conservative
    }
    fn scope(&self) -> ChangeScope {
        ChangeScope::List
    }

    fn apply(&self, ctx: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
        if !ctx.params.enable_parallel_split {
            return Ok(None);
        }
        let mut edits = Vec::new();
        for n in 0..ctx.scan.count() {
            if ctx.scan.kind_at(n) != Some(LineKind::Paragraph) || ctx.scan.is_protected(n) {
                continue;
            }
            let line = &ctx.scan.lines[n];
            if line.indent > 0 {
                continue;
            }
            // 含内联语法的行整体跳过，避免破坏语法
            if line.text.contains(['`', '[', ']', '<', '>', '|', '*', '_', '\\']) {
                continue;
            }
            // 含引号的行同样跳过：换行插进字符串字面量里会直接破坏
            // TOML / YAML 元数据与配置示例（实测 `+++` 区的 `title = "说明：甲、乙、丙"` 被拆行；
            // v0.1.4 补：单引号字面量 `title = '…'` 在未闭合 front matter 里同样被拆过）。
            if line.text.contains(['"', '\'']) {
                continue;
            }
            let Some((colon_rel, seps)) = parallel_structure(&line.text) else { continue };
            if seps.len() < 2 {
                continue; // 少于三项并列，不值得分行
            }
            let base = line.range.start;
            let mut positions: Vec<usize> = vec![colon_rel];
            positions.extend(seps.iter().copied());
            for rel in positions {
                let at = base + rel;
                if ctx.src.as_bytes().get(at) == Some(&b'\n') {
                    continue;
                }
                // 换行后紧跟的行首记号会改变块语义，跳过该切点
                if starts_like_block_marker(&line.text[rel..]) {
                    continue;
                }
                edits.push(Edit {
                    range: at..at,
                    replacement: "\n".to_string(),
                    kind: EditKind::WhitespaceInsert,
                    scope: ChangeScope::List,
                    reason: "并列项分行".to_string(),
                });
            }
            dedup_by_pos(&mut edits);
        }
        package(self, edits)
    }
}

/// 「冒号结束偏移」与各「分隔符结束偏移」（字节，行内相对）。
fn parallel_structure(text: &str) -> Option<(usize, Vec<usize>)> {
    let mut colon_rel = None;
    for (i, ch) in text.char_indices() {
        match ch {
            '：' => {
                colon_rel = Some(i + ch.len_utf8());
                break;
            }
            ':' => {
                // 排除前一字符是字母或数字的半角冒号：`http:` 是协议，
                // `10:30:00` 是时间戳/比例/版本号——数字冒号被当切点会把它们撕开
                // （v0.1.4 回归：`会议纪要 10:30:00、参会人：甲、乙、丙。`）。
                if text[..i]
                    .chars()
                    .next_back()
                    .map(|c| c.is_ascii_alphanumeric())
                    .unwrap_or(false)
                {
                    continue;
                }
                colon_rel = Some(i + ch.len_utf8());
                break;
            }
            _ => {}
        }
    }
    let colon_rel = colon_rel?;
    let head = text[..colon_rel].trim_end_matches(['：', ':']);
    let head_chars = head.chars().count();
    if head_chars < 2 || head_chars > 40 {
        return None;
    }
    let mut seps = Vec::new();
    for (i, ch) in text[colon_rel..].char_indices() {
        if ch == '、' || ch == '；' {
            seps.push(colon_rel + i + ch.len_utf8());
        }
    }
    Some((colon_rel, seps))
}

/// 插入换行后，这段文本会成为新行的行首。若它看起来像块级记号
/// （列表项、引用、表格行、围栏、标题），分行就改变了块语义 —— 放弃该切点。
fn starts_like_block_marker(rest: &str) -> bool {
    let t = rest.trim_start_matches([' ', '\t']);
    if t.is_empty() {
        return true; // 空行 = 段落切分，超出「仅并列分行」范围
    }
    if t.starts_with(['#', '>', '|', '`', '~', '*', '_', '+', ':']) {
        return true;
    }
    if t.starts_with("- ") || t.starts_with("-\t") {
        return true;
    }
    // 数字 + `.` / `、` / `）`：可能变成有序列表项
    if t.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
        if matches!(t[digits..].chars().next(), Some('.' | ')' | '）' | '、')) {
            return true;
        }
    }
    false
}

fn dedup_by_pos(edits: &mut Vec<Edit>) {
    edits.sort_by(|a, b| a.range.start.cmp(&b.range.start).then_with(|| a.range.end.cmp(&b.range.end)));
    // 已排序，只需与前一个键比较：避免大文档下 O(k²) 的线性查重
    let mut prev: Option<(usize, usize)> = None;
    edits.retain(|e| {
        let key = (e.range.start, e.range.end);
        if prev == Some(key) {
            false
        } else {
            prev = Some(key);
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edits::apply_edits;
    use crate::fingerprint::protected_ranges;
    use crate::guard::verify;

    /// 跑一条规则；Guard 失败则回滚原文并返回 Err 标记。
    fn run(rule: &dyn Rule, src: &str) -> (String, Result<usize, String>) {
        let scan = Scan::new(src);
        let params = Params::default();
        let ctx = Ctx { src, scan: &scan, params: &params };
        let Ok(Some(re)) = rule.apply(&ctx) else { return (src.to_string(), Ok(0)) };
        let out = apply_edits(src, &re, &protected_ranges(src)).unwrap_or_else(|e| panic!("{} 非法编辑：{e}", rule.id()));
        if verify(src, &out).is_pass() {
            (out, Ok(re.edits.len()))
        } else {
            let v = verify(src, &out).violations.iter().map(|x| x.detail.clone()).collect::<Vec<_>>().join("；");
            (src.to_string(), Err(v))
        }
    }

    fn ok(rule: &dyn Rule, src: &str) -> String {
        let (out, res) = run(rule, src);
        assert!(res.is_ok(), "{} 未通过 Guard: {:?}", rule.id(), res.err());
        out
    }

    #[test]
    fn r01_collapses_blank_runs() {
        assert_eq!(ok(&BlankLines, "段落一\n\n\n\n段落二\n"), "段落一\n\n段落二\n");
        // 幂等
        let once = ok(&BlankLines, "a\n\n\n\n\nb\n");
        assert_eq!(ok(&BlankLines, &once), once);
    }

    #[test]
    fn r01_never_touches_code_block_interior() {
        let src = "```\na\n\n\n\nb\n```\n";
        assert_eq!(ok(&BlankLines, src), src, "代码块内空行绝不能压缩");
    }

    // ── 中文排版回归：全角空格 / 不间断空格是内容，不是可删空白 ──

    #[test]
    fn r02_keeps_cjk_typographic_spaces_at_line_end() {
        assert_eq!(ok(&TrailingWhitespace, "正文　　\n"), "正文　　\n", "行尾全角空格必须保留");
        assert_eq!(ok(&TrailingWhitespace, "正文\u{00a0}\n"), "正文\u{00a0}\n", "行尾 NBSP 必须保留");
        assert_eq!(ok(&TrailingWhitespace, "正文 　\n"), "正文 　\n", "半角空格后跟全角空格：整段是内容");
    }

    #[test]
    fn r01_keeps_lines_made_of_fullwidth_spaces() {
        let src = "第一段\n　\n第二段\n";
        assert_eq!(ok(&BlankLines, src), src, "只含全角空格的行是内容行，不得当空行删除");
    }

    #[test]
    fn r09_never_creates_a_setext_underline() {
        // 上一行是段落：*** -> --- 会让段落变成标题、分隔线消失
        assert_eq!(ok(&ThematicBreakUnify, "文字\n***\n正文\n"), "文字\n***\n正文\n");
        // 上一行是空行：正常统一
        assert_eq!(ok(&ThematicBreakUnify, "文字\n\n***\n正文\n"), "文字\n\n---\n正文\n");
    }

    #[test]
    fn r09_keeps_indent_inside_lists() {
        // v0.1.4 回归：整行替换曾把缩进一并剥掉，列表内的分隔线逃逸到列表外，
        // 且 Rule 不产内容 token，Guard 拦不住。
        let src = "- 甲\n    ***\n乙\n";
        assert_eq!(ok(&ThematicBreakUnify, src), "- 甲\n    ---\n乙\n", "分隔线统一必须保留缩进");
    }

    #[test]
    fn r05_skips_multi_line_setext_heading() {
        let src = "标题一\n标题二\n===\n\n正文\n";
        assert_eq!(ok(&SetextToAtx, src), src, "多行 setext 转 ATX 会降级前半部分，必须原样保留");
        assert_eq!(ok(&SetextToAtx, "标题\n===\n\n正文\n"), "# 标题\n\n正文\n", "单行仍应正常转换");
    }

    #[test]
    fn r11_never_splits_inside_a_quoted_string() {
        let src = "title = \"说明：甲、乙、丙\"\n";
        assert_eq!(ok(&ParallelSplit, src), src, "换行插进字符串字面量会破坏 TOML/YAML");
        // v0.1.4 回归：单引号字面量在未闭合 front matter 里同样被拆过
        let single = "title = '说明：甲、乙、丙'\n";
        assert_eq!(ok(&ParallelSplit, single), single, "单引号字面量同样不得拆行");
    }

    #[test]
    fn r11_never_splits_a_timestamp() {
        // v0.1.4 回归：半角冒号前一字符是数字（时间戳/比例/版本号）曾被当切点
        // 把 10:30:00 撕成 10\n30:00。修复后时间戳必须完整；
        // 参会人：后的全角冒号仍是合法切点，正常分行。
        let src = "会议纪要 10:30:00、参会人：甲、乙、丙。\n";
        assert_eq!(
            ok(&ParallelSplit, src),
            "会议纪要 10:30:00、参会人：\n甲、\n乙、\n丙。\n",
            "时间戳里的冒号不是并列切点，正常冒号仍分行"
        );
    }

    #[test]
    fn heading_blocks_span_every_line_of_a_setext_heading() {
        let scan = Scan::new("甲\n乙\n===\n\n正文\n");
        assert_eq!(heading_blocks(&scan), vec![(0, 2)], "多行 setext 是一整块标题");
        // R03 不得在标题块中间补空行（旧实现把最后一行当整块，于是标题被劈开）
        assert_eq!(ok(&HeadingSpacing, "甲\n乙\n===\n\n正文\n"), "甲\n乙\n===\n\n正文\n");
        assert_eq!(
            ok(&HeadingSpacing, "前段\n\n甲\n乙\n===\n正文\n"),
            "前段\n\n甲\n乙\n===\n\n正文\n",
            "标题块前后各留一个空行，且只补在整块的外侧"
        );
    }

    #[test]
    fn r02_removes_trailing_but_keeps_hard_break() {
        // v0.1.4：CommonMark 硬换行是行尾 2+ 空格——3 空格与 2 空格同样必须保留
        // （旧实现只认恰好两个，3+ 被整段删掉，与本规则自述矛盾）。
        assert_eq!(ok(&TrailingWhitespace, "第一段   \n第二段\n"), "第一段   \n第二段\n", "三空格硬换行同样保留");
        let hard = "甲  \n乙\n";
        assert_eq!(ok(&TrailingWhitespace, hard), hard, "两空格硬换行必须保留");
        // 下一行是内联 HTML（继续同一段落）：硬换行保留（旧白名单缺 Html 曾丢失它）
        let html_next = "甲  \n<b>乙</b> 丙\n";
        assert_eq!(ok(&TrailingWhitespace, html_next), html_next, "HTML 行前的硬换行保留");
        // 下一行开新块（标题）：行尾空格无硬换行语义，删掉
        assert_eq!(ok(&TrailingWhitespace, "甲  \n# 标\n"), "甲\n# 标\n", "标题前的行尾空格该删");
        let src = "```\ncode   \nx\n```\n";
        assert_eq!(ok(&TrailingWhitespace, src), src);
    }

    #[test]
    fn r03_adds_blank_lines_and_is_idempotent() {
        let out = ok(&HeadingSpacing, "引言\n# 标题\n正文\n");
        assert_eq!(out, "引言\n\n# 标题\n\n正文\n");
        assert_eq!(ok(&HeadingSpacing, &out), out, "规则必须幂等");
    }

    #[test]
    fn r04_strips_closing_hashes() {
        assert_eq!(ok(&AtxCloseMarkerStrip, "## 标题 ##\n"), "## 标题\n");
        // 中文标题也不能把偏移算错（字节 vs 字符）
        assert_eq!(ok(&AtxCloseMarkerStrip, "### 中文标题 ###\n"), "### 中文标题\n");
        assert_eq!(ok(&AtxCloseMarkerStrip, "#\n"), "#\n");
        assert_eq!(ok(&AtxCloseMarkerStrip, "# 纯文字\n"), "# 纯文字\n");
    }

    #[test]
    fn r05_setext_to_atx_preserves_content() {
        assert_eq!(ok(&SetextToAtx, "标题文字\n===\n\n正文\n"), "# 标题文字\n\n正文\n");
        assert_eq!(ok(&SetextToAtx, "次级\n---\n\n正文\n"), "## 次级\n\n正文\n");
    }

    #[test]
    fn r06_quote_space() {
        assert_eq!(ok(&QuoteMarkerSpace, ">引用文字\n"), "> 引用文字\n");
        assert_eq!(ok(&QuoteMarkerSpace, ">>x\n"), ">> x\n");
        assert_eq!(ok(&QuoteMarkerSpace, "> 已有空格\n"), "> 已有空格\n");
    }

    #[test]
    fn r07_unifies_markers_to_first_style() {
        assert_eq!(ok(&ListMarkerUnify, "- 甲\n* 乙\n+ 丙\n"), "- 甲\n- 乙\n- 丙\n");
        assert_eq!(ok(&ListMarkerUnify, "* 一\n- 二\n"), "* 一\n* 二\n");
        let src = "- 甲\n- 乙\n";
        assert_eq!(ok(&ListMarkerUnify, src), src);
    }

    #[test]
    fn r08_normalizes_delimiter_row() {
        assert_eq!(normalize_delimiter_row("|---|:-:|"), "| --- | :---: |");
        assert_eq!(normalize_delimiter_row("| :--- | ---: |"), "| :--- | ---: |");
        assert_eq!(normalize_delimiter_row("| a | b |"), "", "非分隔行不动");
        let out = ok(&TableDelimiterNormalize, "| a | b |\n|---|:-:|\n| 1 | 2 |\n");
        assert_eq!(out, "| a | b |\n| --- | :---: |\n| 1 | 2 |\n");
    }

    #[test]
    fn r09_thematic_break() {
        assert_eq!(ok(&ThematicBreakUnify, "***\n"), "---\n");
        assert_eq!(ok(&ThematicBreakUnify, "___\n"), "---\n");
        assert_eq!(ok(&ThematicBreakUnify, "---\n"), "---\n");
    }

    #[test]
    fn r10_extra_final_newlines() {
        assert_eq!(ok(&FinalNewlines, "正文\n\n\n\n"), "正文\n");
        let src = "\n\n\n";
        assert_eq!(run(&FinalNewlines, src).0, src, "全空白文档不做改动");
        assert_eq!(ok(&FinalNewlines, "正文\n"), "正文\n");
    }

    #[test]
    fn r11_parallel_split_is_zero_char_change() {
        let src = "这个方案有三个问题：成本高、部署复杂、维护困难，都需要评估。\n";
        let (out, res) = run(&ParallelSplit, src);
        assert!(res.is_ok(), "{:?}", res.err());
        assert_eq!(out.replace('\n', ""), src.replace('\n', ""), "只允许插入换行");
        assert!(out.lines().count() > 2, "应已分行");
    }

    #[test]
    fn r11_skips_inline_syntax_and_short_lists() {
        let src = "看 [文档](http://x)、还有 `代码`、以及 更多。\n";
        assert_eq!(run(&ParallelSplit, src).0, src);
        let short = "只有两项：甲、乙。\n";
        assert_eq!(run(&ParallelSplit, short).0, short, "少于 3 个分隔符不处理");
    }

    #[test]
    fn all_rules_pass_guard_on_shared_samples() {
        let samples = [
            "# A\n\n正文一\n\n\n\n正文二\n",
            "```\ncode   \n\n\nx\n```\n\n段落\n",
            ">引用\n>- 项目\n",
            "- 甲\n* 乙\n\n\n",
            "标题\n===\n\n内容\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n\n",
            "---\n\n***\n\n文本   \n\n",
            "说明：甲、乙、丙、丁。\n",
            "#  标题  ##\n\n段落\n",
            "\u{feff}中文标题\n\n===\n\n段落   \n\n\n",
            "[链接](http://a/b_c)、顿号、第三个。\n",
            "价格 $1,000、$2,000、$3,000 三个档：低、中、高。\n",
        ];
        for src in samples {
            let scan = Scan::new(src);
            let params = Params::default();
            let ctx = Ctx { src, scan: &scan, params: &params };
            for rule in builtin_rules() {
                let Ok(Some(re)) = rule.apply(&ctx) else { continue };
                match apply_edits(src, &re, &protected_ranges(src)) {
                    Ok(o) => {
                        let r = verify(src, &o);
                        assert!(r.is_pass(), "{} 在 {src:?} 上违反内容不变：{:?}\n输出 {o:?}", rule.id(), r.violations);
                    }
                    Err(e) => panic!("{} 在 {src:?} 上提交非法编辑：{e}", rule.id()),
                }
            }
        }
    }

    #[test]
    fn registry_order_is_deterministic() {
        let a = Registry::new();
        let b = Registry::new();
        assert_eq!(a.ids(), b.ids(), "注册顺序必须跨运行一致");
        assert_eq!(a.ids().first().copied(), Some("R01.blank-lines"));
        assert_eq!(a.ids().len(), builtin_rules().len());
    }

    #[test]
    fn validate_rejects_content_bearing_replacement() {
        let bad = RuleEdit {
            rule_id: "x".into(),
            rule_name: "x".into(),
            scope: ChangeScope::Other,
            edits: vec![Edit::replace(0..1, "正文", ChangeScope::Other, "bad")],
        };
        assert!(ListMarkerUnify.validate(&bad).is_err());

        let bad_num = RuleEdit {
            rule_id: "x".into(),
            rule_name: "x".into(),
            scope: ChangeScope::Other,
            edits: vec![Edit::replace(0..1, "1. ", ChangeScope::Other, "编号")],
        };
        assert!(ListMarkerUnify.validate(&bad_num).is_err(), "插入序号数字必须被自检拦下");
    }

    #[test]
    fn profile_controls_rules() {
        let reg = Registry::new();
        let mut p = Profile::default();
        p.disabled = vec!["R01.blank-lines".into()];
        assert!(!p.is_active(reg.find("R01.blank-lines").unwrap()));
        assert!(p.is_active(reg.find("R02.trailing-whitespace").unwrap()));

        p.disabled.clear();
        p.only = vec!["R09.thematic-break".into()];
        assert!(p.is_active(reg.find("R09.thematic-break").unwrap()));
        assert!(!p.is_active(reg.find("R01.blank-lines").unwrap()));
    }

    #[test]
    fn aggressive_rules_are_never_active() {
        struct Aggro;
        impl Rule for Aggro {
            fn id(&self) -> &'static str {
                "R99.aggro"
            }
            fn name(&self) -> &'static str {
                "x"
            }
            fn description(&self) -> &'static str {
                "x"
            }
            fn priority(&self) -> i32 {
                1
            }
            fn safety(&self) -> SafetyLevel {
                SafetyLevel::Aggressive
            }
            fn scope(&self) -> ChangeScope {
                ChangeScope::Other
            }
            fn apply(&self, _: &Ctx<'_>) -> CoreResult<Option<RuleEdit>> {
                Ok(None)
            }
        }
        assert!(!Profile::default().is_active(&Aggro));
    }

    #[test]
    fn heading_blocks_detects_setext_group() {
        let src = "标题文字\n===\n\n正文\n## 二级\n";
        let scan = Scan::new(src);
        let blocks = heading_blocks(&scan);
        assert!(blocks.contains(&(0, 1)), "Setext 两行应并为一块：{blocks:?}");
        assert!(blocks.contains(&(4, 4)));
    }
}
