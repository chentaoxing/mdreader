//! 离线 HTML 报告（UI 无关性的可执行证明）。
//!
//! 规格要求「UI Independent」：核心只产出结构化数据，展示层负责渲染。
//! 本模块就是**一个最小展示层**——它只调用 `mdcore` 的公开数据结构
//! （评分、Diff 分类计数、规则链、保护区），不反向影响任何优化决策。
//! 换成 WPF / AppKit / Compose / SwiftUI 时，替换的只是这一层。
//!
//! 报告完全离线：无外链、无脚本、无字体请求，正文一律 HTML 转义。

use mdcore::pipeline::Outcome;
use mdcore::readability::Report as ScoreReport;

/// 转义所有可能破坏 HTML 结构的字符。正文里的 `<` `&` 必须原样可见，
/// 这既是正确性要求，也防止把未信任的 Markdown 当标记注入报告。
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn score_row(name: &str, before: u8, after: u8) -> String {
    let delta = after as i32 - before as i32;
    let tag = match delta {
        0 => "不变".to_string(),
        d if d > 0 => format!("+{d}"),
        d => format!("{d}"),
    };
    format!(
        "<tr><td>{}</td><td class=n>{}</td><td class=n>{}</td><td class=n>{}</td></tr>\n",
        esc(name),
        before,
        after,
        esc(&tag)
    )
}

