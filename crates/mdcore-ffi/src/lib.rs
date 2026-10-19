//! C ABI 封装层（规格第十三章「Shared Core」的唯一对外边界）。
//!
//! ## 设计原则
//! - **本层不含任何 Markdown 语义逻辑**：所有判定都来自 `mdcore`。
//!   因此五平台（Windows / macOS / Linux / Android / iOS）调用的是同一份算法实现，
//!   「先做 Windows 再考虑移植」在本架构下不可能发生。
//! - 边界上只允许两种数据形态：**字节数组**（`*const u8 + len`）与 **UTF-8 JSON 字符串**。
//!   结构体不入 ABI，避免各平台对齐/命名差异造成 ABI 漂移。
//! - 内存所有权明确：所有返回的字符串由本层分配，必须由 `mdfile_free_string` 释放；
//!   输入缓冲区由调用方持有，本层只在调用期间读取。
//! - 不会让 panic 跨过 C ABI：所有返回 JSON 的入口（optimize/analyze/preview/rebuild/rules_json）
//!   都经 [`guard_call`] 的 `catch_unwind` 转成状态码；三个纯 C 辅助入口
//!   （`mdfile_engine_version` / `mdfile_last_error` / `mdfile_free_string`）本身不含可 panic 的逻辑。
//!
//! ## 状态码
//! ```text
//!  0  OK
//!  1  输入不是合法 UTF-8（绝不猜编码）
//!  2  输入超过上限
//!  3  Content Guard 拒绝（正文发生变化）—— 调用方必须放弃本次结果
//!  4  未知预设 / 未知规则 ID
//!  5  内部 panic（已捕获，返回体不含半成品）
//! -1  非法参数（空指针 / 长度越界）
//! ```

use std::ffi::CStr;
use std::os::raw::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};

use serde::Serialize;

use mdcore::diff::Diff;
use mdcore::io::manifest_for;
use mdcore::presets;
use mdcore::rules::{Registry, SafetyLevel};
use mdcore::text::NewlineStyle;

/// 单次调用的输入上限。移动端与桌面端同一数值，避免「某平台能处理、某平台报错」的分裂。
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

const STATUS_OK: i32 = 0;
const STATUS_INVALID_UTF8: i32 = 1;
const STATUS_TOO_LARGE: i32 = 2;
const STATUS_GUARD_REJECTED: i32 = 3;
const STATUS_UNKNOWN: i32 = 4;
const STATUS_PANIC: i32 = 5;
const STATUS_BAD_ARGS: i32 = -1;

static VERSION: std::sync::OnceLock<&'static [u8]> = std::sync::OnceLock::new();

fn version_cstr() -> &'static [u8] {
    VERSION.get_or_init(|| {
        let mut s = mdcore::ENGINE_VERSION.as_bytes().to_vec();
        s.push(0);
        s.leak()
    })
}

/// 引擎版本。UI 层应把它显示在「关于」页，并在跨设备迁移时比对。
#[no_mangle]
pub extern "C" fn mdfile_engine_version() -> *const c_char {
    version_cstr().as_ptr() as *const c_char
}

// 最近一次调用的错误消息（线程本地，避免并发覆盖）。
thread_local! {
    static LAST_ERROR: std::cell::RefCell<String> = std::cell::RefCell::new(String::new());
}

fn set_last_error(msg: impl AsRef<str>) {
    LAST_ERROR.with(|c| *c.borrow_mut() = msg.as_ref().to_string());
}

fn clear_last_error() {
    LAST_ERROR.with(|c| *c.borrow_mut() = String::new());
}

/// 最近一次调用的错误消息。
///
/// **返回值总是本层分配的堆内存**（无错误时是长度为 0 的串），用完必须 `mdfile_free_string` 释放。
/// 旧实现在无错误时返回指向静态 `b"\0"` 的指针，而 `mdfile.h` 写的是「若非空需释放」——
/// 该指针永远非空，于是按文档写的 `if (p) mdfile_free_string(p);` 会 free 一个静态地址，
/// 实测直接段错误（glibc：`free(): invalid pointer`）。C 客户端是唯一裁判，这里改成永远可安全释放。
#[no_mangle]
pub extern "C" fn mdfile_last_error() -> *mut c_char {
    let msg = LAST_ERROR.with(|c| c.borrow().clone());
    match to_heap_cstr(&msg) {
        Some(p) => p,
        None => EMPTY.as_ptr() as *mut c_char,
    }
}

static EMPTY: &[u8] = b"\0";

/// 释放本层返回的字符串。传 null 是合法的空操作。
///
/// # Safety
/// `ptr` 必须来自本层的返回值，且只释放一次。
#[no_mangle]
pub unsafe extern "C" fn mdfile_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(std::ffi::CString::from_raw(ptr));
    }
}

