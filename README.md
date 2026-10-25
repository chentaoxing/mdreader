<img src="assets/brand/icon-tile.png" alt="MD文档阅读助手" width="96" height="96" />

# MD文档阅读助手

**今天大量的 `.md` 是写给 AI 读的，但拍板、核定、审阅的人还是人。**
而 Markdown 的排版和格式，是按书写方便长出来的，不是按人的眼睛长出来的——同一份文件，机器读着没问题，人读着累。

这个小软件按人一贯的阅读习惯重新呈现它：**一个字的内容都不改**，只做三件事——

- **调排版结构**：空行节奏、标题层级、列表符号、表格分隔线、长并列句的断行
- **收掉无效阅读字符**：标题尾部多余的 `##`、混着用的 `-` / `*` / `+`、`***` 与 `___` 混排、行尾看不见却占位的空格
- **阅读时把语法符号从屏幕上拿掉**：`**加粗**` 直接以粗体呈现，`#`、`` ` ``、`[ ]( )` 淡到几乎看不见或干脆不上屏——
  这一步只发生在显示层，文件里一个字节都没少

把它设为 `.md` 的默认打开方式，你读到的是你自己的阅读习惯，不是模型的书写习惯。

而且它不需要 AI。**排版与阅读全程离线**：不联网就能用，不上传任何内容、无账号、无 API Key、不埋点。
唯一会出网的是「检查更新」——只向 GitHub 问一次"最新版是几点"，不带你机器上的任何东西；
顶栏取消勾选「启动时检查」之后，就只有你点「检查更新」那一下才会联网。
排版是确定性的事，所以结果可复现——同一份文件在你机器上和在别人机器上逐字节相同。

---

## 它做什么

| 能力 | 说明 |
| --- | --- |
| **调排版结构** | 压缩成片空行、补齐标题前后空行、下划线式标题统一成 `#`、引用标记后补空格、同一列表里混用的 `-` / `*` / `+` 统一、表格分隔行规范化、长并列句按顿号/分号分行 |
| **收无效阅读字符** | 标题尾部多余的 `##`、`***` 与 `___` 混排统一、行尾那些看不见却搅乱 diff 的空格、文件末尾的多余空行 |
| **阅读视图** | 左右两栏对照：左边源文件、右边优化后的读法，三档语法显示（原样 / 弱化 / 隐藏），源文件只读、永不写回 |
| **读法体检** | 5 个维度打分（标题结构 / 段落结构 / 列表结构 / 空白分布 / 信息密度）+ 综合分，并指出具体在第几行、属于哪个维度 |
| **四种预设** | 通用文章 / 技术文档 / AI 生成文档 / 小说 —— 同一份文件可以按不同读法各跑一遍 |
| **改动可审** | 全部 11 条规则，改动按类别计数，逐条列出「第几行、哪条规则、改前 → 改后」，可以按规则接受或拒绝后重建 |
| **绝不覆盖** | 输出永远是新文件 `原名_阅读优化版.md` + 同名 `.json` 清单；重名自动避让，永不写入原文件 |
| **挂到 .md 上** | Windows 程序可注册进 `.md` 的「打开方式」，双击即用；写前备份、可一键撤销（系统「默认应用」那一步只能由你亲手点，程序不代劳也不假装代劳） |
| **离线报告** | 生成一份纯静态 HTML（无脚本、无外链），把评分、改动分类、保护区一次看清 |

## 它不改什么

代码块、行内代码、URL、邮箱、IP、路径、命令、表格单元、front matter（`---` 与 `+++`）、`$$` 公式块 —— 全部是禁区，规则连看都不看。

更硬的保证：优化前后**正文 token 序列逐个比对**，还看反斜杠总数、代码字节数、内容校验和。任何一处对不上，这条规则的改动整条丢弃并记账，不会静默落地。所以「只改读法」不是承诺，是退出码：

```bash
mdfile-cli check docs/*.md   # 任一份文件正文发生变化 => 退出码 1，可直接挂 CI
```

不做的事也说清楚：不改写措辞、不翻译、不摘要、不生成内容、不做 Unicode 归一化（全角半角、Emoji、中日韩字符原样保留）。