/// 生成一份完整报告。`out_path` 由调用方决定，本函数不碰文件系统。
pub fn render(
    source_name: &str,
    preset: &str,
    outcome: &Outcome,
    body_bytes: &[u8],
) -> String {
    let d = mdcore::diff::Diff::from_outcome(outcome);
    let before: &ScoreReport = &outcome.before;
    let after: &ScoreReport = &outcome.after;

    let mut cats = String::new();
    for c in &d.categories {
        cats.push_str(&format!("<tr><td>{}</td><td class=n>{}</td></tr>\n", esc(c.label), c.count));
    }
    if cats.is_empty() {
        cats.push_str("<tr><td colspan=2 class=mute>本文件无需结构改动</td></tr>\n");
    }

    let mut applied = String::new();
    for a in &outcome.applied {
        applied.push_str(&format!(
            "<li><code>{}</code> {}（{}，{} 处）— {}</li>\n",
            esc(&a.rule_id),
            esc(&a.rule_name),
            esc(a.scope.label()),
            a.edit_count,
            esc(&a.reason)
        ));
    }
    if applied.is_empty() {
        applied.push_str("<li class=mute>没有规则被触发</li>\n");
    }

    let mut rejected = String::new();
    for r in &outcome.rejected {
        rejected.push_str(&format!("<li><code>{}</code> {}：{}</li>\n", esc(&r.rule_id), esc(&r.rule_name), esc(&r.reason)));
    }

    let mut findings = String::new();
    for f in after.findings.iter().take(20) {
        findings.push_str(&format!("<li>第 {} 行 [{}] {}</li>\n", f.line + 1, esc(f.dimension), esc(&f.detail)));
    }
    if findings.is_empty() {
        findings.push_str("<li class=mute>未发现阅读性问题</li>\n");
    }

    // 输出一律以转义后的文本呈现，便于人工比对
    let rendered = String::from_utf8_lossy(body_bytes);
    let guard_ok = outcome.guard.is_pass() && d.content_changes == 0;
    let badge = if guard_ok {
        "<span class=ok>正文内容变化 0</span>".to_string()
    } else {
        format!("<span class=bad>正文内容变化 {}</span>", d.content_changes)
    };

    format!(
        "<!doctype html>\n<html lang=zh><head><meta charset=utf-8>\n\
         <title>MD 文件助手 · {title}</title>\n<style>{css}</style></head>\n\
         <body>\n<h1>{title}</h1>\n<p class=sub>预设 {preset} · 引擎 {ver} · 收敛轮次 {passes}</p>\n\
         <p>{badge} <span class=chip>结构变化 {struct}</span></p>\n\
         <h2>阅读性评分</h2>\n<table><tr><th>维度</th><th>优化前</th><th>优化后</th><th>变化</th></tr>\n\
         {rows}<tr class=total><td>综合</td><td class=n>{bo}</td><td class=n>{ao}</td><td class=n>{bd}</td></tr></table>\n\
         <h2>结构变化明细</h2>\n<table><tr><th>类别</th><th>处数</th></tr>\n{cats}</table>\n\
         <h2>生效规则（可逐条回滚）</h2>\n<ul>{applied}</ul>\n\
         {rejected_block}\
         <h2>评分提示（仅参考，不驱动任何正文改动）</h2>\n<ul>{findings}</ul>\n\
         <h2>不变性证据</h2>\n<table>\n\
         <tr><td>正文 token 校验和（原）</td><td class=mono>{sum0:016x}</td></tr>\n\
         <tr><td>正文 token 校验和（新）</td><td class=mono>{sum1:016x}</td></tr>\n\
         <tr><td>正文 token 数</td><td class=mono>{tok}</td></tr>\n\
         <tr><td>代码字节</td><td class=mono>{code0} → {code1}</td></tr>\n\
         <tr><td>转义反斜杠数</td><td class=mono>{bs0} → {bs1}</td></tr>\n\
         <tr><td>格式层</td><td class=mono>{fmt}</td></tr>\n\
         </table>\n\
         <h2>优化后全文</h2>\n<pre>{doc}</pre>\n\
         <footer>本报告由 mdcore 生成的结构化数据渲染，页面内无脚本、无外部请求、无联网。</footer>\n\
         </body></html>\n",
        title = esc(source_name),
        css = CSS,
        preset = esc(preset),
        ver = mdcore::ENGINE_VERSION,
        passes = outcome.passes,
        badge = badge,
        struct = d.structural_changes,
        rows = [
            score_row("标题结构", before.heading.value, after.heading.value),
            score_row("段落结构", before.paragraph.value, after.paragraph.value),
            score_row("列表结构", before.list.value, after.list.value),
            score_row("空白分布", before.whitespace.value, after.whitespace.value),
            score_row("信息密度", before.density.value, after.density.value),
        ]
        .join(""),
        bo = before.overall,
        ao = after.overall,
        bd = {
            let delta = after.overall as i32 - before.overall as i32;
            if delta == 0 {
                "不变".to_string()
            } else if delta > 0 {
                format!("+{delta}")
            } else {
                format!("{delta}")
            }
        },
        cats = cats,
        applied = applied,
        rejected_block = if rejected.is_empty() {
            String::new()
        } else {
            format!("<h2>被拒绝的规则（未静默丢弃）</h2>\n<ul>{rejected}</ul>\n")
        },
        findings = findings,
        sum0 = outcome.guard.original.checksum,
        sum1 = outcome.guard.candidate.checksum,
        tok = outcome.guard.original.tokens.len(),
        code0 = outcome.guard.original.counts.code_bytes,
        code1 = outcome.guard.candidate.counts.code_bytes,
        bs0 = outcome.guard.original.backslashes,
        bs1 = outcome.guard.candidate.backslashes,
        fmt = format!(
            "BOM {} / 换行 {} / 统一 {}",
            if outcome.body.starts_with('\u{feff}') { "有" } else { "无" },
            "preserve",
            "是"
        ),
        doc = esc(&rendered),
    )
}