fn to_heap_cstr(s: &str) -> Option<*mut c_char> {
    // 内部 NUL 不应出现在我们的 JSON 里；若出现，截断而非 panic
    let sanitized: String = s.chars().filter(|c| *c != '\0').collect();
    std::ffi::CString::new(sanitized).ok().map(|c| c.into_raw())
}

unsafe fn read_bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
    if len > MAX_INPUT_BYTES {
        return None;
    }
    if ptr.is_null() {
        // 空文件在 C 侧完全合法：`malloc(0)` 可能返回 NULL，Swift/Kotlin 也常传空切片的首地址。
        // 旧实现把 (NULL, 0) 判成非法参数(-1)，而 (有效指针, 0) 判成空文档(0)，
        // 同一份 0 字节 .md 在两个平台上得到两种结论。
        return (len == 0).then_some(&[]);
    }
    Some(unsafe { std::slice::from_raw_parts(ptr, len) })
}

unsafe fn read_cstr<'a>(ptr: *const c_char, default: &'a str) -> Result<&'a str, ()> {
    if ptr.is_null() {
        return Ok(default);
    }
    let c = unsafe { CStr::from_ptr(ptr) };
    c.to_str().map_err(|_| ())
}

fn write_out(out: *mut *mut c_char, out_len: *mut usize, payload: &str) -> i32 {
    if out.is_null() || out_len.is_null() {
        set_last_error("输出指针为空");
        return STATUS_BAD_ARGS;
    }
    let ptr = match to_heap_cstr(payload) {
        Some(p) => p,
        None => {
            set_last_error("结果编码失败");
            return STATUS_PANIC;
        }
    };
    unsafe {
        *out = ptr;
        *out_len = payload.len();
    }
    STATUS_OK
}

// ────────────────────────── 返回体结构 ──────────────────────────

#[derive(Serialize)]
struct OptimizeReply {
    engine_version: &'static str,
    preset: String,
    /// 优化后的正文（语义层 LF，不含 BOM）
    body: String,
    /// 按格式层渲染后的完整字节（以 latin1 逐字节编码进 `bytes_latin1`，避免 JSON 破坏二进制）
    rendered_latin1: String,
    newline_style: &'static str,
    had_bom: bool,
    passes: usize,
    content_changes: usize,
    structural_changes: usize,
    applied: Vec<AppliedWire>,
    rejected: Vec<RejectedWire>,
    categories: Vec<(String, usize)>,
    score_before: ScoreWire,
    score_after: ScoreWire,
    manifest_json: String,
    /// 正文 token 序列校验和（原/新），跨平台一致性断言用它。
    /// 注意：u64 直接写进 JSON 时，17~19 位十进制超过 2^53，WebView/JS 的
    /// `JSON.parse` 会把末几位改掉，UI 比对请用下面两个 `*_str` 字段。
    checksum_original: u64,
    checksum_optimized: u64,
    checksum_original_str: String,
    checksum_optimized_str: String,
}

#[derive(Serialize)]
struct AppliedWire {
    rule_id: String,
    rule_name: String,
    scope: String,
    edits: usize,
    reason: String,
}

#[derive(Serialize)]
struct RejectedWire {
    rule_id: String,
    rule_name: String,
    reason: String,
}

#[derive(Serialize)]
struct ScoreWire {
    overall: u8,
    heading: u8,
    paragraph: u8,
    list: u8,
    whitespace: u8,
    density: u8,
}

fn score_of(r: &mdcore::readability::Report) -> ScoreWire {
    ScoreWire {
        overall: r.overall,
        heading: r.heading.value,
        paragraph: r.paragraph.value,
        list: r.list.value,
        whitespace: r.whitespace.value,
        density: r.density.value,
    }
}

fn newline_label(s: NewlineStyle) -> &'static str {
    match s {
        NewlineStyle::Preserve => "preserve",
        NewlineStyle::Lf => "lf",
        NewlineStyle::CrLf => "crlf",
        NewlineStyle::Cr => "cr",
    }
}

/// latin1 逐字节编码：byte n -> char U+000n。UI 侧解码回字节即可得到原始文件内容。
fn bytes_to_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| *b as char).collect()
}

// ────────────────────────── 主入口 ──────────────────────────

