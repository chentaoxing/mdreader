//! MD文档阅读助手核心的 WASM 门面（Android 侧 `mdcore-jni` 的浏览器兄弟）。
//!
//! 与 JNI 版同一契约：每个入口吃字节/字符串、吐一个 **JSON 信封**——
//! `{"status":0,"payload":…}`（成功）或 `{"status":n,"error":"…"}`（失败）。
//! 状态码表见 `mdfile.h`。排版判定一行不在这里，全部来自 mdcore。
//!
//! 为什么是独立 workspace：wasm32 只支持 panic=abort，与根仓库为 FFI 保留的
//! panic="unwind" 有意分开。

use wasm_bindgen::prelude::*;

fn envelope<F>(f: F) -> String
where
    F: FnOnce() -> Result<String, (i32, String)>,
{
    match f() {
        Ok(json) => format!("{{\"status\":0,\"payload\":{json}}}"),
        Err((code, msg)) => {
            let esc: String = msg
                .chars()
                .map(|c| match c {
                    '"' => "\\\"".into(),
                    '\\' => "\\\\".into(),
                    '\n' => "\\n".into(),
                    c => c.to_string(),
                })
                .collect();
            format!("{{\"status\":{code},\"error\":\"{esc}\"}}")
        }
    }
}

/// mdcore 错误 → mdfile.h 状态码（与 mdcore-ffi 同口径；未知预设/规则在下方自行拦截为 4）。
fn status_of(e: &mdcore::CoreError) -> i32 {
    use mdcore::CoreError::*;
    match e {
        InvalidUtf8 { .. } => 1,
        TooLarge { .. } => 1_048_576 + 2, // wasm 层内部约定：核心不该在 64MiB 内报这个；防御
        GuardRejected { .. } => 3,
        _ => 5,
    }
}

fn open_doc(bytes: &[u8], newline: &str) -> Result<mdcore::io::OpenDocument, (i32, String)> {
    use mdcore::text::NewlineStyle as NS;
    let style = match newline {
        "lf" => NS::Lf,
        "crlf" => NS::CrLf,
        "cr" => NS::Cr,
        _ => NS::Preserve,
    };
    mdcore::io::OpenDocument::from_bytes("document", bytes, style)
        .map_err(|e| (status_of(&e), e.to_string()))
}

fn profile(preset: &str) -> Result<mdcore::Profile, (i32, String)> {
    mdcore::presets::profile_for(preset).ok_or_else(|| {
        (
            4,
            format!("未知预设「{preset}」，可选：{}", mdcore::presets::PRESETS.join(" / ")),
        )
    })
}

#[derive(serde::Serialize)]
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

#[wasm_bindgen]
pub fn engine_version() -> String {
    mdcore::ENGINE_VERSION.to_string()
}

#[wasm_bindgen]
pub fn rules_json() -> String {
    envelope(|| {
        let reg = mdcore::Registry::new();
        #[derive(serde::Serialize)]
        struct Rule<'a> {
            id: &'a str,
            name: &'a str,
            description: &'a str,
            priority: i32,
            safety: &'static str,
        }
        #[derive(serde::Serialize)]
        struct Reply<'a> {
            engine_version: &'static str,
            presets: Vec<&'a str>,
            rules: Vec<Rule<'a>>,
        }
        let reply = Reply {
            engine_version: mdcore::ENGINE_VERSION,
            presets: mdcore::presets::PRESETS.to_vec(),
            rules: reg
                .all()
                .iter()
                .map(|r| Rule {
                    id: r.id(),
                    name: r.name(),
                    description: r.description(),
                    priority: r.priority(),
                    safety: match r.safety() {
                        mdcore::rules::SafetyLevel::Conservative => "conservative",
                        mdcore::rules::SafetyLevel::Standard => "standard",
                        mdcore::rules::SafetyLevel::Aggressive => "aggressive",
                    },
                })
                .collect(),
        };
        serde_json::to_string(&reply).map_err(|e| (5u32 as i32, e.to_string()))
    })
}