## 适合谁

- **要核定、审阅 AI 产出的人** —— 大模型爱写超长并列句和三层嵌套空行，这类稿子排版毛病最集中；工具本身不用 AI，也不评判内容写得对不对，只让你读得下去
- **写中文技术文档 / README 的人** —— 手写的、从 Notion/Obsidian 导出的、从 Word 转出来的，排版毛病各不相同
- **做代码评审 / 读别人文档的人** —— 不改内容，所以不需要说服原作者接受你的文字偏好，只调读感
- **笔记库重度用户** —— 支持递归批量处理整个目录，产物自动跳过，不会二次加工
- **要把它嵌进自己产品的人** —— 核心是一个 C ABI 动态库（`mdfile_optimize` / `mdfile_analyze` / `mdfile_preview` / `mdfile_rebuild` / `mdfile_rules_json`），只吃字节、只吐 JSON，五个平台共用同一份算法，不会出现「Windows 上排版好、手机上乱掉」

**不太适合**：想要「一键润色改写」的、只关心渲染后网页样式的（那是 CSS 的事）、以及要求工具替你决定用词的。

## 30 秒上手

Windows 图形界面：装好 .NET 10 后 `dotnet build -c Release`（`shell/win-wpf/`），双击打开的 .md 即进入阅读视图。
命令行与核心引擎：

```bash
cargo build --release
./target/release/mdfile-cli check    tests/corpus/*.md      # 内容不变性回归门
./target/release/mdfile-cli analyze  README.md              # 只读评分
./target/release/mdfile-cli optimize README.md --preset 技术文档
./target/release/mdfile-cli diff     README.md              # 改了什么，逐条列出
./target/release/mdfile-cli report   README.md              # 离线 HTML 报告
```

## 许可

MIT。

选它是因为实际情况允许：核心 15 个 Rust 依赖逐个查过 SPDX（pulldown-cmark / pulldown-cmark-escape / zmij 是 MIT，
bitflags / itoa / proc-macro2 / quote / serde 全家 / syn / unicase 是 MIT OR Apache-2.0，memchr 是 Unlicense OR MIT，
unicode-ident 是 (MIT OR Apache-2.0) AND Unicode-3.0），没有一个 copyleft 依赖会反向约束；
界面壳跑在 .NET 上，运行时本身也是 MIT。所以 MIT 既不影响你以后闭源分发，也不会和上游冲突。

---

一句话：**只改 Markdown 的读法，不改它说的话。**

本仓库是规格书定义的 **Shared Core**（共享核心）＋ 其 **C ABI 边界** ＋ 一个**无头命令行前端**。
它不含任何 UI 代码，也不含任何平台 API 调用——这是规格第七章「Core First / UI Independent /
Platform Independent」的可执行证明，而不是口头承诺。

- 语言：Rust（edition 2021），依赖只有 `pulldown-cmark`（解析）与可选的 `serde`（本机序列化）。
- 完全本地、离线、无 API Key、无账号、无网络请求、无云上传。v1 不含任何 AI 生成内容。
- 唯一硬约束：**优化前后正文内容必须逐 token 相同**。任何无法安全保证这一点的能力，宁可不做。

---

## 1. 目录结构

