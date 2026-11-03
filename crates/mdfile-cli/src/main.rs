//! mdfile —— MD文档阅读助手命令行前端。
//!
//! 它的存在只为证明一件事（规格第七章）：**核心引擎完全无头可用**。
//! 本二进制不含任何 UI 依赖，所有 Markdown 语义决策都来自 `mdcore`；
//! 这里只负责「读字节 / 调核心 / 写字节 / 打印」。
//!
//! 用法：
//! ```text
//! mdfile analyze <file.md>...              阅读性评分（只读，不改文件）
//! mdfile optimize <file.md> [--preset N]   输出 `原名_阅读优化版.md` + 清单，绝不覆盖原文
//! mdfile diff <file.md>                    结构变化分类计数 + 正文内容变化（恒为 0）
//! mdfile preview <file.md> [--rule ID]     规则干跑，不写盘
//! mdfile rules                             列出规则及其 ID / 优先级 / 安全级 / 作用域
//! mdfile batch <dir>                       递归扫描目录批量优化
//! mdfile check <file.md>...                内容不变性回归门，失败退出码 1（CI 用）
//! mdfile report <file.md>|<dir>            生成离线 HTML 阅读优化报告（不联网、无脚本）
//! mdfile fingerprint <file.md>...          跨平台一致性指纹（五平台逐字节对比）
//! ```
//!
//! 全局开关：`--newline keep|lf|crlf|cr`（默认 keep，即保留原文件格式层）。

mod report;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mdcore::diff::Diff;
use mdcore::io::{manifest_for, plan_batch, BatchMode, OpenDocument};
use mdcore::presets;
use mdcore::rules::{Profile, Registry, SafetyLevel};
use mdcore::text::NewlineStyle;
use mdcore::NewlineStyle as NS;

/// 单文件结果，供多个子命令复用。
struct Done {
    /// 已按原文件格式层（BOM / 换行风格 / 结尾换行）渲染好的输出字节
    bytes: Vec<u8>,
    outcome: mdcore::Outcome,
}

/// 解析后的命令行：命令、文件表、全局开关。
struct Args {
    cmd: String,
    files: Vec<String>,
    newline: NewlineStyle,
    preset: String,
    /// preset 是否由 `--preset`/`--preset=` 开关给出：fingerprint 用它区分
    /// 「用户显式选的预设」与「环境变量/默认值」。v0.1.4 之前用
    /// `raw.iter().any(|a| a == "--preset")` 事后扫描，`--preset=小说` 等号形态
    /// 检测不到，对账会静默按错预设出结果。
    preset_from_flag: bool,
    rule: Option<String>,
    to_stdout: bool,
    help: bool,
}

/// 把「开关」与「位置参数」分开。
///
/// 关键：`--newline/--preset/--rule` 的**值必须随开关一起消费掉**。
/// 旧实现只过滤以 `--` 开头的项，于是 `optimize a.md --preset 技术文档`
/// 会把「技术文档」当成第二个待处理文件，报「读取失败：技术文档」——
/// 而 README「三十秒跑起来」里的命令正是这个写法。
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut newline = NewlineStyle::Preserve;
    let mut preset = presets::GENERIC.to_string();
    let mut preset_from_flag = false;
    let mut rule: Option<String> = None;
    let mut to_stdout = false;
    let mut help = false;

    let value = |i: &mut usize, name: &str| -> Result<String, String> {
        let v = args.get(*i + 1).cloned().ok_or_else(|| format!("{name} 缺少参数值"))?;
        if v.starts_with("--") {
            return Err(format!("{name} 缺少参数值（拿到的是另一个开关 {v}）"));
        }
        *i += 1;
        Ok(v)
    };

    let mut i = 0usize;
    while i < args.len() {
        let a = &args[i];
        match a.as_str() {
            "--newline" => {
                let v = value(&mut i, "--newline")?;
                newline = match v.as_str() {
                    "lf" => NS::Lf,
                    "crlf" => NS::CrLf,
                    "cr" => NS::Cr,
                    // v0.1.4：两种形态接受集统一。此前空格形态只认 keep、
                    // 等号形态额外认 preserve，同一语义两种拼写两种结果。
                    "keep" | "preserve" => NS::Preserve,
                    other => return Err(format!("未知的 --newline 值「{other}」，可选：keep|preserve|lf|crlf|cr")),
                };
            }
            "--preset" => {
                preset = value(&mut i, "--preset")?;
                preset_from_flag = true;
            }
            "--rule" => rule = Some(value(&mut i, "--rule")?),
            "--stdout" => to_stdout = true,
            "--help" | "-h" => help = true,
            other => {
                if let Some(flag) = other.strip_prefix("--") {
                    // 只允许等号形态 --preset=X，其它未知开关直接拒绝，避免拼错被当成文件名
                    if let Some((name, val)) = flag.split_once('=') {
                        match name {
                            "newline" => {
                                newline = match val {
                                    "lf" => NS::Lf,
                                    "crlf" => NS::CrLf,
                                    "cr" => NS::Cr,
                                    "keep" | "preserve" => NS::Preserve,
                                    o => return Err(format!("未知的 --newline 值「{o}」")),
                                }
                            }
                            "preset" => {
                                preset = val.to_string();
                                preset_from_flag = true;
                            }
                            "rule" => rule = Some(val.to_string()),
                            o => return Err(format!("未知开关 --{o}")),
                        }
                    } else {
                        return Err(format!("未知开关 --{flag}"));
                    }
                } else {
                    positional.push(other.to_string());
                }
            }
        }
        i += 1;
    }

    let cmd = positional.first().cloned().unwrap_or_else(|| "help".to_string());
    // 注意：必须用 get(1..) —— 直接写 positional[1..] 在「只有开关、没有命令」时
    // 会 panic（旧实现的真实故障：`mdfile` 与 `mdfile --help` 都以 101 退出）。
    let files = positional.get(1..).map(|s| s.to_vec()).unwrap_or_default();
    Ok(Args { cmd, files, newline, preset, preset_from_flag, rule, to_stdout, help })
}

