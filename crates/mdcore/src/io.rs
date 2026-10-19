//! 统一文件访问抽象（规格第十三、三十五、三十六、三十七章）。
//!
//! 核心算法层永远只看到 `&[u8]` / `String`，**不直接触碰任何平台文件系统**。
//! 平台差异（Windows 路径、macOS Security-Scoped Bookmark、Android SAF content://、
//! iOS Files / UIDocumentPicker、Linux 大小写敏感与权限）全部由各 UI 层实现本模块的 trait 吸收。
//!
//! 本模块**不含任何 IO 调用**，因此可编译进 wasm32 / iOS / Android：
//! - `DocumentSource`：读入（Bytes -> Document）
//! - `DocumentStorage`：写出（Document -> Bytes），由平台适配器实现
//! - `FilePicker` / `ShareService` / `ClipboardService`：移动端与桌面端能力位声明
//! - 纯函数：输出文件名推导、批量任务编排、工程清单（开放格式）编解码
//!
//! 铁律（规格第三十三章）：**绝不覆盖原文**。只有显式 overwrite + Content Guard 全绿才允许写回同一路径。

use std::ops::Range;

use crate::error::{CoreError, CoreResult};
use crate::text::{NewlineStyle, TextLayer};

/// 一份待处理的文档（语义层正文 + 格式层信息 + 展示用标识）。
#[derive(Debug, Clone)]
pub struct OpenDocument {
    /// 供人类与 UI 显示的标识：文件名、content URI、资源 id 都行，核心不解析其语义
    pub identifier: String,
    /// 语义层正文（LF 归一化、无 BOM）
    pub body: String,
    /// 原始格式层，用于按原样写回
    pub layer: TextLayer,
    /// 输出时应采用的换行风格
    pub newline: NewlineStyle,
}

impl OpenDocument {
    /// 从字节流构建。非 UTF-8 直接报错，绝不猜测编码（规格第三十二章）。
    pub fn from_bytes(identifier: impl Into<String>, bytes: &[u8], newline: NewlineStyle) -> CoreResult<Self> {
        let layer = TextLayer::from_bytes(bytes)?;
        Ok(OpenDocument {
            identifier: identifier.into(),
            body: layer.body().to_string(),
            layer,
            newline,
        })
    }

    pub fn render(&self, optimized_body: &str) -> Vec<u8> {
        self.layer.render(optimized_body, self.newline)
    }

    /// 平台路径展示（仅用于日志/UI），核心不做路径运算。
    pub fn display(&self) -> &str {
        &self.identifier
    }
}

/// 读入能力：由平台适配器实现（桌面 std::fs、Android ContentResolver、iOS FileManager…）。
pub trait DocumentSource {
    /// 读取一份文档；失败返回错误，不得返回空内容冒充成功
    fn load(&mut self, identifier: &str) -> CoreResult<OpenDocument>;
    /// 列出可处理的文档标识（桌面=目录扫描结果，移动=用户多选结果）
    fn list(&mut self) -> CoreResult<Vec<String>>;
}

/// 写出能力：所有写入都必须经过这里，核心不提供任何「就地改写」的旁路。
pub trait DocumentStorage {
    /// 写入新文件，返回实际落地的标识（路径 / URI）
    fn store(&mut self, identifier: &str, bytes: &[u8]) -> CoreResult<String>;
    /// 是否允许覆盖已有文件。移动端（SAF / iOS）通常返回 false，需走用户确认流程
    fn allow_overwrite(&self) -> bool {
        false
    }
    /// 显式覆盖：仅在调用方已确认且 Guard 全绿时可达
    fn overwrite(&mut self, identifier: &str, bytes: &[u8]) -> CoreResult<String>;
}

/// 文件选择器能力位（规格第三十六、三十七章：不得假设可自由访问文件系统）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PickerCaps {
    /// 是否支持目录级批量扫描（桌面 true，移动 false）
    pub directory_scan: bool,
    /// 是否支持多选文件（移动/桌面均可为 true）
    pub multi_select: bool,
    /// 是否需要持久化授权（Android SAF / iOS Security Scoped Resource）
    pub scoped_permission: bool,
}

/// 分享 / 打开方式（Android share sheet、iOS Share Sheet）。
pub trait ShareService {
    fn share(&mut self, identifier: &str, suggested_name: &str, bytes: &[u8]) -> CoreResult<()>;
}

/// 剪贴板（各平台权限模型不同，交由适配层）。
pub trait ClipboardService {
    fn set_markdown(&mut self, text: &str) -> CoreResult<()>;
    fn get_markdown(&mut self) -> CoreResult<Option<String>>;
}

