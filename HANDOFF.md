# 从云端到本机：落地交接说明

## 1. 现在手上有什么（已实测）

| 产物 | 内容 | 状态 |
| --- | --- | --- |
| `mdfile-assistant.zip` | 完整源码：mdcore（共享核心）+ mdcore-ffi（C ABI）+ mdfile-cli（无头前端）+ 33 份真实语料 + 3 份设计文档 + README | 可直接 `cargo build` |
| `mdfile-cli-linux-x86_64` | Linux x86_64 动态链接可执行文件（依赖 glibc 2.35 / libgcc） | 可直接运行 |
| `示例-阅读优化报告.html` | 对真实 README 生成的离线报告（无脚本、无外链） | 可直接打开 |
| `fingerprint-linux-baseline.txt` | 五平台一致性对账基线（本机跑出的指纹与此逐字节比对） | 用于验收 |

沙箱内验证结果（这些是我亲眼跑出来的，不是计划）：

```text
cargo test                 → 156 项全绿（mdcore 134 / 架构守卫 3 / 语料金样 4 / FFI 11 / 报告 4）
mdfile-cli check corpus/*  → 33/33 全部通过：正文内容不变性 100%
C 客户端链接 libmdcore_ffi → 状态码、错误路径、BOM/CRLF 逐字节还原均正确
LC_ALL / RUST_MIN_STACK / 4 线程并发 → 金样 GOLDEN.csv 完全一致
```

## 2. 放到本机的三条路

**A. zip（最简单，已经做好）**
解压后：

```bash
cd mdfile-assistant
cargo build --release
cargo test
./target/release/mdfile-cli check tests/corpus/*.md
# 与云端基线对账（应逐字节相同）
./target/release/mdfile-cli fingerprint tests/corpus > /tmp/mine.txt
diff /tmp/mine.txt ../dist/fingerprint-linux-baseline.txt
```

**B. 交给你的 git 远端（我这边没有配置任何 remote，需要你先建仓库）**
你需要提供仓库地址与目标分支，并确认允许推送；然后本地执行：

```bash
cd mdfile-assistant
git init && git add -A && git commit -m "MD 文件助手：共享核心 + C ABI + 无头 CLI"
git remote add origin <你的仓库地址>
git push -u origin main
```

**C. 直接替换现有工程的 core 目录**
`crates/mdcore` 是唯一真相源，UI 工程只依赖 `mdcore-ffi` 的 `include/mdfile.h`。

## 3. 为什么另外四个平台的可执行文件必须你自己在本地构建

沙箱缺失的东西（我试过，不是没做）：

- `rustup target add` 对 windows/macos/android/ios/musl 目标**全部失败**：静态站镜像返回 404，
  覆盖 `RUSTUP_DIST_SERVER` 也无效 → 没有交叉 std 可用。
- 没有 Android SDK/NDK、没有 Xcode/macOS、没有 WebKitGTK/GTK 开发包 → 装不了 Tauri/Flutter/GTK 壳。

替代验收手段已经内置，你在本机第一次构建后**只需跑两条命令**就能证明跨平台一致：

```bash
cargo test                                # 含 architecture_guard：核心不允许出现任何平台 API
mdfile-cli fingerprint tests/corpus       # 输出与基线逐字节对比
```

`fingerprint` 输出只含整数与稳定字符串（无路径、无时间、无地址、无 locale），所以五个平台的
结果可以直接 `diff`。这就是规格第四十章「五平台同一结果」的落地形式。

## 4. 建议的本机构建顺序（对应规格第九阶段）

1. 本机 `cargo build --release` 跑通 + `check`/`fingerprint` 对账（先确认核心一致）。
2. Windows 桌面壳：Tauri v2（MSVC + WebView2 自带）调用 `mdcore-ffi.dll`；
   若坚持原生，则 WPF/WinUI + C# P/Invoke 同一份 `mdfile.h`。
3. macOS：SwiftPM 包引用 `libmdfile_ffi.a` + `mdfile.h`（module map），SwiftUI 壳。
4. Linux：同一 `.so`，GTK4 或复用 Tauri。
5. Android：`cargo ndk` 出 `.so`，Kotlin/JNI 薄封装；Compose 壳。
6. iOS：`cargo build --target aarch64-apple-ios{,-sim}` + `xcodebuild -create-xcframework`，Swift 壳。
   具体命令见 `README.md` 第 6 节。

UI 层的铁律：只调 `mdfile.h` 里的入口，拿到结构化 JSON（评分、Diff 分类计数、规则链、保护区）
再渲染。文件读写、路径拼接、 SAF/权限对话框属于平台层；核心里出现 `std::fs` 会被
`architecture_guard` 测试直接判失败。

## 5. 需要你决策的一件事

规格第十九章（隐式并列 → 编号列表）和第二十章（冒号字段 → 表格）按原文实现必然增删正文的
数字与标点，和第四章「内容不可变」硬冲突。当前按第五十一章收尾规则做了**零改字替代**
（R11 只分行、不编号；表格只规范化已有分隔行）。

请二选一：
- 保持现状：严守「只改读法不改内容」，这两条永久降级为安全等价物。
- 放宽内容不变：允许插入编号/删除 `：`，那我就实现原始形态，但 Diff 必须显示「正文内容变化 > 0」，
  产品文案也要改（不再是「内容不变」）。

细节与证据在 `README.md` 第 4 节、`docs/ARCHITECTURE.md`。