fn main() -> ExitCode {
    // v0.1.4：std::env::args() 在参数含未配对代理项（Windows WTF-16）或非 UTF-8 字节
    // （Unix）时会直接 panic（exit 101，无任何提示）。文件名可以是任意字节序列，
    // 所以必须用 args_os()：非法 Unicode 明确报错退出 2，而不是崩溃。
    let mut raw: Vec<String> = Vec::new();
    let mut bad_args: Vec<String> = Vec::new();
    for a in std::env::args_os().skip(1) {
        match a.into_string() {
            Ok(s) => raw.push(s),
            Err(os) => bad_args.push(os.to_string_lossy().into_owned()),
        }
    }
    if !bad_args.is_empty() {
        eprintln!("以下参数名不是合法 Unicode，本工具无法处理（请重命名后重试）：");
        for b in &bad_args {
            eprintln!("  {b}");
        }
        return ExitCode::from(2);
    }
    let args = match parse_args(&raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n");
            usage();
            return ExitCode::from(2);
        }
    };
    if args.help || args.cmd == "help" {
        usage();
        return ExitCode::SUCCESS;
    }

    let registry = Registry::new();
    let profile = match presets::profile_for(&args.preset) {
        Some(p) => p,
        None => {
            eprintln!("未知预设「{}」，可选：{}", args.preset, presets::PRESETS.join(" / "));
            return ExitCode::from(2);
        }
    };

    let code = match args.cmd.as_str() {
        "analyze" => cmd_analyze(&registry, &profile, &args.files),
        "optimize" => cmd_optimize(&registry, &profile, &args.files, args.newline, args.to_stdout, &args.preset),
        "diff" => cmd_diff(&registry, &profile, &args.files, args.newline, &args.preset),
        "preview" => cmd_preview(&registry, &profile, &args.files, args.rule.clone()),
        "rules" => cmd_rules(&registry),
        "batch" => cmd_batch(&registry, &profile, &args.files, args.newline, &args.preset),
        "check" => cmd_check(&registry, &profile, &args.files),
        "report" => cmd_report(&registry, &profile, &args.files, args.newline, &args.preset),
        "fingerprint" => cmd_fingerprint(&args.files, &args.preset, args.preset_from_flag),
        other => {
            eprintln!("未知命令：{other}\n");
            usage();
            2
        }
    };
    ExitCode::from(code)
}