// ────────────────────────── 输出命名：绝不覆盖原文 ──────────────────────────

/// 规格第三十三章：默认输出 `原名_阅读优化版.md`，冲突时追加序号，永不覆盖。
///
/// `exists` 由平台适配器提供（它才知道 SAF/iOS 上的真实存在性）。
/// 本函数只做字符串推导，不做任何文件系统调用，因此五平台结果一致。
pub fn derive_output_name(original: &str, exists: impl Fn(&str) -> bool) -> String {
    let base = strip_md(original);
    let first = format!("{base}_阅读优化版.md");
    if !exists(&first) {
        return first;
    }
    // 序号一路往上找，直到找到空位：绝不返回一个「已存在」的名字，
    // 否则就违反「永不覆盖原文/产物」的铁律（旧实现在 9999 之后会返回未经检查的 `_x`）。
    let mut n = 2usize;
    loop {
        let cand = format!("{base}_阅读优化版_{n}.md");
        if !exists(&cand) {
            return cand;
        }
        n += 1;
    }
}

/// 取文件主干名：去掉 Markdown 扩展名与本工具已添加的优化版后缀。
///
/// 只做字符串运算（不调用 Path 组件解析），因此同一个标识串在五平台得到同一结果。
fn strip_md(name: &str) -> &str {
    let mut stem = name;
    for ext in [".MARKDOWN", ".MD"] {
        let bl = stem.as_bytes();
        if bl.len() > ext.len() && bl[bl.len() - ext.len()..].eq_ignore_ascii_case(ext.as_bytes()) {
            stem = &stem[..bl.len() - ext.len()];
            break;
        }
    }
    // 去掉已有的优化版后缀，避免出现「优化版_阅读优化版」
    let suffix = "_阅读优化版";
    if let Some(i) = stem.rfind(suffix) {
        let rest = &stem[i + suffix.len()..];
        if rest.is_empty() || (rest.starts_with('_') && rest[1..].chars().all(|c| c.is_ascii_digit())) {
            stem = &stem[..i];
        }
    }
    stem
}

/// 是否是本工具生成的优化版输出（用于避免「优化版的优化版」）。
pub fn is_optimized_output(name: &str) -> bool {
    let lower = name.to_lowercase();
    let stem = lower
        .strip_suffix(".md")
        .or_else(|| lower.strip_suffix(".markdown"))
        .unwrap_or(&lower);
    // 后缀形态：`_阅读优化版` / `_阅读优化版_N`
    let suffix = "阅读优化版";
    match stem.rfind(suffix) {
        Some(i) => {
            let rest = &stem[i + suffix.len()..];
            rest.is_empty() || (rest.starts_with('_') && rest[1..].chars().all(|c| c.is_ascii_digit()))
        }
        None => false,
    }
}

// ────────────────────────── 批量处理（规格第三十三章） ──────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchMode {
    /// 桌面：扫描目录
    DirectoryScan,
    /// 移动：用户多选
    UserSelection,
}

/// 一个批量任务项。核心只产出计划，不执行 IO。
#[derive(Debug, Clone)]
pub struct Job {
    pub source_id: String,
    pub output_name: String,
}

#[derive(Debug, Clone)]
pub struct BatchPlan {
    pub mode: BatchMode,
    pub jobs: Vec<Job>,
    /// 被跳过的项及原因（非 Markdown、非 UTF-8、已是优化版输出…）
    pub skipped: Vec<(String, String)>,
}

/// 判定是否应当纳入批量处理。
///
/// 规则：只认 `.md` / `.markdown`（大小写均允许）；跳过已生成的优化版输出，
/// 避免「优化版的优化版」这类无意义叠加。
pub fn should_include(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if !(lower.ends_with(".md") || lower.ends_with(".markdown")) {
        return Some("不是 Markdown 文件");
    }
    if is_optimized_output(name) {
        return Some("已是阅读优化版输出");
    }
    if name.starts_with('.') {
        return Some("隐藏文件");
    }
    None
}

/// 构造批量计划。`ids` 的顺序即执行顺序（来自平台适配器，核心不再排序，
/// 以保证桌面目录扫描与移动多选的表现与用户所见一致）。
pub fn plan_batch(ids: Vec<String>, mode: BatchMode, exists: impl Fn(&str) -> bool) -> BatchPlan {
    let mut jobs = Vec::new();
    let mut skipped = Vec::new();
    for id in ids {
        if let Some(why) = should_include(&id) {
            skipped.push((id, why.to_string()));
            continue;
        }
        let out = derive_output_name(&id, |n| exists(n));
        jobs.push(Job { source_id: id, output_name: out });
    }
    BatchPlan { mode, jobs, skipped }
}