```text
mdlens/
├── Cargo.toml                     # workspace：三个 crate，release 用 panic=unwind（FFI 需要 catch_unwind）
├── crates/
│   ├── mdcore/                    # 共享核心：no_std-able 逻辑层，零平台依赖
│   │   ├── src/text.rs            #   两层模型：语义内容层 vs 文件格式层（BOM/LF/CRLF/CR/结尾换行）
│   │   ├── src/scan.rs            #   行级扫描：围栏状态机、front matter、表格上下文
│   │   ├── src/model.rs           #   Block / Document 语义块模型（来自事件流，不回写 Markdown）
│   │   ├── src/edits.rs           #   Edit{range,replacement,kind,scope,reason} 与 apply_edits
│   │   ├── src/rules.rs           #   R01–R11 规则实现 + Rule trait + Registry + Profile
│   │   ├── src/guard.rs           #   Content Guard：token 序列比对（不是字符数）＋ 4 项校验
│   │   ├── src/fingerprint.rs     #   分词、保护区、FNV-1a 手写哈希、首个分歧位置
│   │   ├── src/pipeline.rs        #   Optimizer：多轮收敛、逐规则原子提交、失败整条丢弃
│   │   ├── src/diff.rs            #   Diff 分类计数 + DecisionLog（接受/拒绝/撤销/重做/恢复）
│   │   ├── src/readability.rs     #   5 维评分（纯整数，advisory-only，永不驱动正文改动）
│   │   ├── src/presets.rs         #   通用文章 / 技术文档 / AI 生成文档 / 小说
│   │   └── src/io.rs              #   打开文档、输出命名（绝不覆盖原文）、批量计划、JSON 清单
│   │   └── tests/
│   │       ├── corpus_golden.rs   #   真实语料：内容不变性 100% 门 + 金样指纹 + 并发/顺序一致
│   │       └── architecture_guard.rs # 源码扫描：核心禁止出现任何平台 API / 文件 / 网络 / 时间
│   ├── mdcore-ffi/                # C ABI 边界：状态码 + JSON 负载 + latin1 字节通道
│   │   ├── src/lib.rs
│   │   └── include/mdfile.h       #   五平台共用契约（Windows dll / macOS / Linux / Android JNI / iOS）
│   └── mdfile-cli/                # 无头前端：analyze/optimize/diff/preview/rules/batch/check/report/fingerprint
│       └── src/{main.rs,report.rs}
├── tests/corpus/                  # 32 份语料 + MANIFEST：README、GFM、表格、代码、Unicode 边界
│                                   # 注意：MANIFEST 自己标注只有 3 份是真实下载（linux/typescript/flask），
│                                   # 其余为「按真实 README 结构仿写」；对外的「33 份真实语料」说法不准确
│   └── GOLDEN.csv                 # 金样：内容指纹、结构指纹、评分、编辑数、轮次、规则链
└── docs/
    ├── ARCHITECTURE.md            # 规格模块 ↔ 实际代码符号 对照表（非愿景，是现状）
    ├── TECH_STACK.md              # 三方案评估与被否理由、五平台落地方式
    └── DETERMINISM.md             # 确定性纪律清单（为什么逐条禁止某些写法）
```

---

## 2. 三十秒跑起来

```bash
cargo build --release
./target/release/mdfile-cli check tests/corpus/*.md      # 内容不变性回归门
./target/release/mdfile-cli optimize tests/corpus/02-readme-rust.md --preset 技术文档
./target/release/mdfile-cli report tests/corpus/02-readme-rust.md
```

`optimize` 只会新增 `02-readme-rust_阅读优化版.md` 与同名 `.json` 清单；重名时自动追加 `_2`，
**永远不会写入原文件**。`report` 只新增一个完全离线的 HTML（无脚本、无外链、正文全部转义）。

常用子命令：

| 命令 | 作用 | 是否写盘 |
| --- | --- | --- |
| `analyze` | 5 维评分 + 问题定位（只读） | 否 |
| `preview [--rule ID]` | 规则干跑，看改动不落盘 | 否 |
| `diff` | 结构变化分类计数 + 正文内容变化（应恒为 0） | 否 |
| `optimize` | 生成 `原名_阅读优化版.md` + `.json` 清单 | 新增 |
| `batch <dir>` | 递归批量优化，跳过本工具产物 | 新增 |
| `report <file\|dir>` | 离线 HTML 阅读优化报告 | 新增 |
| `check` | CI 门：任一文件正文变化 ⇒ 退出码 1 | 否 |
| `fingerprint` | 跨平台一致性探针，输出纯整数/稳定字符串 | 否 |
| `rules` | 列出全部规则的 ID / 优先级 / 安全级 / 作用域 | 否 |

---

## 3. 内容不变性是怎么被保证的（三重防线）

规格第二十八章要求「比较 token 序列，不是字符数」。实现按三条独立防线，任一失败即回退：