/// 优化一份 Markdown。
///
/// `preset` 可为 NULL（=通用文章）。`newline` 取 `"preserve"|"lf"|"crlf"|"cr"`，NULL 视为 preserve。
///
/// # Safety
/// `input`/`preset`/`newline` 必须是有效指针或 NULL；`out_json` 由本层写入，需 `mdfile_free_string` 释放。
#[no_mangle]
pub unsafe extern "C" fn mdfile_optimize(
    input: *const u8,
    input_len: usize,
    preset: *const c_char,
    newline: *const c_char,
    out_json: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    guard_call(out_json, out_len, || {
        let bytes = unsafe { read_bytes(input, input_len) }.ok_or_else(|| {
            set_last_error(if input_len > MAX_INPUT_BYTES {
                format!("输入超过上限 {MAX_INPUT_BYTES} 字节")
            } else {
                "输入指针为空".into()
            });
            if input_len > MAX_INPUT_BYTES {
                STATUS_TOO_LARGE
            } else {
                STATUS_BAD_ARGS
            }
        })?;
        let preset_name = unsafe { read_cstr(preset, presets::GENERIC) }.map_err(|_| {
            set_last_error("预设名不是合法 UTF-8");
            STATUS_INVALID_UTF8
        })?;
        let nl_name = unsafe { read_cstr(newline, "preserve") }.map_err(|_| {
            set_last_error("newline 参数不是合法 UTF-8");
            STATUS_INVALID_UTF8
        })?;
        let style = match nl_name {
            "lf" => NewlineStyle::Lf,
            "crlf" => NewlineStyle::CrLf,
            "cr" => NewlineStyle::Cr,
            _ => NewlineStyle::Preserve,
        };
        let profile = presets::profile_for(preset_name).ok_or_else(|| {
            set_last_error(format!(
                "未知预设「{preset_name}」，可选：{}",
                presets::PRESETS.join(" / ")
            ));
            STATUS_UNKNOWN
        })?;

        let doc = mdcore::io::OpenDocument::from_bytes("document", bytes, style).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_INVALID_UTF8
        })?;

        let registry = Registry::new();
        let outcome = mdcore::Optimizer::new(&registry, &profile)
            .run(&doc.body)
            .map_err(|e| {
                set_last_error(format!("{e}"));
                match e {
                    mdcore::CoreError::GuardRejected { .. } => STATUS_GUARD_REJECTED,
                    mdcore::CoreError::TooLarge { .. } => STATUS_TOO_LARGE,
                    _ => STATUS_PANIC,
                }
            })?;

        let diff = Diff::from_outcome(&outcome);
        let rendered = doc.render(&outcome.body);
        let manifest = manifest_for("document", "document_阅读优化版.md", preset_name, &outcome);
        let reply = OptimizeReply {
            engine_version: mdcore::ENGINE_VERSION,
            preset: preset_name.to_string(),
            body: outcome.body.clone(),
            rendered_latin1: bytes_to_latin1(&rendered),
            newline_style: newline_label(style),
            had_bom: doc.layer.has_bom(),
            passes: outcome.passes,
            content_changes: diff.content_changes,
            structural_changes: diff.structural_changes,
            applied: outcome
                .applied
                .iter()
                .map(|a| AppliedWire {
                    rule_id: a.rule_id.clone(),
                    rule_name: a.rule_name.clone(),
                    scope: a.scope.label().to_string(),
                    edits: a.edit_count,
                    reason: a.reason.clone(),
                })
                .collect(),
            rejected: outcome
                .rejected
                .iter()
                .map(|r| RejectedWire {
                    rule_id: r.rule_id.clone(),
                    rule_name: r.rule_name.clone(),
                    reason: r.reason.clone(),
                })
                .collect(),
            categories: diff.categories.iter().map(|c| (c.label.to_string(), c.count)).collect(),
            score_before: score_of(&outcome.before),
            score_after: score_of(&outcome.after),
            manifest_json: manifest.to_json(),
            checksum_original: outcome.guard.original.checksum,
            checksum_optimized: outcome.guard.candidate.checksum,
            checksum_original_str: outcome.guard.original.checksum.to_string(),
            checksum_optimized_str: outcome.guard.candidate.checksum.to_string(),
        };
        let json = serde_json::to_string(&reply).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_PANIC
        })?;
        Ok(json)
    })
}

