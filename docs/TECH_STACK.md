# TECH_STACK.md — 技术栈调研与选型结论

> 调研日期：2026-10-05  
> 目标：为「MD文档阅读助手」五平台（Windows/macOS/Linux/Android/iOS）共享核心 + 平台无关 UI 选型

---

## 一、Rust Core 解析器选型

### 1.1 候选库对比

| 维度 | pulldown-cmark | comrak | markdown-rs |
|------|---------------|--------|-------------|
| **最新版本** | 0.13.0 (2025) | 0.43 (2025-2026) | 1.x (活跃开发) |
| **CommonMark 合规** | 0.31 全通过 | 0.31.2 全通过 | 全通过 |
| **解析模式** | 拉取式事件流 (无 AST) | 构建 AST | 构建 AST |
| **GFM 表格** | ✅ `ENABLE_TABLES` | ✅ 完整 | ✅ |
| **GFM 任务列表** | ✅ `ENABLE_TASKLISTS` | ✅ | ✅ |
| **GFM 删除线** | ✅ `ENABLE_STRIKETHROUGH` | ✅ | ✅ |
| **Footnote** | ✅ `ENABLE_FOOTNOTES` | ✅ | ✅ |
| **数学公式** | ❌ 不支持 | ✅ `math-dollars` / `math-code` | ✅ |
| **Front matter** | ❌ 不支持 | ✅ `front-matter-delimiter` | ✅ |
| **WikiLinks** | ✅ 0.13 新增 | ✅ | ❌ |
| **上下标** | ✅ 0.13 新增 | ✅ | ❌ |
| **定义列表** | ✅ 0.13 新增 | ✅ | ❌ |
| **MDX** | ❌ | ❌ | ✅ |
| **Source map** | ❌ | ❌ | ✅ 完整 |
| **性能** | ⭐⭐⭐⭐⭐ 最快 | ⭐⭐⭐⭐ 快（构建 AST 有开销） | ⭐⭐⭐ 中等 |
| **SIMD 加速** | ✅ `features = ["simd"]` | ❌ | ❌ |
| **维护方** | Raph Levien (Google) | kivikakk (社区) | wooorm (社区) |
| **使用者** | rustdoc, mdBook, Zola | Ruby commonmarker, 多语言绑定 | — |

**来源：**
- pulldown-cmark: <https://crates.io/crates/pulldown-cmark>、<https://github.com/pulldown-cmark/pulldown-cmark>
- comrak: <https://crates.io/crates/comrak>、<https://github.com/kivikakk/comrak>
- markdown-rs: <https://crates.io/crates/markdown>、<https://github.com/wooorm/markdown-rs>
- 性能基准: <https://github.com/timan-z/Markdown-Parser-Performance-Benchmark>

### 1.2 选型结论：pulldown-cmark 0.13 为主解析器

**理由：**
1. **性能最优**：拉取式事件流设计，零分配，SIMD 加速，比 comrak 快 2-3x
2. **0.13 版本补齐扩展**：新增上下标、WikiLinks、定义列表，覆盖需求大幅提升
3. **生态成熟**：rustdoc / mdBook / Zola 等核心 Rust 项目均采用
4. **CommonMark 0.31 全合规**

**补充方案：** 对于 pulldown-cmark 不支持的扩展（math、front matter），在 Core 层自行实现预处理或后处理：
- Front matter：在解析前用 `gray_matter` crate 或手写 YAML 头提取
- Math：在事件流中识别 `$...$` 和 `$$...$$` 并生成自定义事件

**comrak 作为备选：** 若后续需要完整 GFM 渲染一致性（如与 GitHub 渲染结果完全对齐），可引入 comrak 作为对照验证。

---

## 二、UI 方案实测评估

### 2.1 候选方案对比