1. **静态校验（规则发出编辑时）**：`Edit.replacement` 不得含任何字母、数字、CJK、反斜杠。
   规则只能增删空白、改标题标记、改列表符号、加空行——从类型上就写不出「改字」的编辑。
2. **应用时保护**：代码块、行内代码、URL、邮箱、IP、路径、命令、表格单元、front matter
   由 `protected_ranges()` 标为禁区，任何落在此区的编辑直接丢弃。
3. **Content Guard（提交前后逐规则验证 + 整文档兜底）**：正文 token 序列逐个比对（最多报 8 处分歧）、
   FNV-1a 校验和、反斜杠总数、代码字节下界。逐规则失败 ⇒ 整条规则丢弃并记入 `rejected`（不静默）；
   整文档兜底失败 ⇒ 返回 `Err`，**绝不输出被改坏的文本**。

配套的语义纪律：

- 换行风格 / BOM / 结尾换行属于**格式层**，不计入内容变化（规格第四十一章）。
- 段落优化只做**边界**移动，不做文本合并重写；不做 AST→Markdown 再序列化（避免格式化器改写正文）。
- 评分与规则引擎解耦：评分低不会自动改正文，只是「提示」。
- 未采用任何 Unicode 归一化（NFC/NFD 都会改字节），Emoji / 全角半角 / 中日韩原样保留。

已在测试里固化的反例（证明 Guard 真会拦住）：

- `名称：产品` 改成表格会吃掉 `：` ⇒ 判为正文内容变化，拒绝（`colon_field_to_table_would_drop_punctuation`）。
- `甲乙` 重排成 `乙甲` 字符数不变 ⇒ 仍判为内容变化（`punctuation_loss_is_a_content_change`），
  证明门不是字符计数。

---

## 4. 与规格书的两个已知冲突（需产品决策，已按安全侧处理）

规格第十九章（隐式列表 → 有序列表）与第二十章（冒号字段 → 表格）按原文实现会
**新增数字 token** 与 **删除标点 `：`**，与第四章「标点/数字/文字不可改」直接冲突。
按第五十一章收尾规则「无法安全保证内容不变 ⇒ 暂不实现」，当前实现为**零改字等价物**：

| 规格原意 | 会造成的内容变化 | 现在的替代 |
| --- | --- | --- |
| 隐式并列项 → 编号列表 | 插入 `1.` `2.` 数字 | R11：只在冒号与并列顿号/逗号处**换行**，不加任何符号 |
| 冒号字段 → 表格 | 删除 `：`、插入 `|` 结构 | 保留原文，仅规范化已有表格分隔行；不改正文 |

如果愿意接受「正文增删标点/数字」，这两条可以开启，但那就不再是「只改读法」。
需要你在「严守内容不变」与「结构更漂亮」之间签字确认取哪一边。

---

## 5. 五平台状态（诚实版）

核心与 C ABI 是为五平台同时设计的，但**构建与验收状态分三档**，不要混淆：

### 已在本环境实测通过

- Linux x86_64：`cargo test` 全绿（**187 项**：mdcore 154 + 架构守卫 4 + 语料金样 4 + FFI 15 + CLI/report 10；
  2026-10-19 扩 Guard 可见范围后 +8 项）。
- `mdfile-cli check tests/corpus/*.md` ⇒ 33 份语料全部通过，正文内容不变性 100%。
- **Windows x86_64：本机用交叉工具链（x86_64-pc-windows-gnu）真实构建并运行**，
  `fingerprint tests/corpus` 输出与 Linux **逐字节一致**；C ABI 用 Python ctypes 直调 `mdcore_ffi.dll` 验过
  （状态码、BOM/CRLF 还原、全角空格保留、错误通道、释放契约）。五平台门目前是 **2/5 实测**，不是推定。
- FFI 边界的真实 C 客户端验收代码已入仓库：`tools/ffi-acceptance/ffi_client_test.c`（`./run.sh` 或 `./run.sh docker`）。
- 中文/边界探针：`tools/probes/probe.sh`、`tools/probes/probe2.sh`（全角空格、NBSP、`+++` 元数据、`***` 分隔线陷阱、
  多行 setext、`$$` 公式、NUL、CRLF/BOM 逐字节还原）。
