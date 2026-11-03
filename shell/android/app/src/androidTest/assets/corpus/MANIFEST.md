# Corpus MANIFEST

> 共 32 个测试文件，覆盖全部要求特性。  
> 标注 `synthetic` 的文件为手工构造（网络下载受限），其余为基于真实项目 README 结构仿写。

| # | 文件名 | 大小 | 来源 | 覆盖特性 |
|---|--------|------|------|----------|
| 1 | `01-readme-nodejs.md` | 942 B | synthetic (Node.js README 风格) | 英文技术 README、代码块(bash/js)、加粗列表、链接 |
| 2 | `02-readme-rust.md` | 1.0 KB | synthetic (Rust README 风格) | 英文 README、badge 图片、代码块(bash)、链接 |
| 3 | `03-readme-vue.md` | 2.2 KB | synthetic (Vue.js README 风格) | HTML 居中图片、badge 组、代码块(html/js)、引用链接 |
| 4 | `04-readme-vscode.md` | 2.3 KB | synthetic (VSCode README 风格) | HTML 图片、badge、代码块(bash)、多种链接 |
| 5 | `05-readme-linux.md` | 5.9 KB | **downloaded** (Linux kernel README) | 真实英文技术文档 |
| 6 | `06-readme-tensorflow.md` | 2.7 KB | synthetic (TensorFlow README 风格) | Python 代码块、表格、badge、多种链接 |
| 7 | `07-readme-react.md` | 1.5 KB | synthetic (React README 风格) | JSX 代码块、多种组件示例 |
| 8 | `08-readme-typescript.md` | 2.7 KB | **downloaded** (TypeScript README) | 真实英文 README |
| 9 | `09-readme-flask.md` | 1.7 KB | **downloaded** (Flask README) | 真实英文 README |
| 10 | `10-gfm-extensions.md` | 696 B | synthetic | 删除线、任务列表、autolink、表格混合 |
| 11 | `11-math-special.md` | ~1.2 KB | synthetic | LaTeX 数学公式($/$$)、定义列表、上下标、WikiLinks |
| 12 | `12-cjk-mixed.md` | 1.9 KB | synthetic | 中文标点(，。：；？！)、全角字符、中英混排、中文代码注释 |
| 13 | `13-japanese.md` | 1.4 KB | synthetic | 日文假名(平/片)、汉字、日文代码注释 |
| 14 | `14-korean.md` | 971 B | synthetic | 韩文(한글)、韩英混排 |
| 15 | `15-complex-code.md` | 7.9 KB | synthetic | 大量代码块：Python/JS/TS/Rust/Bash/JSON/YAML/SQL、嵌套代码块 |
| 16 | `16-complex-tables.md` | 3.4 KB | synthetic | 复杂表格：对齐、空单元格、长内容、HTML 内嵌、宽表格、特殊字符 |
| 17 | `17-nested-lists.md` | 3.1 KB | synthetic | 深层嵌套列表(10层)、混合有序/无序、GFM 任务列表(嵌套)、列表内含代码/引用 |
| 18 | `18-blockquotes-footnotes.md` | 3.3 KB | synthetic | 多层嵌套引用、引用内含 Markdown/HTML/代码、Footnote 语法、行内脚注 |
| 19 | `19-html-inline.md` | 3.8 KB | synthetic | 原始 HTML 块、内联 HTML、HTML 实体、HTML 注释、HTML5 语义元素、HTML 表单 |
| 20 | `20-images-links.md` | 3.8 KB | synthetic | 图片(多格式)、inline/reference/autolink 链接、data URI、mailto/tel、锚点 |
| 21 | `21-emoji-unicode.md` | 2.7 KB | synthetic | Emoji(基本/旗帜/肤色/ZWJ)、组合字符、代理对、变体选择符、字素簇、NFC/NFD |
| 22 | `22-ai-generated-style.md` | 3.3 KB | synthetic | AI 生成典型样式：过度加粗、多层标题、混排列表、表格、总结段 |
| 23 | `23-setext-headings.md` | 1.6 KB | synthetic | Setext 标题(=和-)、ATX 标题、标题层级跳跃、空标题、尾部 # |
| 24 | `24-long-paragraphs.md` | 4.2 KB | synthetic | 超长段落(阅读性差)、极短碎片段落、混合质量 |
| 25 | `25-fullwidth-halfwidth.md` | 1.6 KB | synthetic | 全角/半角数字字母符号混排、半角片假名、CJK+Latin 混合 |
| 26 | `26-frontmatter-only.md` | 139 B | synthetic | 仅 YAML front matter，无正文内容 |
| 27 | `27-frontmatter-with-content.md` | 305 B | synthetic | YAML front matter + 正文 |
| 28 | `28-empty-file.md` | 1 B | synthetic | 空文件(仅换行) |
| 29 | `29-crlf-line-endings.md` | 296 B | synthetic | **CRLF 行尾** (Windows 风格)，含标题/列表/表格/引用/代码块 |
| 30 | `30-crlf-line-endings-2.md` | 183 B | synthetic | **CRLF 行尾** (第二个文件)，含代码块(Rust) |
| 31 | `31-cr-line-endings.md` | 92 B | synthetic | **CR 行尾** (旧 Mac 风格，仅 \r) |
| 32 | `32-utf8-bom.md` | 177 B | synthetic | **UTF-8 BOM** (EF BB BF) + CRLF 行尾 |

## 特性覆盖矩阵

| 要求特性 | 覆盖文件 |
|----------|----------|
| 英文技术文档/README | 01-09 |
| 中文 Markdown (含中文标点/全角) | 12, 25 |
| AI 生成典型样式 | 22 |
| 大量代码块 (Python/JS/Rust/Shell/JSON/YAML/SQL) | 15, 01-09 |
| 复杂表格 | 16, 06 |
| 嵌套列表/任务列表 (GFM) | 17, 10 |
| 引用块嵌套/Footnote | 18 |
| 原始 HTML/内联 HTML | 19, 03, 04 |
| 图片/多种链接形式 | 20, 02-04 |
| Emoji/CJK/日文/韩文/组合字符/全角半角 | 21, 12, 13, 14, 25 |
| 长段落/极短碎片段落 | 24 |
| Setext 标题/多层级/层级跳跃 | 23 |
| CRLF 行尾 (≥2) | 29, 30, 32 |
| CR 行尾 (≥1) | 31 |
| UTF-8 BOM (≥1) | 32 |
| 空文件 | 28 |
| 仅 front matter | 26 |
| 数学公式/特殊语法 | 11 |

## 统计

- 总文件数：**32**
- 下载真实文件：**3** (05, 08, 09)
- 手工构造 (synthetic)：**29**
- 最大文件：`15-complex-code.md` (7.9 KB) — 远低于 400KB 限制
- 总大小：约 55 KB

## 下载失败说明

由于沙箱网络环境限制 (curl 连接 raw.githubusercontent.com 出现 SSL 错误 exit code 35)，
仅成功下载 3 个真实文件 (Linux kernel README、TypeScript README、Flask README)。
其余 29 个文件基于真实开源项目 README 的结构和内容风格手工构造，
在 MANIFEST 中标注为 `synthetic`。这些文件覆盖了所有要求的测试特性。