fn usage() {
    eprintln!(
        "mdfile —— MD文档阅读助手（核心引擎命令行前端，v{}）\n\
         \n\
         用法：mdfile <命令> [文件...] [开关]\n\
         \n\
         命令：\n\
         \x20 analyze   阅读性评分（只读）\n\
         \x20 optimize  生成 `原名_阅读优化版.md` + 同名 .json 清单（绝不覆盖原文）\n\
         \x20 diff      结构变化分类计数 + 正文内容变化\n\
         \x20 preview   规则干跑预览（不写盘）\n\
         \x20 rules     列出全部规则\n\
         \x20 batch     递归扫描目录批量优化\n\
         \x20 check     内容不变性回归门，任一项失败退出码 1\n\
         \x20 report    生成离线 HTML 阅读优化报告（只新增 .html，不改原文件）\n\
         \x20 fingerprint 输出跨平台一致性指纹（五平台逐字节对比用）\n\
         \n\
         开关：\n\
         \x20--preset 通用文章|技术文档|AI 生成文档|小说（也支持 --preset=值）\n\
         \x20--newline keep|lf|crlf|cr\n\
         \x20--rule <ID>   仅预览某条规则（可省略后缀：R01 = R01.blank-lines）\n\
         \x20--stdout      结果打到标准输出，不落盘\n\
         \n\
         \x20fingerprint 预设优先级：--preset > 环境变量 MDFILE_PRESET > 默认「通用文章」\n",
        mdcore::ENGINE_VERSION
    );
}

/// 读入文件字节；超过核心上限时在读盘前就拒绝。
/// 上限真相源是 `mdcore::io::MAX_INPUT_BYTES`（v0.1.4 起 `from_bytes` 内还有第二道
/// 兜底，防 stat 与 read 之间文件长大的竞态）；此前 CLI 直连核心把 64 MiB 上限
/// 整个绕过，66 MiB 的文件被照单全收（约 60 秒、数 GB 内存）。
fn read_capped(path: &Path) -> Result<Vec<u8>, String> {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() as usize > mdcore::io::MAX_INPUT_BYTES {
            return Err(format!(
                "{}: 输入超过上限 {} 字节（实际 {} 字节）",
                path.display(),
                mdcore::io::MAX_INPUT_BYTES,
                meta.len()
            ));
        }
    }
    std::fs::read(path).map_err(|e| format!("{}: 读取失败 {e}", path.display()))
}

/// 读入 + 优化，全部字节级细节交给核心的两层模型。
fn process(registry: &Registry, profile: &Profile, path: &Path, newline: NewlineStyle) -> Result<Done, String> {
    let bytes = read_capped(path)?;
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("untitled.md").to_string();
    let doc = OpenDocument::from_bytes(&name, &bytes, newline).map_err(|e| format!("{name}: {e}"))?;
    let optimizer = mdcore::Optimizer::new(registry, profile);
    let outcome = optimizer.run(&doc.body).map_err(|e| format!("{name}: {e}"))?;
    Ok(Done { bytes: doc.render(&outcome.body), outcome })
}

fn cmd_analyze(registry: &Registry, profile: &Profile, files: &[String]) -> u8 {
    let optimizer = mdcore::Optimizer::new(registry, profile);
    for f in files {
        let path = Path::new(f);
        let bytes = match read_capped(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        let doc = match OpenDocument::from_bytes(f, &bytes, NewlineStyle::Preserve) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{f}: {e}");
                return 1;
            }
        };
        match optimizer.analyze(&doc.body) {
            Ok(r) => {
                println!("{f}");
                println!("  {}", mdcore::readability::render_summary(&r));
                println!(
                    "  规模 行 {} / 标题 {} / 段落 {} / 列表项 {} / 正文 token {}",
                    r.total_lines, r.headings, r.paragraphs, r.list_items, r.content_tokens
                );
                for fd in r.findings.iter().take(10) {
                    println!("  · 第 {} 行 [{}] {}", fd.line + 1, fd.dimension, fd.detail);
                }
            }
            Err(e) => {
                eprintln!("{f}: 分析失败 {e}");
                return 1;
            }
        }
    }
    0
}