// ────────────────────────── 工程清单：开放格式（规格第三十四章） ──────────────────────────

/// 项目元数据。落地形态是 `Markdown 正文 + 同名 .json`，不使用任何平台专有数据库，
/// 因此可以整目录拷到另一台机器（含不同操作系统）继续用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub format: String,
    pub engine_version: String,
    pub source_name: String,
    pub output_name: String,
    pub profile: String,
    /// 内容指纹（token 序列校验和），用于跨设备验证「正文未变」
    pub original_checksum: u64,
    pub optimized_checksum: u64,
    pub content_changes: usize,
    pub structural_changes: usize,
    pub passes: usize,
    pub applied_rules: Vec<String>,
}

impl Manifest {
    /// 规格要求的稳定键顺序：五平台写出同一份 JSON，逐字节可比对。
    pub fn to_json(&self) -> String {
        let mut s = String::new();
        s.push_str("{\n");
        s.push_str(&format!("  \"{}\": \"{}\",\n", "format", esc(&self.format)));
        s.push_str(&format!("  \"{}\": \"{}\",\n", "engine_version", esc(&self.engine_version)));
        s.push_str(&format!("  \"{}\": \"{}\",\n", "source_name", esc(&self.source_name)));
        s.push_str(&format!("  \"{}\": \"{}\",\n", "output_name", esc(&self.output_name)));
        s.push_str(&format!("  \"{}\": \"{}\",\n", "profile", esc(&self.profile)));
        s.push_str(&format!("  \"{}\": {},\n", "original_checksum", self.original_checksum));
        s.push_str(&format!("  \"{}\": {},\n", "optimized_checksum", self.optimized_checksum));
        s.push_str(&format!("  \"{}\": {},\n", "content_changes", self.content_changes));
        s.push_str(&format!("  \"{}\": {},\n", "structural_changes", self.structural_changes));
        s.push_str(&format!("  \"{}\": {},\n", "passes", self.passes));
        let rules = self.applied_rules.iter().map(|r| format!("\"{}\"", esc(r))).collect::<Vec<_>>().join(", ");
        s.push_str(&format!("  \"applied_rules\": [{rules}]\n"));
        s.push_str("}\n");
        s
    }

    /// 只解析本模块写出的子集（扁平 string / number 字段 + applied_rules 字符串数组）。
    ///
    /// 解析失败必须报错而不是「尽力而为」，否则跨设备迁移会静默拿到错误元数据。
    pub fn from_json(src: &str) -> CoreResult<Manifest> {
        let get_str = |k: &str| -> CoreResult<String> {
            json_string(src, k).ok_or_else(|| CoreError::Storage(format!("清单缺少字段 {k}")))
        };
        let get_num = |k: &str| -> CoreResult<u64> {
            json_number(src, k).ok_or_else(|| CoreError::Storage(format!("清单缺少字段 {k}")))
        };
        Ok(Manifest {
            format: get_str("format")?,
            engine_version: get_str("engine_version")?,
            source_name: get_str("source_name")?,
            output_name: get_str("output_name")?,
            profile: get_str("profile")?,
            original_checksum: get_num("original_checksum")?,
            optimized_checksum: get_num("optimized_checksum")?,
            content_changes: get_num("content_changes")? as usize,
            structural_changes: get_num("structural_changes")? as usize,
            passes: get_num("passes")? as usize,
            applied_rules: json_string_array(src, "applied_rules"),
        })
    }

    /// 跨设备迁移校验：清单记录的两个指纹必须与本地重新计算的一致。
    ///
    /// 规格第三十五、三十九章：换设备后「正文没变」是可验证的事实，不是乐观假设。
    pub fn verify_against(&self, original_body: &str, optimized_body: &str) -> CoreResult<()> {
        use crate::fingerprint::tokenize;
        let a = tokenize(original_body).checksum;
        let b = tokenize(optimized_body).checksum;
        if a != self.original_checksum {
            return Err(CoreError::Storage("原始正文指纹与清单不一致".into()));
        }
        if b != self.optimized_checksum {
            return Err(CoreError::Storage("优化正文指纹与清单不一致".into()));
        }
        Ok(())
    }
}