#[wasm_bindgen]
pub fn optimize(input: &[u8], preset: &str, newline: &str) -> String {
    envelope(|| {
        let doc = open_doc(input, newline)?;
        let prof = profile(preset)?;
        let outcome = mdcore::Optimizer::new(&mdcore::Registry::new(), &prof)
            .run(&doc.body)
            .map_err(|e| (status_of(&e), e.to_string()))?;
        let diff = mdcore::diff::Diff::from_outcome(&outcome);
        let rendered = doc.render(&outcome.body);
        let manifest = mdcore::io::manifest_for("document", "document_阅读优化版.md", preset, &outcome);

        #[derive(serde::Serialize)]
        struct Applied<'a> {
            rule_id: &'a str,
            rule_name: &'a str,
            scope: String,
            edits: usize,
            reason: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Rejected<'a> {
            rule_id: &'a str,
            rule_name: &'a str,
            reason: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Categories<'a> {
            label: &'a str,
            count: usize,
        }
        #[derive(serde::Serialize)]
        struct Reply<'a> {
            engine_version: &'static str,
            preset: String,
            body: String,
            /// latin1 逐字节编码的完整文件字节（含 BOM 与原换行风格）；下载前逐位还原
            rendered_latin1: String,
            newline_style: String,
            had_bom: bool,
            passes: usize,
            content_changes: usize,
            structural_changes: usize,
            applied: Vec<Applied<'a>>,
            rejected: Vec<Rejected<'a>>,
            categories: Vec<Categories<'a>>,
            score_before: ScoreWire,
            score_after: ScoreWire,
            manifest_json: String,
            checksum_original_str: String,
            checksum_optimized_str: String,
        }
        let reply = Reply {
            engine_version: mdcore::ENGINE_VERSION,
            preset: preset.to_string(),
            body: outcome.body.clone(),
            rendered_latin1: rendered.iter().map(|&b| b as char).collect(),
            newline_style: match doc.newline {
                mdcore::text::NewlineStyle::Lf => "lf".to_string(),
                mdcore::text::NewlineStyle::CrLf => "crlf".to_string(),
                mdcore::text::NewlineStyle::Cr => "cr".to_string(),
                mdcore::text::NewlineStyle::Preserve => "preserve".to_string(),
            },
            had_bom: doc.layer.has_bom(),
            passes: outcome.passes,
            content_changes: diff.content_changes,
            structural_changes: diff.structural_changes,
            applied: outcome
                .applied
                .iter()
                .map(|a| Applied {
                    rule_id: &a.rule_id,
                    rule_name: &a.rule_name,
                    scope: a.scope.label().to_string(),
                    edits: a.edit_count,
                    reason: &a.reason,
                })
                .collect(),
            rejected: outcome
                .rejected
                .iter()
                .map(|r| Rejected {
                    rule_id: &r.rule_id,
                    rule_name: &r.rule_name,
                    reason: &r.reason,
                })
                .collect(),
            categories: diff
                .categories
                .iter()
                .map(|c| Categories { label: c.label, count: c.count })
                .collect(),
            score_before: score_of(&outcome.before),
            score_after: score_of(&outcome.after),
            manifest_json: manifest.to_json(),
            checksum_original_str: outcome.guard.original.checksum.to_string(),
            checksum_optimized_str: outcome.guard.candidate.checksum.to_string(),
        };
        serde_json::to_string(&reply).map_err(|e| (5, e.to_string()))
    })
}

#[wasm_bindgen]
pub fn analyze(input: &[u8], preset: &str) -> String {
    envelope(|| {
        let doc = open_doc(input, "preserve")?;
        let prof = profile(preset)?;
        let r = mdcore::Optimizer::new(&mdcore::Registry::new(), &prof)
            .analyze(&doc.body)
            .map_err(|e| (status_of(&e), e.to_string()))?;
        #[derive(serde::Serialize)]
        struct Finding<'a> {
            line: usize,
            dimension: &'a str,
            detail: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Reply<'a> {
            summary: String,
            score: ScoreWire,
            findings: Vec<Finding<'a>>,
        }
        let reply = Reply {
            summary: mdcore::readability::render_summary(&r),
            score: score_of(&r),
            findings: r
                .findings
                .iter()
                .take(20)
                .map(|f| Finding { line: f.line, dimension: f.dimension, detail: &f.detail })
                .collect(),
        };
        serde_json::to_string(&reply).map_err(|e| (5, e.to_string()))
    })
}

#[wasm_bindgen]
pub fn rebuild(original: &[u8], preset: &str, decisions_json: &str) -> String {
    envelope(|| {
        let doc = open_doc(original, "preserve")?;
        let prof = profile(preset)?;
        let outcome = mdcore::Optimizer::new(&mdcore::Registry::new(), &prof)
            .run(&doc.body)
            .map_err(|e| (status_of(&e), e.to_string()))?;
        #[derive(serde::Deserialize)]
        struct DecisionWire {
            decision: String,
            rule_id: String,
        }
        let items: Vec<DecisionWire> = serde_json::from_str(decisions_json)
            .map_err(|e| (4, format!("决策 JSON 解析失败：{e}")))?;
        let mut log = mdcore::diff::DecisionLog::default();
        for d in items {
            let kind = match d.decision.as_str() {
                "accept" => mdcore::diff::Decision::Accept,
                "reject" => mdcore::diff::Decision::Reject,
                "undo" => mdcore::diff::Decision::Undo,
                "redo" => mdcore::diff::Decision::Redo,
                "restore" => mdcore::diff::Decision::RestoreOriginal,
                other => return Err((4, format!("未知决策类型 {other}"))),
            };
            log.record(kind, &d.rule_id);
        }
        let body = log
            .rebuild(&doc.body, &outcome, &mdcore::Registry::new(), &prof)
            .map_err(|e| (status_of(&e), e.to_string()))?;
        let verdict = mdcore::guard::verify(&doc.body, &body);
        if !verdict.violations.is_empty() {
            return Err((3, "重建后正文发生变化，已丢弃".into()));
        }
        let rendered = doc.render(&body);
        #[derive(serde::Serialize)]
        struct Reply {
            body: String,
            rendered_latin1: String,
            newline_style: String,
            had_bom: bool,
            content_changes: usize,
            checksum_str: String,
        }
        let reply = Reply {
            content_changes: 0,
            body,
            rendered_latin1: rendered.iter().map(|&b| b as char).collect(),
            newline_style: match doc.newline {
                mdcore::text::NewlineStyle::Lf => "lf".to_string(),
                mdcore::text::NewlineStyle::CrLf => "crlf".to_string(),
                mdcore::text::NewlineStyle::Cr => "cr".to_string(),
                mdcore::text::NewlineStyle::Preserve => "preserve".to_string(),
            },
            had_bom: doc.layer.has_bom(),
            checksum_str: mdcore::fingerprint::tokenize(&doc.body).checksum.to_string(),
        };
        serde_json::to_string(&reply).map_err(|e| (5, e.to_string()))
    })
}