fn cmd_optimize(
    registry: &Registry,
    profile: &Profile,
    files: &[String],
    newline: NewlineStyle,
    to_stdout: bool,
    preset: &str,
) -> u8 {
    for f in files {
        let path = Path::new(f);
        let done = match process(registry, profile, path, newline) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        let diff = Diff::from_outcome(&done.outcome);
        if to_stdout {
            let mut out = std::io::stdout();
            let _ = out.write_all(&done.bytes);
            let _ = out.flush();
            eprintln!("{}", diff.render());
            continue;
        }
        // 绝不覆盖原文：命名与冲突避让由核心推导，存在性由本机 fs 提供
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        let stem = path.file_name().and_then(|s| s.to_str()).unwrap_or("untitled.md").to_string();
        let exists = |n: &str| dir.join(n).exists();
        let name = mdcore::io::derive_output_name(&stem, exists);
        let out_path = dir.join(&name);
        if let Err(e) = std::fs::write(&out_path, &done.bytes) {
            eprintln!("写入失败：{e}");
            return 1;
        }
        let manifest = manifest_for(&stem, &name, preset, &done.outcome);
        let meta_path = out_path.with_extension("json");
        if let Err(e) = std::fs::write(&meta_path, manifest.to_json()) {
            eprintln!("清单写入失败：{e}");
            return 1;
        }
        println!("✓ {}", out_path.display());
        println!("  {}", meta_path.display());
        print!("{}", diff.render());
        for r in &done.outcome.rejected {
            println!("  ! 规则 {} 未生效：{}", r.rule_id, r.reason);
        }
        println!(
            "  评分 {} → {}",
            done.outcome.before.overall, done.outcome.after.overall
        );
    }
    0
}

fn cmd_diff(
    registry: &Registry,
    profile: &Profile,
    files: &[String],
    newline: NewlineStyle,
    preset: &str,
) -> u8 {
    for f in files {
        let path = Path::new(f);
        let done = match process(registry, profile, path, newline) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{e}");
                return 1;
            }
        };
        println!("{f}  预设：{preset}");
        let mut d = Diff::from_outcome(&done.outcome);
        d.format_only = done.outcome.body == done.outcome.original_body;
        print!("{}", d.render());
        for e in d.entries.iter().take(60) {
            println!("  行 {:>4} [{}] {}：{} ⇒ {}", e.line, e.rule_id, e.reason, e.before, e.after);
        }
        if !d.acceptable() {
            eprintln!("✗ 正文内容变化非 0，禁止接受本次优化");
            return 1;
        }
    }
    0
}

fn cmd_preview(registry: &Registry, profile: &Profile, files: &[String], rule: Option<String>) -> u8 {
    // `--rule R01` 这种简写要能解析成唯一的全 ID；歧义或不存在时明确报错，
    // 旧实现直接把 `R01` 传给核心，回一句看不出原因的 "unknown rule id: R01"。
    let rule: Option<String> = match rule {
        None => None,
        Some(raw) => match resolve_rule_id(registry, &raw) {
            Ok(id) => Some(id),
            Err(msg) => {
                eprintln!("{msg}");
                return 2;
            }
        },
    };
    for f in files {
        let path = Path::new(f);
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{f}: 读取失败 {e}");
                return 1;
            }
        };
        let doc = match OpenDocument::from_bytes(f, &bytes, NewlineStyle::Preserve) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{f}: {e}");
                return 1;
            }
        };
        let optimizer = mdcore::Optimizer::new(registry, profile);
        match &rule {
            Some(id) => match optimizer.preview_rule(&doc.body, id) {
                Ok(Some(p)) => {
                    println!("{} {}（{}）", p.rule_id, p.rule_name, p.scope.label());
                    println!("  {}", p.description);
                    println!("  编辑数 {}", p.edit_count);
                    if let Some(r) = p.reject_reason {
                        println!("  已拒绝：{r}");
                    }
                    if let Some(next) = p.result {
                        println!("  ── 预览（不写盘）──");
                        for line in next.lines().take(40) {
                            println!("  │ {line}");
                        }
                    }
                }
                Ok(None) => {
                    println!("{id}: 无改动");
                }
                Err(e) => {
                    eprintln!("{id}: {e}");
                    return 1;
                }
            },
            None => match optimizer.preview_all(&doc.body) {
                Ok(ps) => {
                    println!("{f} 规则干跑（不写盘）");
                    for p in ps {
                        let status = if p.edit_count == 0 {
                            "无改动".to_string()
                        } else if let Some(r) = &p.reject_reason {
                            format!("拒绝：{r}")
                        } else {
                            format!("{} 处编辑", p.edit_count)
                        };
                        println!("  {:<24} {:<12} {}", p.rule_id, p.scope.label(), status);
                    }
                }
                Err(e) => {
                    eprintln!("{f}: {e}");
                    return 1;
                }
            },
        }
    }
    0
}