- 金样复现性：`LC_ALL=zh_CN.UTF-8` 与不同 `RUST_MIN_STACK` 下 `GOLDEN.csv` 完全一致；
  4 线程并发与单线程顺序结果一致。

### 已具备能力、但尚未在真机验证

- Android / iOS / macOS 目标：**没有跨目标编译产物可展示**（构建环境里 `rustup target add` 拿不到镜像）。
  **本机更新（2026-10-08）：Windows 一档已不再是「无法验证」** —— 用 `x86_64-pc-windows-gnu` 交叉构建出
  `mdfile-cli.exe` 与 `mdcore_ffi.dll`，在真机上跑 `check`（33/33）与 `fingerprint`，
  输出与 Linux 逐字节一致；C ABI 另用 Python ctypes 直调 DLL 验过。五平台门为 **2/5 实测**。
- 代替性证明：`architecture_guard.rs` 源码级扫描 ＋ `fingerprint` 探针 ＋ `GOLDEN.csv`。
  注意这条守卫本身有过死规则（比对前源码被转小写，而 `SystemTime`/`Instant::now`/`File::open`/`File::create`/
  `Command::new` 五个模式带大写 ⇒ 永不触发），已修并加了「守卫的守卫」测试；
  它仍然拦不住 `use std::{fs, env};` 以外的花式写法（如 `#[cfg(all(unix, target_arch = "x86_64"))]`），
  所以它是**辅助**，跨平台一致性的裁判始终是 `fingerprint` 逐字节对比。

### 尚未实现（需要本机工具链）

- **Windows 壳已落地**：`shell/win-wpf/`（.NET 10 WPF，P/Invoke 同一份 `mdfile.h`）。
  做四件事：打开 .md（含双击/「打开方式」传入）、阅读视图、逐条接受或拒绝改动（走核心 `mdfile_rebuild`）、
  另存为 `原名_阅读优化版.md`。
  **UI 不参与任何落盘字节的决定**：要不要改、改哪儿、能不能改，全部来自核心返回的 JSON 与 `rendered`。
  阅读视图只决定屏幕上怎么摆——把硬换行接回整句、行长封顶（约 40 个汉字）、标题层级与节奏、
  列表悬挂缩进、引用块、代码块、表格在隐藏档画成真表格；语法符号分原样 / 弱化 / 隐藏三档。
  这一层没有任何写文件的路径，切换模式与滚动之后源文件 md5 不变（已实测）。
  详见 `shell/win-wpf/README.md`，可复现的两份样例文档在 `shell/win-wpf/samples/`。
- 其余三个壳：macOS(AppKit/SwiftUI)、Linux(GTK4)、Android(Compose)、iOS(SwiftUI) 仍未实现。
  它们只做「文件选择 + 调 `mdfile.h` + 渲染结构化数据 + 接受/拒绝按钮」，逻辑一行都不写在 UI 里。
- Tauri / Flutter 桌面壳：缺少 WebKitGTK/GTK 依赖的环境装不起来；建议在 **Windows** 上优先做
  Tauri v2（MSVC + WebView2 自带），macOS/Linux 复用同一份前端。

---

## 6. 把工程落到本机后各平台怎么构建

```bash
# 共享核心 + CLI（五平台同一条命令）
cargo build --release

# Windows：核心 dll + C ABI（供 WPF/WinUI/Tauri 调用）
cargo build --release -p mdcore-ffi            # 产物 mdcore_ffi.dll / .lib

# Android：cargo-ndk（NDK 需本机安装）
cargo ndk -o ../android/app/src/main/jniLibs \
  build --release -p mdcore-ffi
# 产物 libmdfile_ffi.so：arm64-v8a / armeabi-v7a / x86_64，Kotlin 侧 System.loadLibrary + JNI

# iOS：xcframework（需 macOS + Xcode 交叉目标）
for t in aarch64-apple-ios aarch64-apple-ios-sim; do
  cargo build --release -p mdcore-ffi --target $t
done
xcodebuild -create-xcframework \
  -framework ios/build/Release-iphoneos/libmdfile_ffi.a -headers ../ios/headers \
  -framework sim/build/Release-iphonesimulator/libmdfile_ffi.a -headers ../ios/headers \
  -output mdfile.xcframework
```