| 方案 | Win | Mac | Linux | Android | iOS | Rust 集成 | 成熟度 |
|------|:---:|:---:|:-----:|:-------:|:---:|-----------|--------|
| **Tauri v2** | ✅ | ✅ | ✅ | ✅ | ✅ | 原生 Rust 后端 | ⭐⭐⭐⭐ |
| **Flutter + FRB** | ✅ | ✅ | ✅ | ✅ | ✅ | flutter_rust_bridge | ⭐⭐⭐⭐ |
| **Avalonia** | ✅ | ✅ | ✅ | ⚠️ 实验 | ⚠️ 实验 | 需 .NET，FFI 桥接 | ⭐⭐⭐ |
| **Qt (C++)** | ✅ | ✅ | ✅ | ✅ | ✅ | cxx/autocxx 桥接 | ⭐⭐⭐⭐ |
| **KMP** | ✅ | ✅ | ⚠️ 桌面Beta | ✅ | ✅ | 不原生支持 Rust | ⭐⭐⭐ |
| **React Native** | ✅ | ✅ | ⚠️ 社区 | ✅ | ✅ | 需 Native Module | ⭐⭐⭐ |

### 2.2 Tauri v2 详细评估

**当前状态：**
- 稳定版 2.11.5（2026 年），crates.io 可查
- iOS/Android 支持自 2.0 stable (2024-10) 起正式可用
- 来源：<https://v2.tauri.app/blog/tauri-2-0-0-beta/>、<https://www.oschina.net/news/315100/tauri-2-0-stable>