/// 从一次优化结果生成清单。
pub fn manifest_for(
    source_name: &str,
    output_name: &str,
    profile: &str,
    o: &crate::pipeline::Outcome,
) -> Manifest {
    Manifest {
        format: "mdfile-assistant-project/1".to_string(),
        engine_version: crate::ENGINE_VERSION.to_string(),
        source_name: source_name.to_string(),
        output_name: output_name.to_string(),
        profile: profile.to_string(),
        original_checksum: o.guard.original.checksum,
        optimized_checksum: o.guard.candidate.checksum,
        content_changes: o.guard.violations.len(),
        structural_changes: o.edit_total(),
        passes: o.passes,
        applied_rules: o.applied.iter().map(|a| a.rule_id.clone()).collect(),
    }
}

// ── 极简 JSON 子集读写（无依赖，保证五平台字节一致） ──

fn esc(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                let hex: String = it.by_ref().take(4).collect();
                if let Ok(v) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(v) {
                        out.push(ch);
                    }
                }
            }
            Some(other) => out.push(other),
            None => break,
        }
    }
    out
}

fn json_string(src: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\": \"");
    let at = src.find(&pat)? + pat.len();
    let rest = &src[at..];
    let mut i = 0usize;
    let bytes = rest.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(unesc(&rest[..i]));
        }
        i += 1;
    }
    None
}

fn json_number(src: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{key}\": ");
    let at = src.find(&pat)? + pat.len();
    let rest = &src[at..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u64>().ok()
}

fn json_string_array(src: &str, key: &str) -> Vec<String> {
    let pat = format!("\"{key}\": [");
    let Some(at) = src.find(&pat) else { return Vec::new() };
    let rest = &src[at + pat.len()..];
    let Some(end) = rest.find(']') else { return Vec::new() };
    let inner = &rest[..end];
    let mut out = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        let start = i + 1;
        let mut j = start;
        while j < bytes.len() {
            if bytes[j] == b'\\' {
                j += 2;
                continue;
            }
            if bytes[j] == b'"' {
                break;
            }
            j += 1;
        }
        out.push(unesc(&inner[start..j.min(inner.len())]));
        i = j + 1;
    }
    out
}

/// 供适配器复用的「行号 -> 区间」辅助：把语义层区间映射回展示用行范围。
pub fn line_span_for(layer_body: &str, range: &Range<usize>) -> (usize, usize) {
    crate::text::Lines::new(layer_body).line_range(range.start, range.end)
}