fn cmd_rules(registry: &Registry) -> u8 {
    println!("{:<24} {:<24} {:>8} {:<12} {}", "ID", "名称", "优先级", "安全级", "作用域");
    for r in registry.all() {
        let safety = match r.safety() {
            SafetyLevel::Conservative => "保守",
            SafetyLevel::Standard => "标准",
            SafetyLevel::Aggressive => "激进(v1 禁用)",
        };
        println!(
            "{:<24} {:<24} {:>8} {:<12} {}",
            r.id(),
            r.name(),
            r.priority(),
            safety,
            r.scope().label()
        );
    }
    println!("\n全部规则均须通过 Content Guard（正文 token 序列比对）后才可落盘。");
    0
}

fn cmd_batch(registry: &Registry, profile: &Profile, files: &[String], newline: NewlineStyle, preset: &str) -> u8 {
    let Some(dir) = files.first() else {
        eprintln!("batch 需要一个目录参数");
        return 2;
    };
    let root = Path::new(dir);
    if !root.is_dir() {
        eprintln!("{dir} 不是目录");
        return 2;
    }
    let mut found: Vec<PathBuf> = Vec::new();
    walk(root, &mut found);
    found.sort();
    // 批量计划由核心生成（含跳过原因），标识用完整路径，输出名仍按文件名推导
    let ids: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
    let plan = plan_batch(ids, BatchMode::DirectoryScan, |_| false);
    println!("扫描 {dir}：{} 个文件待处理，{} 个跳过", plan.jobs.len(), plan.skipped.len());
    for (name, why) in &plan.skipped {
        println!("  跳过 {name}：{why}");
    }

    let mut failed = 0usize;
    for job in &plan.jobs {
        // plan 保留了输入顺序，逐项对应回真实路径
        let path = found
            .iter()
            .find(|p| p.display().to_string() == job.source_id)
            .cloned()
            .unwrap_or_else(|| PathBuf::from(&job.source_id));
        let stem = path.file_name().and_then(|s| s.to_str()).unwrap_or("untitled.md").to_string();
        match process(registry, profile, &path, newline) {
            Ok(done) => {
                let diff = Diff::from_outcome(&done.outcome);
                if diff.content_changes != 0 {
                    eprintln!("✗ {stem} 正文变化非 0，放弃写入");
                    failed += 1;
                    continue;
                }
                let dir_of = path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
                let exists = |n: &str| dir_of.join(n).exists();
                let name = mdcore::io::derive_output_name(&stem, exists);
                let out = dir_of.join(&name);
                if let Err(e) = std::fs::write(&out, &done.bytes) {
                    eprintln!("✗ 写入 {} 失败：{e}", out.display());
                    failed += 1;
                    continue;
                }
                let m = manifest_for(&stem, &name, preset, &done.outcome);
                let _ = std::fs::write(out.with_extension("json"), m.to_json());
                println!(
                    "  ✓ {:<32} 结构 {} 处 / 正文变化 {}",
                    stem, diff.structural_changes, diff.content_changes
                );
            }
            Err(e) => {
                eprintln!("  ! {e}");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        eprintln!("批量处理存在 {failed} 项失败");
        return 1;
    }
    0
}

/// 内容不变性回归门：任何一份文件优化后正文 token 序列发生变化 => 退出码 1。
fn cmd_check(registry: &Registry, profile: &Profile, files: &[String]) -> u8 {
    let mut bad = 0usize;
    for f in files {
        let path = Path::new(f);
        let bytes = match read_capped(path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("✗ {e}");
                bad += 1;
                continue;
            }
        };
        let doc = match OpenDocument::from_bytes(f, &bytes, NewlineStyle::Preserve) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("✗ {f}: {e}");
                bad += 1;
                continue;
            }
        };
        match mdcore::Optimizer::new(registry, profile).run(&doc.body) {
            Ok(o) if o.guard.violations.is_empty() => {
                let idem = mdcore::Optimizer::new(registry, profile)
                    .run(&o.body)
                    .map(|s| !s.changed())
                    .unwrap_or(false);
                if !idem {
                    eprintln!("✗ {f}: 二次运行仍产生改动（未收敛）");
                    bad += 1;
                } else {
                    println!(
                        "✓ {f} 结构 {} 处 / 正文 0 / 评分 {}→{} / 轮次 {}",
                        o.edit_total(),
                        o.before.overall,
                        o.after.overall,
                        o.passes
                    );
                }
            }
            Ok(o) => {
                eprintln!("✗ {f}: 正文内容发生变化 {}", o.guard.violations.len());
                for v in o.guard.violations.iter().take(5) {
                    eprintln!("    {}", v.detail);
                }
                bad += 1;
            }
            Err(e) => {
                eprintln!("✗ {f}: {e}");
                bad += 1;
            }
        }
    }
    if bad == 0 {
        println!("全部通过：正文内容不变性 100%");
        0
    } else {
        eprintln!("{bad} 项未通过");
        1
    }
}