**Linux 构建依赖：**
- `libwebkit2gtk-4.1-dev`（Tauri v2 要求 4.1，非旧版 4.0）
- `build-essential`, `curl`, `wget`, `file`, `libxdo-dev`, `libssl-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
- 需 Ubuntu 22.04+（20.04 的 GLIBC 版本过低）
- 来源：<https://v2.tauri.app/start/prerequisites/>

**无 GUI 沙箱限制：**
- ❌ 无法运行 `tauri dev`（需要 display server）
- ✅ 可以 `cargo build` 编译 Rust 后端（不链接 webkit）
- ✅ 可以 `cargo check` / `cargo test` 核心逻辑
- ❌ 无法构建最终安装包（需要 webkit2gtk 链接）
- ❌ 无法构建 Android/iOS（需要 SDK）

**Rust 核心集成：**
- Tauri 后端本身就是 Rust，核心库可直接作为 crate 依赖
- 无需 FFI 桥接，零开销调用
- 前端用 JS/TS + 任意框架（Vue/React/Svelte）

**风险：**
- 移动端支持虽已稳定，但社区反馈仍有兼容性边缘 case
- Linux WebKitGTK 版本碎片化（不同发行版 WebKit 版本不同，CSS/JS 新特性支持滞后）
- 来源：<https://juejin.cn/post/7420980084361625600>

### 2.3 Flutter + flutter_rust_bridge 详细评估

**当前状态：**
- flutter_rust_bridge v2.12.0，官方 Flutter Favorite
- 支持 Flutter 3.44+（2026 年）
- 来源：<https://pub.dev/packages/flutter_rust_bridge>、<https://github.com/fzyzcjy/flutter_rust_bridge>

**五平台覆盖：**
- ✅ Android / iOS / Windows / macOS / Linux / Web
- Linux 桌面构建需要：`libgtk-3-dev`, `libblkid-dev`, `liblzma-dev` 等系统依赖
- 来源：<https://docs.flutter.cn/install/archive>

**Rust 集成（flutter_rust_bridge v2）：**
- 自动代码生成：Rust fn → Dart 调用绑定
- 支持 async/sync、Stream、任意类型（含 non-Clone、non-Serializable）
- 支持 Rust → Dart 回调
- 支持 trait objects、lifetime
- 来源：<https://github.com/fzyzcjy/flutter_rust_bridge>

**无 GUI 沙箱限制：**
- ❌ 无法 `flutter run`（需要 display / emulator）
- ✅ 可以 `flutter build` 部分目标（如 web）
- ✅ 可以 `dart test` 纯逻辑测试
- ❌ 无法编译 Linux 桌面（需 GTK 头文件）
- ❌ 无法编译 Android/iOS（需 SDK）

**风险：**
- flutter_rust_bridge 代码生成增加构建复杂度
- Flutter 自绘 UI 在桌面端缺乏原生感（与系统控件不一致）
- Linux 桌面 Flutter 构建依赖较多

### 2.4 其他方案简述

**Avalonia：** .NET 生态，与 Rust Core 需跨语言 FFI，增加复杂度。移动端仍实验性。不推荐。

**Qt (C++)：** 成熟但 C++ 生态与 Rust 桥接（cxx/autocxx）增加维护成本。许可证问题（LGPL/商业）。

**Kotlin Multiplatform：** 桌面端仍在 Beta，不原生支持 Rust。不推荐。

**React Native：** 桌面端支持弱（依赖社区方案），与 Rust 集成需 Native Module。不推荐。

### 2.5 UI 方案推荐结论

| 优先级 | 方案 | 理由 |
|--------|------|------|
| **首选** | **Tauri v2** | Rust 原生后端零 FFI、包体积极小(~6MB)、五平台正式支持、Web 前端生态丰富 |
| **备选** | **Flutter + FRB** | 自绘 UI 跨平台一致性好、移动端成熟、但 FFI 桥接增加复杂度 |

**推荐 Tauri v2 的核心理由：**
1. Rust 后端 = 零 FFI 开销，核心 crate 直接 `use`
2. 包体积最小（利用系统 WebView，无捆绑渲染引擎）
3. 安全模型成熟（capabilities 权限系统，经独立安全审计）
4. 前端可自由选择 Vue/React/Svelte，生态灵活

---

## 三、当前 CI 沙箱可交付性评估

### 3.1 可交付（无 GUI / 无移动端 SDK）

| 交付物 | 可行性 | 说明 |
|--------|:------:|------|
| Rust Core 引擎全部代码 | ✅ | `cargo build/test` 纯 Rust |
| Core 单元测试 + corpus 集成测试 | ✅ | 32 个测试文件已就绪 |
| Core FFI 边界 C ABI 定义 | ✅ | 纯 Rust `extern "C"` |
| Tauri 项目脚手架 | ✅ | `cargo create-tauri-app` + 配置 |
| Tauri Rust 后端代码 | ✅ | 纯 Rust，可编译检查 |
| Tauri 前端代码 (HTML/JS/CSS) | ✅ | 纯文本，无需构建 |
| Flutter 项目脚手架 | ✅ | 可生成项目结构 |
| Flutter Rust 绑定定义 | ✅ | Rust 侧 API 可写 |
| CI 脚本 (GitHub Actions) | ✅ | 纯文本 |
| 全部文档 | ✅ | — |

### 3.2 不可交付

| 交付物 | 原因 |
|--------|------|
| Tauri Linux 桌面安装包 | 缺 webkit2gtk-4.1 |
| Tauri Android APK | 缺 Android SDK / NDK |
| Tauri iOS IPA | 缺 Xcode / macOS |
| Flutter Linux 桌面包 | 缺 GTK 开发库 |
| Flutter Android/iOS 包 | 缺 SDK |
| 任何 GUI 运行时测试 | 缺 display server |

---

## 四、风险总结

| 风险 | 等级 | 缓解措施 |
|------|:----:|----------|
| Tauri 移动端兼容性边缘 case | 中 | 早期真机测试、关注 Tauri 更新 |
| Linux WebKitGTK 版本碎片化 | 中 | 设定最低支持版本（2.36+），CI 覆盖 Ubuntu 22.04/24.04 |
| pulldown-cmark 不支持 math/front matter | 低 | Core 层预处理，或后续版本可能支持 |
| flutter_rust_bridge 代码生成复杂度 | 低 | 若选 Tauri 则不涉及 |
| 五平台输出一致性（Unicode/换行） | 高 | 见 DETERMINISM.md |
