//! 只在 Windows 目标上生效：把应用图标与版本信息编进 exe。
//!
//! 做法：`resources/mdfile.rc` 是一份模板，里面 `@ICON@` 由本脚本换成**绝对路径**后再编译 ——
//! windres 按当前目录解析相对路径、rc.exe 按 .rc 文件所在目录解析，两边不一致，
//! 用绝对路径就不必猜。
//!
//! 为什么不引 embed-resource：workspace 现在零构建期依赖，而这件事只需要一个资源编译器
//! （GNU 侧 windres、MSVC 侧 rc.exe），两者都在工具链里现成存在。
//! 编译器缺失或编译失败时只报 warning、绝不让构建失败 —— 图标缺失是可接受的降级。
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=resources/mdfile.rc");
    println!("cargo:rerun-if-changed=../../assets/brand/app.ico");
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_else(|_| ".".into()));
    let template = manifest.join("resources").join("mdfile.rc");
    let icon = manifest.join("../../assets/brand/app.ico");
    let icon = match icon.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            println!("cargo:warning=找不到应用图标 {}（{e}），跳过资源嵌入", icon.display());
            return;
        }
    };
    let Ok(raw) = std::fs::read_to_string(&template) else {
        println!("cargo:warning=找不到资源脚本 {}", template.display());
        return;
    };
    // .rc 里的字符串必须用反斜杠且做 C 转义
    let icon_rc = icon.display().to_string().replace('\\', "\\\\");
    let rc_path = out.join("mdfile.rc");
    if let Err(e) = std::fs::write(&rc_path, raw.replace("@MDFILE_ICON@", &icon_rc)) {
        println!("cargo:warning=写出预处理后的 .rc 失败：{e}");
        return;
    }

    let gnu = target.contains("gnu");
    // Debian/MinGW 的 windres 带的是 *工具链* 三重名（x86_64-w64-mingw32-windres），
    // 和 rustc 的 target 三元组（x86_64-pc-windows-gnu）不一样，所以按候选顺序全试一遍；
    // 也允许用 WINDRES / RC 环境变量直接指定。
    let mut tools: Vec<String> = Vec::new();
    if gnu {
        if let Ok(v) = std::env::var("WINDRES") { tools.push(v); }
        tools.push("windres".into());
        tools.push(format!("{target}-windres"));
        tools.push("x86_64-w64-mingw32-windres".into());
        tools.push("i686-w64-mingw32-windres".into());
        tools.push("aarch64-w64-mingw32-windres".into());
    } else {
        if let Ok(v) = std::env::var("RC") { tools.push(v); }
        tools.push("rc.exe".into());
        tools.push("rc".into());
        tools.push("llvm-rc".into());
    }
    tools.retain(|t| !t.is_empty());
    let artifact = if gnu { "mdfile-res.o" } else { "mdfile-res.lib" };
    let obj = out.join(artifact);

    let mut last = String::from("没有可用的资源编译器");
    for tool in &tools {
        let mut cmd = Command::new(tool);
        if gnu {
            cmd.arg("-i").arg(&rc_path).arg("-o").arg(&obj).args(["-J", "rc", "-O", "coff"]);
        } else {
            cmd.arg("/nologo").arg("/fo").arg(&obj).arg(&rc_path);
        }
        match cmd.output() {
            Ok(o) if o.status.success() => {
                link(&obj, &target);
                println!("cargo:rustc-link-arg={}", obj.display());
                return;
            }
            Ok(o) => {
                let msg = String::from_utf8_lossy(&o.stderr);
                last = format!("{tool} 编译资源失败：{}", msg.lines().next().unwrap_or("未知错误"));
            }
            Err(e) => last = format!("找不到 {tool}：{e}"),
        }
    }
    println!("cargo:warning={last}；图标与版本信息未嵌入（不影响功能），装好 Windows 工具链后自动生效");
}

/// MSVC 侧 rc.exe 产出的是 .lib，需要额外告诉链接器别把它当成普通归档；
/// GNU 侧 .o 直接作为链接参数传入即可。
fn link(obj: &Path, target: &str) {
    if target.contains("msvc") {
        println!("cargo:rustc-link-lib=static={}", "mdfile-res");
        if let Some(dir) = obj.parent() {
            println!("cargo:rustc-link-search=native={}", dir.display());
        }
    }
}