/// 离线 HTML 报告：证明「核心出数据、展示层只管渲染」（规格第七章 UI Independent）。
///
/// 只读原文、只在同目录新增 `*_阅读优化报告.html`，绝不改动或删除任何原文件；
/// 命名避让复用核心的 `derive_output_name` 思路（存在性由本机 fs 提供）。
fn cmd_report(
    registry: &Registry,
    profile: &Profile,
    files: &[String],
    newline: NewlineStyle,
    preset: &str,
) -> u8 {
    let mut paths: Vec<PathBuf> = Vec::new();
    for f in files {
        let p = Path::new(f);
        if p.is_dir() {
            walk(p, &mut paths);
        } else {
            paths.push(p.to_path_buf());
        }
    }
    paths.sort();
    if paths.is_empty() {
        eprintln!("report：未找到 Markdown 文件");
        return 2;
    }
    let mut bad = 0usize;
    for path in &paths {
        let done = match process(registry, profile, path, newline) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{e}");
                bad += 1;
                continue;
            }
        };
        // 内容不变性是报告的准入门：一旦非 0，宁可不出报告
        let diff = Diff::from_outcome(&done.outcome);
        if !diff.acceptable() {
            eprintln!("✗ {}: 正文内容变化 {}，拒绝出报告", path.display(), diff.content_changes);
            bad += 1;
            continue;
        }
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("untitled.md");
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        let exists = |n: &str| dir.join(n).exists();
        let html_name = {
            let first = format!("{stem}_阅读优化报告.html");
            if !exists(&first) {
                first
            } else {
                // 一直往后找空位：旧实现找满 9999 就返回未经检查的 `_x.html`，会覆盖已有报告
                let mut k = 2usize;
                loop {
                    let cand = format!("{stem}_阅读优化报告_{k}.html");
                    if !exists(&cand) {
                        break cand;
                    }
                    k += 1;
                }
            }
        };
        let html = report::render(name, preset, &done.outcome, &done.bytes);
        if let Err(e) = std::fs::write(dir.join(&html_name), html.as_bytes()) {
            eprintln!("写入失败：{e}");
            bad += 1;
            continue;
        }
        println!(
            "✓ {}（结构 {} 处 / 正文 0 / 评分 {}→{}）",
            dir.join(&html_name).display(),
            diff.structural_changes,
            done.outcome.before.overall,
            done.outcome.after.overall
        );
    }
    if bad > 0 {
        eprintln!("{bad} 项未生成报告");
        return 1;
    }
    0
}