/// 只分析，不改一字。返回评分与明细 JSON。
///
/// # Safety
/// 同 [`mdfile_optimize`]。
#[no_mangle]
pub unsafe extern "C" fn mdfile_analyze(
    input: *const u8,
    input_len: usize,
    preset: *const c_char,
    out_json: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    guard_call(out_json, out_len, || {
        let bytes = unsafe { read_bytes(input, input_len) }.ok_or_else(|| {
            set_last_error(if input_len > MAX_INPUT_BYTES { "输入超过上限" } else { "输入指针为空且长度非 0" });
            STATUS_BAD_ARGS
        })?;
        let preset_name = unsafe { read_cstr(preset, presets::GENERIC) }.map_err(|_| {
            set_last_error("预设名不是合法 UTF-8");
            STATUS_INVALID_UTF8
        })?;
        let profile = presets::profile_for(preset_name).ok_or_else(|| {
            set_last_error(format!("未知预设「{preset_name}」"));
            STATUS_UNKNOWN
        })?;
        // 走格式层：BOM/CRLF 与 mdfile_optimize 看到同一份正文，两个入口的评分才可比
        let doc = mdcore::io::OpenDocument::from_bytes("document", bytes, NewlineStyle::Preserve).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_INVALID_UTF8
        })?;
        let text = doc.body.as_str();
        let registry = Registry::new();
        let r = mdcore::Optimizer::new(&registry, &profile)
            .analyze(text)
            .map_err(|e| {
                set_last_error(format!("{e}"));
                STATUS_PANIC
            })?;
        #[derive(Serialize)]
        struct AnalyzeReply {
            summary: String,
            score: ScoreWire,
            total_lines: u32,
            headings: u32,
            paragraphs: u32,
            list_items: u32,
            code_lines: u32,
            content_tokens: u32,
            heading_level_skips: u32,
            heading_starved: u32,
            long_paragraphs: u32,
            findings: Vec<FindingWire>,
        }
        #[derive(Serialize)]
        struct FindingWire {
            line: usize,
            dimension: String,
            detail: String,
        }
        let reply = AnalyzeReply {
            summary: mdcore::readability::render_summary(&r),
            score: score_of(&r),
            total_lines: r.total_lines,
            headings: r.headings,
            paragraphs: r.paragraphs,
            list_items: r.list_items,
            code_lines: r.code_lines,
            content_tokens: r.content_tokens,
            heading_level_skips: r.heading_level_skips,
            heading_starved: r.heading_starved,
            long_paragraphs: r.long_paragraphs,
            findings: r
                .findings
                .iter()
                .map(|f| FindingWire { line: f.line, dimension: f.dimension.to_string(), detail: f.detail.clone() })
                .collect(),
        };
        serde_json::to_string(&reply).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_PANIC
        })
    })
}

/// 规则列表 + 能力位（设置页与「单规则测试」用）。
#[no_mangle]
pub extern "C" fn mdfile_rules_json(out_json: *mut *mut c_char, out_len: *mut usize) -> i32 {
    guard_call(out_json, out_len, || {
        #[derive(Serialize)]
        struct RuleWire {
            id: String,
            name: String,
            description: String,
            priority: i32,
            safety: &'static str,
            scope: String,
            available_in_v1: bool,
        }
        #[derive(Serialize)]
        struct Reply {
            engine_version: &'static str,
            presets: Vec<(&'static str, &'static str)>,
            rules: Vec<RuleWire>,
        }
        let registry = Registry::new();
        let reply = Reply {
            engine_version: mdcore::ENGINE_VERSION,
            presets: presets::PRESETS.iter().map(|p| (*p, presets::describe(p))).collect(),
            rules: registry
                .all()
                .iter()
                .map(|r| RuleWire {
                    id: r.id().to_string(),
                    name: r.name().to_string(),
                    description: r.description().to_string(),
                    priority: r.priority(),
                    safety: match r.safety() {
                        SafetyLevel::Conservative => "conservative",
                        SafetyLevel::Standard => "standard",
                        SafetyLevel::Aggressive => "aggressive",
                    },
                    scope: r.scope().label().to_string(),
                    available_in_v1: r.safety() != SafetyLevel::Aggressive,
                })
                .collect(),
        };
        serde_json::to_string(&reply).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_PANIC
        })
    })
}

/// 规则干跑（不写盘）。`rule_id` 为 NULL 表示全部规则。
///
/// # Safety
/// 同 [`mdfile_optimize`]。
#[no_mangle]
pub unsafe extern "C" fn mdfile_preview(
    input: *const u8,
    input_len: usize,
    preset: *const c_char,
    rule_id: *const c_char,
    out_json: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    guard_call(out_json, out_len, || {
        let bytes = unsafe { read_bytes(input, input_len) }.ok_or_else(|| {
            set_last_error(if input_len > MAX_INPUT_BYTES { "输入超过上限" } else { "输入指针为空且长度非 0" });
            STATUS_BAD_ARGS
        })?;
        let preset_name = unsafe { read_cstr(preset, presets::GENERIC) }.map_err(|_| {
            set_last_error("预设名不是合法 UTF-8");
            STATUS_INVALID_UTF8
        })?;
        let profile = presets::profile_for(preset_name).ok_or_else(|| {
            set_last_error(format!("未知预设「{preset_name}」"));
            STATUS_UNKNOWN
        })?;
        let doc = mdcore::io::OpenDocument::from_bytes("document", bytes, NewlineStyle::Preserve).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_INVALID_UTF8
        })?;
        let text = doc.body.as_str();
        let registry = Registry::new();
        let optimizer = mdcore::Optimizer::new(&registry, &profile);
        #[derive(Serialize)]
        struct PreviewWire {
            rule_id: String,
            rule_name: String,
            scope: String,
            edits: usize,
            /// 预览文本（None 表示该规则无改动或被拒绝）
            result: Option<String>,
            reject_reason: Option<String>,
        }
        let previews = if rule_id.is_null() {
            optimizer.preview_all(text).map_err(|e| {
                set_last_error(format!("{e}"));
                STATUS_PANIC
            })?
        } else {
            let id = unsafe { read_cstr(rule_id, "") }.map_err(|_| {
                set_last_error("rule_id 不是合法 UTF-8");
                STATUS_INVALID_UTF8
            })?;
            match optimizer.preview_rule(text, id) {
                Ok(Some(p)) => vec![p],
                Ok(None) => Vec::new(),
                Err(e) => {
                    set_last_error(e.to_string());
                    return Err(if matches!(e, mdcore::CoreError::UnknownRule(_)) { STATUS_UNKNOWN } else { STATUS_PANIC });
                }
            }
        };
        let list: Vec<PreviewWire> = previews
            .into_iter()
            .map(|p| PreviewWire {
                rule_id: p.rule_id,
                rule_name: p.rule_name,
                scope: p.scope.label().to_string(),
                edits: p.edit_count,
                result: p.result,
                reject_reason: p.reject_reason,
            })
            .collect();
        serde_json::to_string(&list).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_PANIC
        })
    })
}

