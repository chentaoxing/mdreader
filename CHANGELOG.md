# 更新记录

格式参考 Keep a Changelog；版本号遵循 SemVer。`mdfile_*` 是 C ABI 符号前缀，只要它不破坏，
主版本号就不会因为内部重构而跳动。

## 0.1.0 — 2026-10-19

第一个可跑的形态：跨平台核心 + 无头 CLI + Windows 图形界面。

### 新增

- **核心 `mdcore`**：Markdown 扫描、规则引擎（11 条）、内容守卫（Content Guard）、可读性评分、
  差异与清单输出。平台无关，不读文件、不取时间、不用哈希表遍历顺序、无网络。
- **C ABI `mdcore-ffi`**：`mdfile_optimize` / `mdfile_analyze` / `mdfile_preview` / `mdfile_rebuild` /
  `mdfile_rules_json` / `mdfile_last_error`，只吃字节、只吐 JSON，五平台共用同一份算法。
- **无头 CLI `mdfile-cli`**：`check` / `analyze` / `optimize` / `diff` / `report` / `fingerprint` / `rules`；
  `check` 任一份文件正文发生变化即以退出码 1 结束，可直接挂 CI。
- **Windows 界面壳**（`shell/win-wpf/`，.NET 10 WPF）：左栏源文件（等宽、只读、永不写回），
  右栏阅读视图；语法符号三档（原样 / 弱化 / 隐藏）；改动清单可逐条接受或拒绝并即时重建；
  「专心读」视图与四档字号；「设为 .md 打开方式」可注册可撤销。
- **阅读视图排版**（只在屏幕上）：硬换行接回整句、行长封顶、标题层级与节奏、列表悬挂缩进、
  引用块、代码块、表格在隐藏档画成真表格、任务项渲染成 ☑ / ☐、front matter 收成元信息块。
- **编码层**（`shell/win-wpf/Shell/TextDecode.cs`，只在内存里）：GBK / GB18030 / UTF-16 的老 .md
  能打开来读；核心仍然只吃 UTF-8，原文件字节不动，另存写出的会是 UTF-8 新文件。
- **验收资产**：`tools/ffi-acceptance/`（真实 C 客户端，Linux 与 Windows 两侧都跑过）、
  `tools/probes/`（全角空格、`+++`、`***`、多行 setext、`$$` 公式、NUL、CRLF/BOM 逐字节还原等边界探针）、
  `tests/corpus/` 33 份语料 + 金样 `GOLDEN.csv` + `fingerprint` 逐字节基线。

### 修复（本机复核时发现的六个会改坏用户正文的缺陷）

- 块级公式 `$$ … $$` 内的行被当普通空行压缩、公式内 `=` 被认成 setext 标题下划线 ——
  新增数学区识别并并入受保护区间。
- 由全角空格（U+3000）组成的行被当空行删除 —— 空白判定改为只认 ASCII 空白，全角空格与 NBSP 视为正文。
- 多行 setext 标题被从中间劈开 —— 识别回溯整段标题文字。
- 标题下插入空行时误造出一条 setext 下划线；`R11` 会在引号字符串内部断行 —— 各自加前置判定。
- `+++` 风格的 front matter 被拆行 —— 纳入元数据保护区间。
- C ABI 侧 `mdfile_last_error` 返回静态指针，按头文件释放会踩坏堆；`mdfile_rebuild` 把
  `content_changes` 写死为 0（等于自证清白）—— 改为堆分配 + 真正过守卫，头文件同步更正。
- CLI 无参数或只给 `--help` 时 panic、未知参数被忽略、清单 JSON 少一个引号、输出名去重可能返回已占用名 ——
  引入参数解析器并逐条加测试。
- 架构守卫里五个禁用模式因大小写写反而**永不触发** —— 修正并加「守卫的守卫」测试。

### 变更

- **Content Guard 可见范围扩大**：脚注引用与定义标签（`[^1]`）、任务列表符号（`[ ]` / `[x]`）
  从"语法侧"移入"正文侧"，规则吃掉它们会当场被拒。33 份语料里只有 3 份的**指纹与 `r` 计数**两列变化
  （`10-gfm-extensions` r0→r6、`17-nested-lists` r0→r25、`18-blockquotes-footnotes` r0→r21），
  输出字节长度、结构变化处数、命中规则链、五维评分逐列不变 —— 补的是盲区，不是行为。
  金样与指纹基线随之重生成（`MDFILE_REGOLDEN=1` 显式触发，正常跑不会静默改基线）。
- README 里两处虚标更正：MVP 第 10 项「缩进修正」实际未实现（只统一同一列表内混用的符号）；
  「33 份真实语料」按 `tests/corpus/MANIFEST.md` 自述改为准确说法。

### 已知限制

见 README 最后一节，不在此重复。其中两条最要紧：**五平台一致性目前只有 Linux + Windows 实测**；
`rust-toolchain.toml` 尚未固定，而指纹依赖 `std` 的 Unicode 表。