/// 跨平台一致性探针（规格第四十章）。
///
/// 五平台各跑一次 `mdfile fingerprint <dir>`，把输出逐字节对比即可判定
/// 「同一文件是否得到同一 AST / 同一规则集 / 同一优化结果 / 同一 Guard 结论」。
/// 输出只含整数与稳定字符串，不含路径、时间、内存地址、locale，因此可直接 diff。
///
/// 预设来源（优先级从高到低）：`--preset` 开关 → 环境变量 `MDFILE_PRESET` → 默认「通用文章」。
/// 旧实现只读环境变量且忽略 `--preset`，于是「用技术文档跑一致性对账」会静默按默认预设出结果。
fn cmd_fingerprint(files: &[String], preset_flag: &str, preset_from_flag: bool) -> u8 {
    let registry = Registry::new();
    let preset_name = if preset_from_flag {
        preset_flag.to_string()
    } else {
        std::env::var("MDFILE_PRESET").unwrap_or_else(|_| preset_flag.to_string())
    };
    let profile = match presets::profile_for(&preset_name) {
        Some(p) => p,
        None => {
            eprintln!("预设「{preset_name}」无效，可选：{}", presets::PRESETS.join(" / "));
            return 2;
        }
    };
    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    for f in files {
        let p = std::path::Path::new(f);
        if p.is_dir() {
            walk(p, &mut paths);
        } else {
            paths.push(p.to_path_buf());
        }
    }
    paths.sort();
    // v0.1.4：一份文件都没找到时旧实现只打表头、退出码 0——
    // CI 的对账脚本会把「什么都没跑」当成通过（与 691-697 行修过的空跑陷阱同类）。
    if paths.is_empty() {
        eprintln!("fingerprint：未找到 Markdown 文件");
        return 2;
    }
    println!("# mdfile fingerprint  engine={}", mdcore::ENGINE_VERSION);
    println!("# 列：文件 | 输入正文指纹 | 优化后指纹 | 结构变化 | 正文变化 | 评分(综/标/段/列/空/密) | 规则链");
    let mut bad = 0usize;
    for path in &paths {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else { continue };
        let bytes = match read_capped(path) {
            Ok(b) => b,
            Err(e) => {
                println!("{name} READ_ERROR {e}");
                bad += 1;
                continue;
            }
        };
        let doc = match OpenDocument::from_bytes(name, &bytes, NewlineStyle::Preserve) {
            Ok(d) => d,
            Err(e) => {
                // 被拒绝的文件同样是「没有结论」，必须计入失败并让退出码非 0，
                // 否则对账脚本会把「一份都没跑成」当成通过。
                println!("{name} REJECT {e}");
                bad += 1;
                continue;
            }
        };
        let outcome = match mdcore::Optimizer::new(&registry, &profile).run(&doc.body) {
            Ok(o) => o,
            Err(e) => {
                println!("{name} GUARD_FAIL {e}");
                bad += 1;
                continue;
            }
        };
        let fpi = fp_of(&doc.body);
        let fpo = fp_of(&outcome.body);
        let rules: Vec<&str> = outcome.applied.iter().map(|a| a.rule_id.as_str()).collect();
        let r = &outcome.after;
        println!(
            "{name} {fpi} {fpo} {} {} {}/{}/{}/{}/{}/{} {}",
            outcome.edit_total(),
            outcome.guard.violations.len(),
            r.overall,
            r.heading.value,
            r.paragraph.value,
            r.list.value,
            r.whitespace.value,
            r.density.value,
            if rules.is_empty() { "-".to_string() } else { rules.join("+") }
        );
    }
    if bad > 0 {
        eprintln!("{bad} 项异常");
        return 1;
    }
    0
}

/// 内容指纹：token 序列校验和 + 各类 token 计数 + 反斜杠数 + 字节长度，全整数。
fn fp_of(body: &str) -> String {
    let fp = mdcore::fingerprint::tokenize(body);
    format!(
        "{:016x}:w{}p{}c{}u{}h{}r{}b{}n{}",
        fp.checksum,
        fp.counts.words,
        fp.counts.puncts,
        fp.counts.code_bytes,
        fp.counts.urls,
        fp.counts.html_bytes,
        fp.counts.refs,
        fp.backslashes,
        body.len()
    )
}

/// 递归扫描 .md/.markdown，保持深度优先 + 名称排序（确定性）。
///
/// v0.1.4 的两处加固：
/// - 用 `symlink_metadata` 判型，**不跟随** junction / 符号链接目录——旧实现的
///   `p.is_dir()` 会跟随 reparse point，目录里一个指向祖先目录的链接就足以
///   无界递归直到栈溢出。指向文件的链接不受影响（仍按扩展名收录）。
/// - `read_dir` 失败不再静默吞掉：无权限的子目录以前会从 batch/report 里无声消失。
fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("警告：无法读取目录 {}（{e}），其中文件未被处理", dir.display());
            return;
        }
    };
    let mut names: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    names.sort();
    for p in names {
        let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
        if meta.is_dir() {
            let Some(n) = p.file_name().and_then(|s| s.to_str()) else { continue };
            // 不进入自身输出目录与常见版本控制目录
            if n == ".git" || n == "target" || n == "node_modules" {
                continue;
            }
            walk(&p, out);
        } else if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
            let lower = ext.to_lowercase();
            if lower == "md" || lower == "markdown" {
                out.push(p);
            }
        }
    }
}

/// 把 `--rule` 的写法解析成唯一规则 ID（支持 `R01` 简写）。
fn resolve_rule_id(registry: &Registry, raw: &str) -> Result<String, String> {
    if registry.find(raw).is_some() {
        return Ok(raw.to_string());
    }
    let prefix = format!("{raw}.");
    let mut hits: Vec<String> = registry
        .ids()
        .iter()
        .filter(|id| id.starts_with(&prefix))
        .map(|id| id.to_string())
        .collect();
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => Err(format!("没有规则「{raw}」，用 `mdfile rules` 查看全部 ID")),
        _ => Err(format!("规则「{raw}」有歧义：{}", hits.join(" / "))),
    }
}