/// 逐条接受/拒绝后重建正文（规格第二十八章）。`decisions_json` 形如
/// `[{"decision":"reject","rule_id":"R01.blank-lines"}]`。
///
/// # Safety
/// 同 [`mdfile_optimize`]。
#[no_mangle]
pub unsafe extern "C" fn mdfile_rebuild(
    original: *const u8,
    original_len: usize,
    preset: *const c_char,
    decisions_json: *const c_char,
    out_json: *mut *mut c_char,
    out_len: *mut usize,
) -> i32 {
    guard_call(out_json, out_len, || {
        let bytes = unsafe { read_bytes(original, original_len) }.ok_or(STATUS_BAD_ARGS)?;
        let preset_name = unsafe { read_cstr(preset, presets::GENERIC) }.map_err(|_| {
            set_last_error("预设名不是合法 UTF-8");
            STATUS_INVALID_UTF8
        })?;
        let dj = unsafe { read_cstr(decisions_json, "[]") }.map_err(|_| {
            set_last_error("决策 JSON 不是合法 UTF-8");
            STATUS_INVALID_UTF8
        })?;
        let profile = presets::profile_for(preset_name).ok_or_else(|| {
            set_last_error(format!("未知预设「{preset_name}」"));
            STATUS_UNKNOWN
        })?;
        // 与 mdfile_optimize 走同一套「两层模型」：旧实现直接拿裸字节 from_utf8，
        // 于是同一份 CRLF/BOM 文件在 optimize 与 rebuild 两个入口得到不同结论，
        // 且 rebuild 的返回值丢掉了格式层（写盘就把 CRLF 变成 LF）。
        let doc = mdcore::io::OpenDocument::from_bytes("document", bytes, NewlineStyle::Preserve).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_INVALID_UTF8
        })?;
        let text = doc.body.as_str();
        #[derive(serde::Deserialize)]
        struct DecisionWire {
            decision: String,
            rule_id: String,
        }
        let items: Vec<DecisionWire> = serde_json::from_str(dj).map_err(|e| {
            set_last_error(format!("决策 JSON 解析失败：{e}"));
            STATUS_UNKNOWN
        })?;
        let registry = Registry::new();
        let outcome = mdcore::Optimizer::new(&registry, &profile).run(text).map_err(|e| {
            set_last_error(format!("{e}"));
            match e {
                mdcore::CoreError::GuardRejected { .. } => STATUS_GUARD_REJECTED,
                mdcore::CoreError::TooLarge { .. } => STATUS_TOO_LARGE,
                _ => STATUS_PANIC,
            }
        })?;
        let mut log = mdcore::diff::DecisionLog::default();
        for d in items {
            let kind = match d.decision.as_str() {
                "accept" => mdcore::diff::Decision::Accept,
                "reject" => mdcore::diff::Decision::Reject,
                "undo" => mdcore::diff::Decision::Undo,
                "redo" => mdcore::diff::Decision::Redo,
                "restore" => mdcore::diff::Decision::RestoreOriginal,
                other => {
                    set_last_error(format!("未知决策类型 {other}"));
                    return Err(STATUS_UNKNOWN);
                }
            };
            log.record(kind, &d.rule_id);
        }
        let body = log.rebuild(text, &outcome, &registry, &profile).map_err(|e| {
            set_last_error(format!("{e}"));
            STATUS_GUARD_REJECTED
        })?;
        // content_changes 必须**算出来**：旧实现写死 0，而 mdfile.h 把「正文变化 0」
        // 当成 UI 允许落盘的凭据，等于让一个常量给改动背书。
        let verdict = mdcore::guard::verify(text, &body);
        let content_changes = verdict.violations.len();
        if content_changes > 0 {
            let detail = verdict
                .violations
                .iter()
                .take(3)
                .map(|v| v.detail.clone())
                .collect::<Vec<_>>()
                .join("；");
            set_last_error(format!("重建后正文发生变化：{detail}"));
            return Err(STATUS_GUARD_REJECTED);
        }
        let rendered = doc.render(&body);
        let fp = mdcore::fingerprint::tokenize(&body);
        #[derive(Serialize)]
        struct Reply {
            body: String,
            /// 按格式层渲染后的完整文件字节（latin1 逐字节编码），与 mdfile_optimize 同义
            rendered_latin1: String,
            newline_style: &'static str,
            had_bom: bool,
            content_changes: usize,
            checksum: u64,
            /// 同一个校验和的十进制字符串：u64 写进 JSON 超过 2^53，
            /// WebView / JS 侧 `JSON.parse` 会把末位改掉，UI 必须用这个字段比对
            checksum_str: String,
        }
        let reply = Reply {
            content_changes,
            rendered_latin1: bytes_to_latin1(&rendered),
            newline_style: newline_label(NewlineStyle::Preserve),
            had_bom: doc.layer.has_bom(),
            checksum: fp.checksum,
            checksum_str: fp.checksum.to_string(),
            body,
        };
        serde_json::to_string(&reply).map_err(|e| {
            set_last_error(e.to_string());
            STATUS_PANIC
        })
    })
}