const CSS: &str = "body{font:15px/1.7 -apple-system,Segoe UI,Helvetica,Arial,sans-serif;margin:2rem auto;max-width:56rem;color:#1c1c1e;background:#fafafa}\
h1{font-size:1.5rem;margin:0 0 .2rem}h2{font-size:1.05rem;margin:1.6rem 0 .5rem;border-left:3px solid #0a84ff;padding-left:.5rem}\
.sub{color:#6b7280;margin:.2rem 0 1rem}\
table{border-collapse:collapse;width:100%;background:#fff;font-size:.9rem}\
th,td{border:1px solid #e5e7eb;padding:.4rem .6rem;text-align:left}\
th{background:#f3f4f6;font-weight:600}\ntd.n,td.mono{text-align:right;font-variant-numeric:tabular-nums}\ntd.mono{font-family:ui-monospace,Menlo,Consolas,monospace;text-align:left}\ntr.total td{font-weight:700;background:#f8fafc}\ncode{font-family:ui-monospace,Menlo,Consolas,monospace;background:#eef2ff;padding:.1rem .3rem;border-radius:3px}\npre{background:#fff;border:1px solid #e5e7eb;border-radius:6px;padding:.8rem;overflow:auto;white-space:pre-wrap;word-break:break-word;font-size:.85rem}\nul{margin:.3rem 0;padding-left:1.3rem}li{margin:.15rem 0}\n.ok{background:#dcfce7;color:#166534;padding:.15rem .5rem;border-radius:4px;font-size:.85rem}\n.bad{background:#fee2e2;color:#991b1b;padding:.15rem .5rem;border-radius:4px;font-size:.85rem;font-weight:700}\n.chip{background:#e0f2fe;color:#075985;padding:.15rem .5rem;border-radius:4px;font-size:.85rem;margin-left:.4rem}\n.mute{color:#6b7280}\nfooter{margin-top:2.5rem;color:#6b7280;font-size:.8rem;border-top:1px solid #e5e7eb;padding-top:.6rem}";

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome_of(src: &str) -> (Outcome, Vec<u8>) {
        let registry = mdcore::Registry::new();
        let profile = mdcore::Profile::default();
        let o = mdcore::Optimizer::new(&registry, &profile).run(src).unwrap();
        let doc = mdcore::OpenDocument::from_bytes("x.md", src.as_bytes(), mdcore::NewlineStyle::Preserve).unwrap();
        let bytes = doc.render(&o.body);
        (o, bytes)
    }

    #[test]
    fn html_escapes_untrusted_body() {
        let src = "# 标题\n\n<img src=x onerror=alert(1)> 与 <script>bad()</script>\n\n\n\n正文\n";
        let (o, bytes) = outcome_of(src);
        let html = render("evil.md", "通用文章", &o, &bytes);
        // 正文里的尖括号必须被转义，且不能出现可执行标记
        assert!(!html.contains("<img src=x"), "未转义的 img 注入");
        assert!(!html.contains("<script>bad"), "未转义的 script 注入");
        assert!(html.contains("&lt;img"), "应显示为转义文本");
        // 报告本身不得引入脚本与外链
        assert!(!html.contains("<script>"), "报告不得含脚本");
        assert!(!html.contains("http://") && !html.contains("https://"), "报告不得含外链");
    }

    #[test]
    fn report_shows_zero_content_change_and_both_checksums() {
        let src = "# A\n\n\n\n正文\n\n\n\n正文二\n";
        let (o, bytes) = outcome_of(src);
        let html = render("a.md", "通用文章", &o, &bytes);
        assert!(html.contains("正文内容变化 0"), "应显示零内容变化");
        assert!(html.contains("结构变化"));
        assert!(html.contains(&format!("{:016x}", o.guard.original.checksum)));
        assert_eq!(o.guard.original.checksum, o.guard.candidate.checksum);
    }

    #[test]
    fn cjk_and_emoji_survive_rendering() {
        let src = "# 日本語と한국어과 中文\n\n表情 🎉👩‍👩‍👧‍👦 全角ＡＢＣ 半角ABC\n\n\n\n終わり\n";
        let (o, bytes) = outcome_of(src);
        let html = render("u.md", "通用文章", &o, &bytes);
        assert!(html.contains("👩‍👩‍👧‍👦"), "Emoji 家族符号不得被拆散");
        assert!(html.contains("全角ＡＢＣ"));
        assert!(html.contains("日本語と한국어과 中文"));
    }

    #[test]
    fn rendering_is_deterministic() {
        let src = "# A\n\n\n\n文\n\n\n\n文二\n";
        let (o, bytes) = outcome_of(src);
        let a = render("a.md", "通用文章", &o, &bytes);
        let b = render("a.md", "通用文章", &o, &bytes);
        assert_eq!(a, b);
    }
}
