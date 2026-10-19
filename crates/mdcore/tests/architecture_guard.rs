//! 架构守卫测试（规格第六、七、十三章的可执行版本）。
//!
//! 「Core 不得调用任何平台 API」如果只写在文档里，就一定会有人在某个规则里顺手
//! `std::fs::read` 或 `#[cfg(windows)]`。这条测试把纪律变成会失败的断言，
//! 五平台 CI 上都能跑，因为它检查的是**源码文本与本 crate 的依赖清单**。

use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("src 目录必须存在") {
        let p = e.expect("dir entry").path();
        if p.is_dir() {
            md_files(&p, out);
        } else if p.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

/// 核心源码中禁止出现的平台 / UI / 网络标识符。
///
/// 注意：文档注释里会出现这些词（例如「不调用 Win32」），因此只检查**代码行**：
/// 逐行剥掉行注释与字符串字面量后再匹配。
const FORBIDDEN: &[&str] = &[
    "win32",
    "windows_sys",
    "windows-rs",
    "wpf",
    "winui",
    "appkit",
    "cocoa",
    "uikit",
    "foundation",
    "android_sdk",
    "jni",
    "ndk",
    "gtk",
    "webkit",
    "qt",
    "avalonia",
    "flutter",
    "dart",
    "tauri",
    "wasm_bindgen",
    "react_native",
];

/// 核心源码中禁止出现的调用与属性。
///
/// 比对时源码会被转小写（UI 框架名大小写不一），因此**这里的模式必须全小写**，
/// 否则是死规则：旧列表里的 `SystemTime` / `Instant::now` / `File::open` / `File::create` /
/// `Command::new` 永远匹配不上，测试却一路绿灯。
const FORBIDDEN_CODE: &[&str] = &[
    "std::fs",
    "std::net",
    "std::env",
    // 花括号合并写法（`use std::{fs, env};` 不含 "std::fs" 这个子串，旧列表漏得干干净净）
    "std::{fs",
    "std::{net",
    "std::{env",
    "std::{io",
    "std::{process",
    "std::{time",
    "std::io",
    "std::process",
    "std::time",
    "file::open",
    "file::create",
    "read_dir",
    "reqwest",
    "libc::",
    "#[cfg(target_os",
    "#[cfg(windows",
    "#[cfg(unix)",
    "#[cfg(all(target_os",
    "command::new",
    "systemtime",
    "unix_epoch",
    "instant::now",
    "thread::sleep",
    "locale",
    // 确定性纪律：输出顺序不得依赖哈希表迭代顺序，也不得引入随机数
    "hashmap",
    "hashset",
    "btreemap::new_random",
    "rand::",
    "rand_",
    "chrono",
    "uuid",
];

fn strip_noise(line: &str) -> String {
    // 去行注释
    let code = match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    };
    // 去字符串字面量（够用即可：核心不该有指向平台的长字符串）
    let mut out = String::new();
    let mut in_str = false;
    let mut prev = '\0';
    for ch in code.chars() {
        if ch == '"' && prev != '\\' {
            in_str = !in_str;
            prev = ch;
            continue;
        }
        if !in_str {
            out.push(ch);
        }
        prev = ch;
    }
    out
}

#[test]
fn forbidden_patterns_are_matchable() {
    // 守卫守卫：模式里只要有大写，它在被转小写的源码上永远是死规则。
    for pat in FORBIDDEN_CODE.iter().chain(FORBIDDEN.iter()) {
        assert_eq!(
            pat.to_lowercase(),
            *pat,
            "禁止模式必须全小写（源码比对前会被 to_lowercase）：{pat}"
        );
    }
    // 自检：确实能抓到违规写法，而不是空跑
    let fake = "    let _ = std::fs::read(\"x\"); let t = SystemTime::now();".to_string();
    let lower = strip_noise(&fake).to_lowercase();
    assert!(lower.contains("std::fs") && lower.contains("systemtime"), "扫描逻辑本身失效了");
}

#[test]
fn core_calls_no_platform_api() {
    let mut files = Vec::new();
    md_files(&src_dir(), &mut files);
    assert!(files.len() >= 10, "核心模块数量异常：{}", files.len());
    let mut hits: Vec<String> = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for (lineno, line) in text.lines().enumerate() {
            // 先剥注释与字符串，再判断是不是测试模块起始行：
            // 旧实现直接在原始文本上找 "#[cfg(test)]"，于是一行文档注释
            // `//! 见 #[cfg(test)]` 就能把它后面的整份源码从扫描里抹掉。
            let stripped = strip_noise(line);
            if stripped.contains("#[cfg(test)]") {
                break;
            }
            let lower = stripped.to_lowercase();
            for pat in FORBIDDEN.iter().chain(FORBIDDEN_CODE.iter()) {
                if lower.contains(pat) {
                    hits.push(format!("{}:{}: {pat}", f.file_name().unwrap().to_string_lossy(), lineno + 1));
                }
            }
        }
    }
    assert!(hits.is_empty(), "核心出现平台/UI/网络依赖：\n{}", hits.join("\n"));
}

#[test]
fn core_has_only_parser_and_optional_serialization_deps() {
    let manifest = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let deps = manifest
        .split("[dev-dependencies]")
        .next()
        .unwrap()
        .split("[dependencies]")
        .nth(1)
        .expect("必须有 [dependencies]");
    let mut names: Vec<String> = Vec::new();
    for line in deps.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with('[') {
            continue;
        }
        if let Some((k, _)) = l.split_once('=') {
            names.push(k.trim().to_string());
        }
    }
    names.sort();
    assert_eq!(names, vec!["pulldown-cmark", "serde", "serde_json"], "核心依赖必须只有解析器与可选序列化");
    // serde 只能是 optional
    assert!(manifest.contains("serde = { workspace = true, optional = true }"));
    assert!(manifest.contains("serde_json = { workspace = true, optional = true }"));
}

#[test]
fn core_crate_type_allows_headless_and_ffi() {
    let manifest = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    assert!(manifest.contains("crate-type"), "必须显式声明 crate-type，否则各平台链接方式会漂移");
}