#[cfg(test)]
mod cli_tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_and_help_do_not_panic() {
        // 回归：旧实现 `mdfile` / `mdfile --help` 都会 panic（exit 101）
        for case in [v(&[]), v(&["--help"]), v(&["-h"]), v(&["--stdout"]), v(&["--newline", "lf"])] {
            let a = parse_args(&case).unwrap_or_else(|e| panic!("{case:?} 应可解析：{e}"));
            assert!(a.help || a.cmd == "help", "{case:?} 必须落到 help");
            assert!(a.files.is_empty(), "{case:?} 不应凭空多出文件");
        }
    }

    #[test]
    fn flag_values_are_consumed_with_their_flag() {
        let a = parse_args(&v(&["optimize", "a.md", "--preset", "技术文档", "--newline", "crlf"])).unwrap();
        assert_eq!(a.cmd, "optimize");
        assert_eq!(a.files, vec!["a.md".to_string()], "开关值不得变成待处理文件");
        assert_eq!(a.preset, "技术文档");
        assert_eq!(a.newline, NS::CrLf);
    }

    #[test]
    fn rule_flag_and_equals_form() {
        let a = parse_args(&v(&["preview", "a.md", "--rule", "R01.blank-lines"])).unwrap();
        assert_eq!(a.rule.as_deref(), Some("R01.blank-lines"));
        assert_eq!(a.files, vec!["a.md".to_string()]);
        let b = parse_args(&v(&["check", "--preset=小说", "b.md"])).unwrap();
        assert_eq!(b.preset, "小说");
        assert_eq!(b.files, vec!["b.md".to_string()]);
    }

    #[test]
    fn bad_flags_are_rejected_with_code_2_message() {
        assert!(parse_args(&v(&["check", "--newline", "bogus"])).is_err(), "非法 newline 值要报错");
        assert!(parse_args(&v(&["check", "--preset"])).is_err(), "开关缺值要报错");
        assert!(parse_args(&v(&["check", "--peset", "技术文档"])).is_err(), "拼错的开关不得被当成文件");
    }

    #[test]
    fn rule_id_shortform_resolves() {
        let reg = Registry::new();
        assert_eq!(resolve_rule_id(&reg, "R01").unwrap(), "R01.blank-lines");
        assert_eq!(resolve_rule_id(&reg, "R08.table-delimiter").unwrap(), "R08.table-delimiter");
        assert!(resolve_rule_id(&reg, "R99").is_err(), "不存在的 ID 要说清楚");
    }

    #[test]
    fn cjk_filenames_stay_positional() {
        let a = parse_args(&v(&["check", "中文 文档.md"])).unwrap();
        assert_eq!(a.files, vec!["中文 文档.md".to_string()]);
    }

    #[test]
    fn preset_from_flag_is_detected_in_both_forms() {
        // 回归（v0.1.4）：`--preset=小说` 等号形态此前检测不到，fingerprint 的
        // 对账会静默落到环境变量或默认预设上出结果。
        let space = parse_args(&v(&["fingerprint", "corpus", "--preset", "小说"])).unwrap();
        assert!(space.preset_from_flag, "空格形态必须算显式指定");
        let eq = parse_args(&v(&["fingerprint", "corpus", "--preset=小说"])).unwrap();
        assert!(eq.preset_from_flag, "等号形态必须算显式指定");
        let none = parse_args(&v(&["fingerprint", "corpus"])).unwrap();
        assert!(!none.preset_from_flag, "没给开关时不算显式指定");
    }

    #[test]
    fn newline_acceptance_is_the_same_in_both_forms() {
        // 回归（v0.1.4）：`--newline preserve` 空格形态此前报错、等号形态却收下。
        for form in [v(&["check", "a.md", "--newline", "preserve"]), v(&["check", "--newline=preserve", "a.md"])] {
            let a = parse_args(&form).unwrap();
            assert!(matches!(a.newline, NS::Preserve), "{form:?}");
        }
        // keep 仍然两种形态都合法
        assert!(parse_args(&v(&["check", "a.md", "--newline", "keep"])).is_ok());
        // 拼错的值两种形态都必须报错
        assert!(parse_args(&v(&["check", "a.md", "--newline", "clrf"])).is_err());
        assert!(parse_args(&v(&["check", "--newline=clrf", "a.md"])).is_err());
    }
}