CI 必跑的两条门（任何平台）：

```bash
cargo test                      # 含架构守卫与语料不变性
mdfile-cli check tests/corpus/*.md
```

---

## 7. MVP 23 项清单（第二十九章）落地状态

| # | 项 | 状态 | 位置 |
| --- | --- | --- | --- |
| 1 | 打开单个 .md | 核心 + CLI 已实现 | `io::OpenDocument` |
| 2 | 打开文件夹批量 | 已实现 | `io::plan_batch` / `cli batch` |
| 3 | 一键阅读优化 | 已实现 | `pipeline::Optimizer::run` |
| 4 | 标题层级分析与修复 | 已实现 | R01/R02/R03 |
| 5 | 标题过深 / 密集 / 碎片化检测 | 已实现 | `readability::heading` + findings |
| 6 | 重复标题 / 标题饥饿检测 | 已实现（仅提示，不自动改名） | `readability` findings |
| 7 | 段落边界优化 | 已实现（仅空行/边界） | R04/R05 |
| 8 | 隐式列表识别 | **降级实现**（见第 4 节） | R11 |
| 9 | 冒号字段转表格 | **未实现**（会删 `：`） | 替代：表格分隔行规范化 R08 |
| 10 | 列表标记统一 | 已实现（只统一同一列表内混用的 `-`/`*`/`+`；**缩进修正未实现**，R06 也只是引用标记后补空格） | R07 |
| 11 | 表格结构整理 | 已实现 | R08 |
| 12 | 代码块完全保护 | 已实现（逐字节测试） | `protected_ranges` + 金样测试 |
| 13 | 特殊内容保护（链接/图片/HTML/front matter） | 已实现 | `fingerprint::protected_ranges` |
| 14 | Diff 展示 + 分类计数 | 已实现 | `diff::Diff` / `cli diff` |
| 15 | 正文内容变化必须为 0 | 已实现并作为准入门 | `guard` / `cli check` |
| 16 | 接受 / 拒绝 / 撤销 / 恢复 | 核心数据结构已实现 | `diff::DecisionLog::rebuild` |
| 17 | 阅读性评分（5 维 + 综合） | 已实现（advisory-only） | `readability` |
| 18 | 四种风格预设 | 已实现 | `presets` |
| 19 | 规则可插拔（Rule trait 全字段） | 已实现 | `rules::Rule` |
| 20 | 每步可回滚 / 逆编辑 | 已实现（原始文 + 编辑区间） | `edits` / `DecisionLog` |
| 21 | 项目格式开放（Markdown + JSON，无数据库） | 已实现 | `io::Manifest` |
| 22 | 绝不覆盖原文 | 已实现（命名推导 + 冲突避让 + 产物识别） | `io::derive_output_name` |
| 23 | 自动化测试（不变性 100%） | 已实现，187 项全绿（本机复核后） | `tests/` + `tools/` |

跨设备迁移、批量的「导出到其他设备」属于 UI/平台层（SAF、iCloud 文件提供器等），
核心的 `Manifest` 已带原始/优化校验和，迁移时可用同一门复验。

---

## 8. 性能（第四十二章）

- 3.6 MB 单文件全量优化：约 3.1 秒（修复了三处 O(n²)：snippet 逐次重建行索引、
  去重的线性 `contains`、代码块上下界双搜索）。小文件 2 ms 级。
- 行索引 `Lines` 建一次、区间查询 O(log n)；代码块判定改成 O(n) 掩码。
- 输入上限 64 MiB 的唯一执行点在 `mdcore::io::OpenDocument::from_bytes`（`mdfile.h` 的
  `MDFILE_MAX_INPUT_BYTES` 引用的同一常量）；CLI 在读盘前另有大小预检。超限返回状态码而非崩溃；
  大文件应在 UI 层走后台线程 + 进度取消（`pipeline::Cancel`）。