impl From<&str> for BatchMode {
    fn from(s: &str) -> Self {
        if s.eq_ignore_ascii_case("scan") {
            BatchMode::DirectoryScan
        } else {
            BatchMode::UserSelection
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `exists` 以闭包按值传入（平台适配器提供真实存在性检查）。
    fn no_conflict(_: &str) -> bool {
        false
    }

    #[test]
    fn output_name_never_collides_with_source() {
        assert_eq!(derive_output_name("article.md", no_conflict), "article_阅读优化版.md");
        assert_eq!(derive_output_name("article.markdown", no_conflict), "article_阅读优化版.md");
    }

    #[test]
    fn output_name_bumps_on_conflict_deterministically() {
        let seen = std::cell::RefCell::new(Vec::<String>::new());
        let exists = |n: &str| seen.borrow().contains(&n.to_string());
        let mut got: Vec<String> = Vec::new();
        for _ in 0..3 {
            let name = derive_output_name("article.md", exists);
            seen.borrow_mut().push(name.clone());
            got.push(name);
        }
        assert_eq!(got, vec!["article_阅读优化版.md", "article_阅读优化版_2.md", "article_阅读优化版_3.md"]);
    }

    #[test]
    fn optimized_output_is_not_reprocessed() {
        assert_eq!(should_include("article_阅读优化版.md"), Some("已是阅读优化版输出"));
        assert_eq!(should_include("article_阅读优化版_2.md"), Some("已是阅读优化版输出"));
        assert_eq!(should_include("notes.txt"), Some("不是 Markdown 文件"));
        assert_eq!(should_include("notes.md"), None);
        assert_eq!(should_include("NOTES.MD"), None);
        assert_eq!(should_include("readme.markdown"), None);
    }

    #[test]
    fn batch_plan_keeps_user_order() {
        let ids = vec!["b.md".to_string(), "skip.txt".to_string(), "a.md".to_string()];
        let p = plan_batch(ids, BatchMode::UserSelection, |_| false);
        assert_eq!(p.jobs.iter().map(|j| j.source_id.as_str()).collect::<Vec<_>>(), vec!["b.md", "a.md"]);
        assert_eq!(p.skipped.len(), 1);
    }

    #[test]
    fn manifest_roundtrip_is_byte_stable() {
        let m = Manifest {
            format: "mdfile-assistant-project/1".into(),
            engine_version: "0.1.0".into(),
            source_name: "笔记.md".into(),
            output_name: "笔记_阅读优化版.md".into(),
            profile: "技术文档".into(),
            original_checksum: 1234567890,
            optimized_checksum: 9876543210,
            content_changes: 0,
            structural_changes: 7,
            passes: 2,
            applied_rules: vec!["R01.blank-lines".into(), "R02.trailing-whitespace".into()],
        };
        let json = m.to_json();
        let back = Manifest::from_json(&json).unwrap();
        assert_eq!(m, back);
        // 同一份数据在任意平台写出同一串字节
        assert_eq!(json, m.to_json());
    }

    #[test]
    fn manifest_json_parses_with_a_strict_parser() {
        // 回归：曾经写出 `  , "applied_rules"`（双逗号），自研 from_json 读得动、
        // 任何标准 JSON 解析器都会失败 => 跨设备迁移/其它工具打开 .json 直接坏掉。
        let m = Manifest {
            format: "mdfile-assistant-project/1".into(),
            engine_version: "0.1.0".into(),
            source_name: "笔记.md".into(),
            output_name: "笔记_阅读优化版.md".into(),
            profile: "技术文档".into(),
            original_checksum: 1234567890,
            optimized_checksum: 9876543210,
            content_changes: 0,
            structural_changes: 7,
            passes: 2,
            applied_rules: vec!["R01.blank-lines".into(), "R02.trailing-whitespace".into()],
        };
        let json = m.to_json();
        let v: serde_json::Value = serde_json::from_str(&json).expect("清单必须是合法 JSON");
        assert_eq!(v["applied_rules"].as_array().unwrap().len(), 2);
        assert_eq!(v["source_name"].as_str(), Some("笔记.md"));
        assert!(!json.contains(",\n  ,"), "不得出现连续逗号：{json}");
    }

    #[test]
    fn output_name_never_returns_a_taken_name_even_past_9999() {
        // 回归：旧实现找满 9999 后返回未经 exists 检查的 `_x`，会覆盖已有产物。
        let mut taken = std::collections::HashSet::new();
        taken.insert("article_阅读优化版.md".to_string());
        for n in 2..=10001usize {
            taken.insert(format!("article_阅读优化版_{n}.md"));
        }
        let got = derive_output_name("article.md", |n| taken.contains(n));
        assert_eq!(got, "article_阅读优化版_10002.md", "必须继续往后找空位，绝不返回已存在的名字");
        assert!(!taken.contains(&got));
    }

    #[test]
    fn manifest_rejects_broken_input() {
        assert!(Manifest::from_json("{}").is_err(), "缺字段必须报错，不得静默给默认值");
        assert!(Manifest::from_json("{\"format\": \"x\"}").is_err());
    }

    #[test]
    fn checksum_verification_catches_content_drift() {
        let o = crate::pipeline::optimize("# A\n\n\n\n正文\n").unwrap();
        let m = manifest_for("a.md", "a_阅读优化版.md", "通用文章", &o);
        assert!(m.verify_against(&o.original_body, &o.body).is_ok());
        // 有人改了正文 -> 必须发现
        let drift = o.body.replace("正文", "正文啊");
        assert!(m.verify_against(&o.original_body, &drift).is_err());
    }

    #[test]
    fn document_bytes_roundtrip_keeps_format_layer() {
        let raw = "\u{feff}# A\r\n正文\r\n".to_string().into_bytes();
        let d = OpenDocument::from_bytes("a.md", &raw, NewlineStyle::Preserve).unwrap();
        assert_eq!(d.body, "# A\n正文\n", "语义层应为 LF 且无 BOM");
        let out = d.render("# A\n\n正文\n");
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with('\u{feff}') && text.contains("\r\n"), "写回应保留格式层：{text:?}");
    }

    #[test]
    fn invalid_utf8_is_rejected_not_guessed() {
        let bad = [0xffu8, 0xfe, 0x00, 0x41];
        let e = OpenDocument::from_bytes("a.md", &bad, NewlineStyle::Preserve).unwrap_err();
        assert!(matches!(e, CoreError::InvalidUtf8), "{e}");
    }

    #[test]
    fn line_span_maps_into_display_lines() {
        let body = "# A\n\n正文\n";
        let range = 5..8;
        let (a, b) = line_span_for(body, &range);
        assert!(a <= b && b >= 1, "{a} {b}");
    }

    #[test]
    fn escaping_handles_quotes_and_backslashes() {
        assert_eq!(esc("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(unesc("a\\\"b\\\\c"), "a\"b\\c");
        assert_eq!(unesc("\\u4e2d\\u6587"), "中文");
    }
}