/// 统一入口：清空上一次的错误、捕获 panic、写回 out 指针、把 Err(code) 转为返回值。
fn guard_call(out_json: *mut *mut c_char, out_len: *mut usize, f: impl FnOnce() -> Result<String, i32>) -> i32 {
    // 每次调用先清空 LAST_ERROR：否则「失败 → 成功」之后调用方读到的仍是上一次的错误，
    // UI 会在一次正常优化后面挂一条红色报错（真实 C 客户端可复现）。
    clear_last_error();
    let res = catch_unwind(AssertUnwindSafe(|| match f() {
        Ok(json) => write_out(out_json, out_len, &json),
        Err(code) => {
            unsafe {
                if !out_json.is_null() {
                    *out_json = std::ptr::null_mut();
                }
                if !out_len.is_null() {
                    *out_len = 0;
                }
            }
            code
        }
    }));
    match res {
        Ok(code) => code,
        Err(_) => {
            set_last_error("核心内部异常，已中止本次操作，未返回任何结果");
            unsafe {
                if !out_json.is_null() {
                    *out_json = std::ptr::null_mut();
                }
                if !out_len.is_null() {
                    *out_len = 0;
                }
            }
            STATUS_PANIC
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_optimize(input: &[u8], preset: &str) -> (i32, String) {
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0usize;
        let preset_c = std::ffi::CString::new(preset).unwrap();
        let code = unsafe {
            mdfile_optimize(input.as_ptr(), input.len(), preset_c.as_ptr(), std::ptr::null(), &mut out, &mut len)
        };
        let s = if out.is_null() {
            String::new()
        } else {
            let owned = unsafe { std::ffi::CString::from_raw(out) };
            owned.to_string_lossy().to_string()
        };
        (code, s)
    }

    #[test]
    fn optimize_returns_parseable_json_and_zero_content_changes() {
        let src = "# 标题\n\n\n\n正文一段。\n".as_bytes();
        let (code, json) = call_optimize(src, presets::GENERIC);
        assert_eq!(code, STATUS_OK, "{json}");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["content_changes"].as_u64(), Some(0));
        assert!(v["structural_changes"].as_u64().unwrap() > 0);
        assert_eq!(v["checksum_original"], v["checksum_optimized"], "正文指纹必须相同");
        assert_eq!(v["body"].as_str().unwrap(), "# 标题\n\n正文一段。\n");
    }

    #[test]
    fn rendered_latin1_is_the_exact_file_bytes() {
        // iOS/Android 拿到的字节必须与桌面写盘一致（含 BOM 与 CRLF）
        let raw = "\u{feff}# 标题\r\n\r\n\r\n\r\n正文\r\n".as_bytes().to_vec();
        let (code, json) = call_optimize(&raw, presets::TECHNICAL);
        assert_eq!(code, STATUS_OK);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        let latin = v["rendered_latin1"].as_str().unwrap();
        let back: Vec<u8> = latin.chars().map(|c| c as u8).collect();
        assert_eq!(&back[..3], &[0xEF, 0xBB, 0xBF], "BOM 应原样保留");
        assert!(back.windows(2).any(|w| w == b"\r\n"), "CRLF 应原样保留");
        assert_eq!(v["had_bom"].as_bool(), Some(true));
    }

    #[test]
    fn invalid_utf8_is_reported_not_guessed() {
        let bad = vec![0xffu8, 0xfe, 0x41];
        let (code, _) = call_optimize(&bad, presets::GENERIC);
        assert_eq!(code, STATUS_INVALID_UTF8);
    }

    #[test]
    fn unknown_preset_is_rejected() {
        let (code, json) = call_optimize("# A\n".as_bytes(), "不存在的预设");
        assert_eq!(code, STATUS_UNKNOWN, "{json}");
        assert!(json.is_empty(), "失败时不得返回半成品");
        let ptr = mdfile_last_error();
        let err = unsafe { std::ffi::CString::from_raw(ptr as *mut c_char) }.to_string_lossy().to_string();
        assert!(err.contains("未知预设"), "{err}");
    }

    #[test]
    fn null_input_pointer_is_bad_args() {
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0usize;
        let code = unsafe {
            mdfile_optimize(std::ptr::null(), 5, std::ptr::null(), std::ptr::null(), &mut out, &mut len)
        };
        assert_eq!(code, STATUS_BAD_ARGS);
        assert!(out.is_null());
    }

    #[test]
    fn empty_document_is_accepted_not_rejected() {
        // 0 字节 .md 在 C 侧可能是 (NULL, 0)（malloc(0)），必须与 (有效指针, 0) 同结论
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0usize;
        let a = unsafe { mdfile_optimize(std::ptr::null(), 0, std::ptr::null(), std::ptr::null(), &mut out, &mut len) };
        if !out.is_null() {
            unsafe { drop(std::ffi::CString::from_raw(out)) };
        }
        out = std::ptr::null_mut();
        let b = unsafe { mdfile_optimize("\0".as_ptr(), 0, std::ptr::null(), std::ptr::null(), &mut out, &mut len) };
        if !out.is_null() {
            unsafe { drop(std::ffi::CString::from_raw(out)) };
        }
        assert_eq!((a, b), (STATUS_OK, STATUS_OK), "同一份空文件不得有两种结论");
    }

    fn take_last_error() -> String {
        let ptr = mdfile_last_error();
        let owned = unsafe { std::ffi::CString::from_raw(ptr as *mut c_char) };
        owned.to_string_lossy().to_string()
    }

    #[test]
    fn last_error_is_freeable_even_when_empty() {
        // 回归：无错误时返回的是静态 `b"\0"`，而 mdfile.h 写着「若非空需释放」，
        // C 客户端照做即 free() 静态地址 -> glibc abort（真实 C 客户端段错误已复现）。
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0usize;
        let code = unsafe { mdfile_optimize("# A\n".as_ptr(), 4, std::ptr::null(), std::ptr::null(), &mut out, &mut len) };
        assert_eq!(code, STATUS_OK);
        drop(unsafe { std::ffi::CString::from_raw(out) });
        let msg = take_last_error(); // 这里必须能安全释放，哪怕内容是空串
        assert!(msg.is_empty(), "成功调用后不应有错误消息：{msg}");
    }

    #[test]
    fn success_clears_previous_error() {
        // 回归：失败 -> 成功 之后 LAST_ERROR 仍是上次的消息，UI 会在正常结果后挂红字
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0usize;
        let bad_preset = std::ffi::CString::new("不存在的预设").unwrap();
        let bad = unsafe {
            mdfile_optimize("# A\n".as_ptr(), 4, bad_preset.as_ptr(), std::ptr::null(), &mut out, &mut len)
        };
        assert_eq!(bad, STATUS_UNKNOWN);
        assert!(take_last_error().contains("未知预设"));
        let ok = unsafe { mdfile_optimize("# A\n".as_ptr(), 4, std::ptr::null(), std::ptr::null(), &mut out, &mut len) };
        assert_eq!(ok, STATUS_OK);
        drop(unsafe { std::ffi::CString::from_raw(out) });
        assert_eq!(take_last_error(), "", "成功后必须清空上一次的错误");
    }

    #[test]
    fn rebuild_returns_the_format_layer_too() {
        // 回归：rebuild 旧实现不吃格式层，也不返回渲染字节 -> CRLF/BOM 文件按 rebuild
        // 结果写盘就把 CRLF 变成 LF；content_changes 还是写死的 0。
        let raw = "\u{feff}# 标题   \r\n\r\n\r\n\r\n正文\r\n".as_bytes().to_vec();
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0usize;
        let decisions = std::ffi::CString::new("[]").unwrap();
        let code = unsafe {
            mdfile_rebuild(raw.as_ptr(), raw.len(), std::ptr::null(), decisions.as_ptr(), &mut out, &mut len)
        };
        assert_eq!(code, STATUS_OK, "err={}", take_last_error());
        let v: serde_json::Value =
            serde_json::from_str(&unsafe { std::ffi::CString::from_raw(out) }.to_string_lossy()).unwrap();
        assert_eq!(v["content_changes"].as_u64(), Some(0), "正文变化必须是算出来的 0");
        let latin = v["rendered_latin1"].as_str().unwrap();
        let back: Vec<u8> = latin.chars().map(|c| c as u8).collect();
        assert_eq!(&back[..3], &[0xEF, 0xBB, 0xBF], "BOM 保留");
        assert!(back.windows(2).any(|w| w == b"\r\n"), "CRLF 保留：{:?}", String::from_utf8_lossy(&back));
        assert!(!v["checksum_str"].as_str().unwrap().is_empty(), "UI 侧要有不丢精度的校验和串");
    }

    #[test]
    fn oversized_input_rejected() {
        let len = MAX_INPUT_BYTES + 1;
        let code = unsafe {
            let mut out: *mut c_char = std::ptr::null_mut();
            let mut ol = 0usize;
            mdfile_optimize(std::ptr::null(), len, std::ptr::null(), std::ptr::null(), &mut out, &mut ol)
        };
        assert_eq!(code, STATUS_TOO_LARGE);
    }

    #[test]
    fn rules_json_lists_every_rule() {
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut len = 0usize;
        let code = mdfile_rules_json(&mut out, &mut len);
        assert_eq!(code, STATUS_OK);
        let v: serde_json::Value = serde_json::from_str(&unsafe { std::ffi::CString::from_raw(out) }.to_str().unwrap()).unwrap();
        let n = v["rules"].as_array().unwrap().len();
        assert_eq!(n, Registry::new().all().len());
        assert_eq!(v["presets"].as_array().unwrap().len(), presets::PRESETS.len());
        for r in v["rules"].as_array().unwrap() {
            assert!(!r["id"].as_str().unwrap().is_empty());
            assert!(!r["name"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn preview_is_headless_and_deterministic() {
        let src = "正文\n\n\n\n正文二\n".as_bytes();
        let mut o1: *mut c_char = std::ptr::null_mut();
        let mut o2: *mut c_char = std::ptr::null_mut();
        let mut l = 0usize;
        let c1 = unsafe { mdfile_preview(src.as_ptr(), src.len(), std::ptr::null(), std::ptr::null(), &mut o1, &mut l) };
        let c2 = unsafe { mdfile_preview(src.as_ptr(), src.len(), std::ptr::null(), std::ptr::null(), &mut o2, &mut l) };
        assert_eq!((c1, c2), (STATUS_OK, STATUS_OK));
        let a = unsafe { std::ffi::CString::from_raw(o1) }.to_str().unwrap().to_string();
        let b = unsafe { std::ffi::CString::from_raw(o2) }.to_str().unwrap().to_string();
        assert_eq!(a, b, "同一输入两次调用必须逐字节一致");
        assert!(a.contains("R01.blank-lines"));
    }

    #[test]
    fn rebuild_honours_reject() {
        let src = "# 标题   \n\n\n\n正文\n".as_bytes();
        let dec = std::ffi::CString::new(r#"[{"decision":"reject","rule_id":"R02.trailing-whitespace"}]"#).unwrap();
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut l = 0usize;
        let code = unsafe { mdfile_rebuild(src.as_ptr(), src.len(), std::ptr::null(), dec.as_ptr(), &mut out, &mut l) };
        assert_eq!(code, STATUS_OK);
        let v: serde_json::Value = serde_json::from_str(&unsafe { std::ffi::CString::from_raw(out) }.to_str().unwrap()).unwrap();
        assert_eq!(v["content_changes"].as_u64(), Some(0));
        let body = v["body"].as_str().unwrap();
        assert!(body.contains("# 标题   "), "被拒绝的规则不应生效：{body:?}");
        assert!(!body.contains("\n\n\n\n"), "其它规则应生效：{body:?}");
    }

    #[test]
    fn analyze_does_not_change_content() {
        let src = "# A\n\n\n\n正文\n".as_bytes();
        let mut out: *mut c_char = std::ptr::null_mut();
        let mut l = 0usize;
        let code = unsafe { mdfile_analyze(src.as_ptr(), src.len(), std::ptr::null(), &mut out, &mut l) };
        assert_eq!(code, STATUS_OK);
        let v: serde_json::Value = serde_json::from_str(&unsafe { std::ffi::CString::from_raw(out) }.to_str().unwrap()).unwrap();
        assert!(v["summary"].as_str().unwrap().contains("综合评分"));
        assert_eq!(v["total_lines"].as_u64(), Some(5));
    }

    #[test]
    fn free_string_accepts_null() {
        unsafe { mdfile_free_string(std::ptr::null_mut()) };
    }
}