---

## 9. 已知限制（不粉饰）

1. 未实现第十九/二十章的原始形态，理由见第 4 节；等你签字决定要不要放宽「内容不变」。
2. 交付物里**只有 Linux 桌面二进制**可直接运行；其余四平台的可执行文件必须在你的机器上构建
   （拿不到交叉工具链 / Android SDK / Xcode / WebKitGTK）。
3. 没有 GUI。`report` 生成的 HTML 是「展示层用结构化数据」的示例实现，不是最终产品界面。
4. `is_optimized_output` 依赖本工具的命名约定；用户手工改名成别的名字仍会被当普通文件处理。
5. 评分是启发式提示，不构成质量保证；分数上升不代表「更好读」这一主观判断成立。
6. 金样 `GOLDEN.csv` 锁的是当前规则集输出。加规则或改规则必然改金样，需要显式 `MDFILE_REGOLDEN=1`
   重生成并在评审里说明——这是有意的摩擦，防止无声改行为。
   （2026-10-08 本机复核就走过一次：修掉「`$$` 公式里的 `=` 被当标题」后，只有 `11-math-special.md` 一行变化，
   旧基线的逐行差异记在 `CHANGELOG.md`。）
7. **核心只吃 UTF-8**，非 UTF-8 会被干净拒绝、绝不猜编码。转码属于平台层，核心不碰。
   Windows 壳（`shell/win-wpf/Shell/TextDecode.cs`）已按这个分工做了显示层转码：BOM 明确按 BOM；
   含 NUL 的字节流直接拒读（顺手堵住了「无 BOM 的 UTF-16LE 恰好是合法 UTF-8 会被当正文收下」这个洞）；
   严格 UTF-8 解得出来就是 UTF-8；都不行才试 GB18030（GBK 的超集）；再不行就明说认不出。
   转码只发生在内存里——原文件字节不动，状态条写明「现在读的是 UTF-8 副本」，另存出去的会是 UTF-8 新文件。
   实测：同一份内容的 UTF-8 / GBK / UTF-16LE 三个文件，核心算出的正文校验和完全相同（`284810…085`）。
   其余四个平台还没做这一层，中文 Windows 用户的老文件在它们上目前仍会被拒。
7b. `fingerprint` 输出的**第一行带引擎版本号**（`# mdfile fingerprint  engine=0.1.1`），33 行数据才是要逐字节比的东西。跨版本 diff 基线时会看到这一行变动，那不是不一致；比对时要么连头一起同版本生成，要么只比数据行。
8. 指纹依赖 `std` 的 Unicode 表（`is_whitespace` / `is_alphanumeric`）来切 token，因此**理论上也依赖工具链版本**：
   ~~仓库没有 `rust-toolchain.toml` 固定~~ —— **2026-10-24（v0.1.4）已固定 1.99.1**：
   1.99.0（云端基线生成版本）与 1.99.1（Windows 本机）双向验证指纹逐字节一致。
   升级 pinned 版本前必须在全部平台重跑指纹对账，绿了才允许提交。
9. ~~Content Guard 看不见脚注引用与任务列表标记~~ —— **2026-10-19 已扩**（用户签字「扩」）：
   `tokenize()` 现在为 `[^1]`（引用与定义标签）和 `- [ ]` / `- [x]` 各发射一个 `Ref` token，
   吃掉编号或丢掉勾选状态都会当场拒绝该规则。实测代价为零改动输出：33 份语料里只有 3 份的
   指纹列与 `r` 计数变了（`10-gfm-extensions` r0→r6、`17-nested-lists` r0→r25、`18-blockquotes-footnotes` r0→r21），
   输出字节长度、结构变化处数、命中的规则链、五维评分**全部不变**——说明现有规则从没动过这些标记，
   补的是盲区而不是行为。金样与指纹基线已重生成，旧版逐行差异同样记在 `CHANGELOG.md`。
   HTML 注释仍走 `Event::Html` 逐字节比对（有测试钉住）。
