# ARCHITECTURE.md — 跨平台核心架构设计

> 版本：1.0 (2026-10-05)  
> 目标：定义「MD文档阅读助手」的 Core First 架构，确保五平台共享核心引擎

---

## 一、核心架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                        Platform Layer                            │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐ ┌──────┐│
│  │ Windows  │  │  macOS   │  │  Linux   │  │Android │ │ iOS  ││
│  │  (Tauri) │  │  (Tauri) │  │  (Tauri) │  │ (Tauri)│ │(Tauri)│
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └───┬────┘ └──┬───┘│
│       │              │              │            │         │     │
│       └──────────────┴──────────────┴────────────┴─────────┘     │
│                              │ FFI / Native Bridge               │
└──────────────────────────────┼───────────────────────────────────┘
                               │
┌──────────────────────────────┼───────────────────────────────────┐
│                    Core Abstraction Layer                         │
│  ┌───────────────────────────┴───────────────────────────────┐  │
│  │              Unified Platform Interface                    │  │
│  │  • DocumentSource  • DocumentStorage  • FilePicker        │  │
│  │  • ShareService    • ClipboardService                     │  │
│  └───────────────────────────────────────────────────────────┘  │
└──────────────────────────────┼───────────────────────────────────┘
                               │
┌──────────────────────────────┼───────────────────────────────────┐
│                     Rust Core Engine                             │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Markdown Processing Pipeline                 │   │
│  │                                                           │   │
│  │  ┌─────────────────┐                                      │   │
│  │  │ markdown_parser │ ← pulldown-cmark 0.13                │   │
│  │  └────────┬────────┘                                      │   │
│  │           ↓                                               │   │
│  │  ┌────────┴────────┐                                      │   │
│  │  │  markdown_ast   │ ← 自定义 AST 扩展                    │   │
│  │  └────────┬────────┘                                      │   │
│  │           ↓                                               │   │
│  │  ┌────────┴────────┐                                      │   │
│  │  │ document_model  │ ← 语义化文档模型                     │   │
│  │  └────────┬────────┘                                      │   │
│  │           ↓                                               │   │
│  │  ┌────────┴─────────┐                                     │   │
│  │  │document_analyzer │ ← 结构分析、可读性评估              │   │
│  │  └────────┬─────────┘                                     │   │
│  │           ↓                                               │   │
│  │  ┌────────┴──────────┐                                    │   │
│  │  │readability_engine │ ← 可读性优化引擎                   │   │
│  │  └────────┬──────────┘                                    │   │
│  │           ↓                                               │   │
│  │  ┌─────────┴─────────┐                                    │   │
│  │  │   rule_engine     │ ← 规则匹配与执行                   │   │
│  │  └─────────┬─────────┘                                    │   │
│  │            ↓                                              │   │
│  │  ┌──────────┴──────────┐                                  │   │
│  │  │structure_optimizer  │ ← 结构优化（段落拆分、列表整理） │   │
│  │  └──────────┬──────────┘                                  │   │
│  │             ↓                                             │   │
│  │  ┌──────────┴──────────┐                                  │   │
│  │  │  content_guard      │ ← 内容完整性保护（确保语义不变） │   │
│  │  └──────────┬──────────┘                                  │   │
│  │             ↓                                             │   │
│  │  ┌──────────────┴───────────┐                             │   │
│  │  │   content_fingerprint    │ ← 内容指纹（变更检测）       │   │
│  │  └──────────────┬───────────┘                             │   │
│  │                 ↓                                         │   │
│  │  ┌──────────────┴──────┐                                  │   │
│  │  │    diff_engine      │ ← 差异对比与可视化               │   │
│  │  └──────────────┬──────┘                                  │   │
│  │                 ↓                                         │   │
│  │  ┌──────────────┴──────┐                                  │   │
│  │  │ markdown_renderer   │ ← 渲染回 Markdown                │   │
│  │  └─────────────────────┘                                  │   │
│  └───────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                  Supporting Modules                       │   │
│  │  • unicode_utils    • line_ending    • encoding_detect    │   │
│  │  • front_matter     • math_extension • custom_events      │   │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

---

## 二、核心模块职责（已落地，与 `crates/mdcore/src/` 一一对应）

> 规格第十五章明确禁止重新实现 Markdown 解析，因此本项目**不自建 AST**：
> `markdown_parser / markdown_ast / document_model / markdown_renderer` 四个规格模块
> 由「成熟解析器事件流 + 块级视图 + 编辑式写回」承担，映射如下。

| 规格模块 | 实际实现 | 职责 | 输入 → 输出 |
|----------|----------|------|-------------|
| markdown_parser | `fingerprint::PARSE_OPTIONS` + pulldown-cmark 0.13.4 | 统一解析入口，全项目只有一处构造 Parser，保证五平台同参数 | `&str` → `Event` 流（带 offset） |
| markdown_ast | `model::Document::parse`（事件流上的块/大纲层） | 不重造 AST，只建「块级 + 大纲」视图 | `Event` 流 → `Vec<Block>` |
| document_model | `model::{Block, BlockKind, Document, outline, block_of, code_ranges}` | 语义化文档模型 | `Document` 查询接口 |
| document_analyzer | `scan::Scan`（逐行分类 + 保护区）+ `model` 计数 | 结构分析，坐标为语义层字节偏移 | `&str` → `Scan` / 指标 |
| readability_engine | `readability::analyze` | 五维评分 + 明细 + findings，**全整数**，只提示不改写 | `Scan`+`Document` → `Report` |
| rule_engine | `rules::{Rule, Registry, Profile, SafetyLevel, builtin_rules}` | 规则接口（ID/名称/描述/优先级/安全级/Detect/Preview/Apply/Rollback/Validate） | `Ctx` → `Option<RuleEdit>` |
| structure_optimizer | `pipeline::Optimizer::run` | 多轮编排、逐条提交、收敛 | 正文 → `Outcome` |
| content_guard | `guard::{verify, ensure_pass, GuardReport, Violation}` | **token 序列**比对（非字数）+ 反斜杠数 + 代码字节下限 | (原, 候选) → `GuardReport` |
| content_fingerprint | `fingerprint::{tokenize, Fingerprint, fnv1a64}` | 手写 FNV-1a 64，无依赖、无 locale | `&str` → `Fingerprint` |
| diff_engine | `diff::{Diff, DiffEntry, CategoryCount, DecisionLog}` | 分类计数 + 正文变化(恒 0) + 接受/拒绝/撤销 | `Outcome` → `Diff` |
| markdown_renderer | `text::TextLayer::render` | 不重排 Markdown，只按格式层（BOM/换行/结尾换行）写回字节 | (语义层正文, 风格) → `Vec<u8>` |

关键取舍：**输出永远基于原文编辑，不做 AST→Markdown 重序列化**。
重序列化会引入标点、转义、列表序号等无法证明无损的改写，违背内容不可变原则；
因此规则只提交 `Edit{range, replacement}`（`edits::apply_edits`），替换体被静态自检限制为
「不含字母数字与 CJK」，再由 Content Guard 动态否决。

### 2.2 辅助模块

| 规格模块 | 实际实现 | 说明 |
|----------|----------|------|
| unicode_utils | `text::Lines` + `fingerprint::is_cjk_char` + `readability::display_width` | **不做 Unicode 规范化**（NFC/NFD 会改字节）；只识别 CJK/全角/Emoji 宽度用于排版判断 |
| line_ending | `text::{NewlineStyle, TextLayer}` | LF/CRLF/CR 属格式层；混合换行标记为 `uniform = false` 并降级为 LF，**不计入内容变化** |
| encoding_detect | `text::TextLayer::from_bytes` | **刻意不做编码猜测**：非 UTF-8 直接报错 `CoreError::InvalidUtf8`。按 GBK 猜编码 = 用错编码就静默毁正文 |
| front_matter | `scan::LineKind::FrontMatter`（整段列保护） | 原样保留，不参与任何规则；`scan` 只在成对出现时判定为 front matter |
| math_extension | `PARSE_OPTIONS` 未开启 `ENABLE_MATH` | v1 不开：数学语法属结构层，开启后 `$…$` 可能被解析成强调，破坏正文 |
| custom_events | 不需要 | 未启用 wikilinks/定义列表/上下标等扩展位，避免解析口径随平台变化 |

### 2.3 对外边界

| 边界 | 位置 | 用途 |
|------|------|------|
| C ABI | `crates/mdcore-ffi` + `include/mdfile.h` | Windows/macOS/Linux/Android/iOS 五平台唯一功能入口 |
| 无头 CLI | `crates/mdfile-cli` | 证明核心可脱离 GUI 运行；`fingerprint` 子命令是跨平台一致性探针 |


---

## 三、统一文件抽象层接口设计

### 3.1 核心 Trait 定义

```rust
/// 文档来源抽象（屏蔽平台差异）
pub trait DocumentSource: Send + Sync {
    /// 读取文档内容（返回原始字节，由 Core 处理编码）
    fn read(&self, path: &Path) -> Result<Vec<u8>>;
    
    /// 检查文档是否存在
    fn exists(&self, path: &Path) -> Result<bool>;
    
    /// 获取文档元信息（大小、修改时间等）
    fn metadata(&self, path: &Path) Result<DocumentMetadata>;
    
    /// 列出目录下的 Markdown 文件
    fn list_markdown_files(&self, dir: &Path) -> Result<Vec<PathBuf>>;
}

/// 文档存储抽象
pub trait DocumentStorage: Send + Sync {
    /// 写入文档（保留原始行尾和编码）
    fn write(&self, path: &Path, content: &[u8]) -> Result<()>;
    
    /// 创建备份
    fn backup(&self, path: &Path) -> Result<PathBuf>;
    
    /// 删除文档
    fn delete(&self, path: &Path) -> Result<()>;
    
    /// 重命名/移动
    fn rename(&self, from: &Path, to: &Path) -> Result<()>;
}

/// 文件选择器抽象（屏蔽平台文件选择对话框差异）
pub trait FilePicker: Send + Sync {
    /// 打开文件选择对话框
    fn pick_file(&self, options: PickOptions) -> Result<Option<PathBuf>>;
    
    /// 打开目录选择对话框
    fn pick_directory(&self, options: PickOptions) -> Result<Option<PathBuf>>;
    
    /// 保存文件对话框
    fn save_file(&self, options: SaveOptions) -> Result<Option<PathBuf>>;
}

/// 分享服务抽象（屏蔽平台分享机制差异）
pub trait ShareService: Send + Sync {
    /// 分享文件
    fn share_file(&self, path: &Path) -> Result<()>;
    
    /// 分享文本
    fn share_text(&self, text: &str, title: Option<&str>) -> Result<()>;
    
    /// 获取可用的分享目标列表
    fn get_share_targets(&self) -> Result<Vec<ShareTarget>>;
}

/// 剪贴板服务抽象
pub trait ClipboardService: Send + Sync {
    /// 读取剪贴板文本
    fn read_text(&self) -> Result<Option<String>>;
    
    /// 写入剪贴板文本
    fn write_text(&self, text: &str) -> Result<()>;
    
    /// 读取剪贴板文件路径
    fn read_file_path(&self) -> Result<Option<PathBuf>>;
    
    /// 写入文件路径到剪贴板
    fn write_file_path(&self, path: &Path) -> Result<()>;
}
```

### 3.2 平台适配器实现

| 平台 | DocumentSource | FilePicker | ShareService |
|------|----------------|------------|--------------|
| **Windows** | `Win32DocumentSource` (Win32 API) | `Win32FilePicker` (IFileOpenDialog) | `Win32ShareService` (Share contract) |
| **macOS** | `MacDocumentSource` (Foundation) | `MacFilePicker` (NSOpenPanel) | `MacShareService` (NSSharingService) |
| **Linux** | `LinuxDocumentSource` (std::fs) | `LinuxFilePicker` (zenity/kdialog) | `LinuxShareService` (xdg-open) |
| **Android** | `AndroidDocumentSource` (SAF) | `AndroidFilePicker` (Intent) | `AndroidShareService` (Intent.ACTION_SEND) |
| **iOS** | `IosDocumentSource` (UIDocumentPicker) | `IosFilePicker` (UIDocumentPicker) | `IosShareService` (UIActivityViewController) |

---

## 四、Android SAF 与 iOS Files/Share Sheet 差异屏蔽

### 4.1 核心差异

| 维度 | Android SAF | iOS Files / Share Sheet |
|------|-------------|-------------------------|
| **文件访问** | Content URI (`content://...`) | File URL (`file://...`) |
| **权限模型** | 持久化 URI 权限 (`takePersistableUriPermission`) | Sandbox 内访问，外部文件需用户选择 |
| **文件选择** | `Intent.ACTION_OPEN_DOCUMENT` | `UIDocumentPickerViewController` |
| **分享** | `Intent.ACTION_SEND` | `UIActivityViewController` |
| **目录访问** | `Intent.ACTION_OPEN_DOCUMENT_TREE` | 不支持直接选择目录（iOS 13+ 可通过 `UIDocumentPickerViewController` 选择） |
| **文件监控** | `ContentResolver.registerContentObserver` | `NSFileCoordinator` + `NSFilePresenter` |

### 4.2 屏蔽策略

```rust
/// Android SAF 适配器
pub struct AndroidDocumentSource {
    context: jni::JNIEnv,
    content_resolver: jni::sys::jobject,
}

impl DocumentSource for AndroidDocumentSource {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        // path 实际上是 content URI 的字符串表示
        let uri_str = path.to_str().ok_or(Error::InvalidPath)?;
        
        // 通过 ContentResolver 读取
        let input_stream = self.content_resolver.call_method(
            "openInputStream",
            "(Landroid/net/Uri;)Ljava/io/InputStream;",
            &[JValue::Object(uri_str.into())]
        )?;
        
        // 读取 InputStream 到 Vec<u8>
        read_input_stream(&self.context, input_stream.l()?)
    }
}

/// iOS Files 适配器
pub struct IosDocumentSource;

impl DocumentSource for IosDocumentSource {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        // iOS 直接访问文件路径（在 sandbox 内）
        std::fs::read(path).map_err(Into::into)
    }
}
```

**关键设计：**
1. **统一 Path 抽象**：`Path` 在 Android 上是 content URI 的字符串表示，在 iOS 上是真实文件路径
2. **权限管理**：Android 适配器内部处理 URI 权限持久化，上层无感知
3. **错误映射**：平台特定错误统一映射为 `Error` 枚举

---

## 五、FFI 边界设计

### 5.1 C ABI 导出函数清单

```rust
// ===== 文档处理 =====

/// 解析 Markdown 文档
/// 
/// # Safety
/// - `input` 必须是有效的 UTF-8 字符串
/// - `input_len` 是字节长度（不含 null terminator）
/// - 返回的 `DocumentHandle` 必须通过 `mdlens_document_free` 释放
#[no_mangle]
pub unsafe extern "C" fn mdlens_parse(
    input: *const u8,
    input_len: usize,
) -> *mut DocumentHandle;

/// 分析文档可读性
/// 
/// # Safety
/// - `doc` 必须是由 `mdlens_parse` 返回的有效句柄
#[no_mangle]
pub unsafe extern "C" fn mdlens_analyze(
    doc: *const DocumentHandle,
) -> *mut AnalysisReportHandle;

/// 优化文档可读性
/// 
/// # Safety
/// - `doc` 必须是由 `mdlens_parse` 返回的有效句柄
/// - `report` 必须是由 `mdlens_analyze` 返回的有效句柄
/// - 返回的 `OptimizedDocumentHandle` 必须通过 `mdlens_optimized_free` 释放
#[no_mangle]
pub unsafe extern "C" fn mdlens_optimize(
    doc: *const DocumentHandle,
    report: *const AnalysisReportHandle,
) -> *mut OptimizedDocumentHandle;

/// 渲染优化后的文档为 Markdown
/// 
/// # Safety
/// - `optimized` 必须是由 `mdlens_optimize` 返回的有效句柄
/// - 返回的 `MarkdownString` 必须通过 `mdlens_string_free` 释放
#[no_mangle]
pub unsafe extern "C" fn mdlens_render(
    optimized: *const OptimizedDocumentHandle,
) -> *mut MarkdownString;

/// 生成差异报告
/// 
/// # Safety
/// - `original` 和 `optimized` 必须是有效的文档句柄
#[no_mangle]
pub unsafe extern "C" fn mdlens_diff(
    original: *const DocumentHandle,
    optimized: *const OptimizedDocumentHandle,
) -> *mut DiffReportHandle;

// ===== 内存管理 =====

#[no_mangle]
pub unsafe extern "C" fn mdlens_document_free(doc: *mut DocumentHandle);

#[no_mangle]
pub unsafe extern "C" fn mdlens_optimized_free(opt: *mut OptimizedDocumentHandle);

#[no_mangle]
pub unsafe extern "C" fn mdlens_string_free(s: *mut MarkdownString);

#[no_mangle]
pub unsafe extern "C" fn mdlens_analysis_free(report: *mut AnalysisReportHandle);

#[no_mangle]
pub unsafe extern "C" fn mdlens_diff_free(report: *mut DiffReportHandle);

// ===== 字符串访问 =====

/// 获取 Markdown 字符串的指针和长度
/// 
/// # Safety
/// - `s` 必须是由 `mdlens_render` 返回的有效句柄
/// - 返回的指针在 `mdlens_string_free` 调用前有效
#[no_mangle]
pub unsafe extern "C" fn mdlens_string_get(
    s: *const MarkdownString,
    out_ptr: *mut *const u8,
    out_len: *mut usize,
);
```

### 5.2 内存所有权约定

| 场景 | 所有权 | 释放责任 |
|------|--------|----------|
| 输入字符串 (`input`) | 调用方 | 调用方 |
| 返回的句柄 (`*mut XxxHandle`) | Core 分配 | 调用方通过 `xxx_free` 释放 |
| 返回的字符串 (`*mut MarkdownString`) | Core 分配 | 调用方通过 `mdlens_string_free` 释放 |
| 输出参数 (`out_ptr`, `out_len`) | 指向调用方内存 | 调用方管理 |

**关键原则：**
1. **Core 不持有外部内存**：所有输入数据在 FFI 边界处拷贝到 Core 内部
2. **句柄模式**：返回不透明指针，内部是 `Box<T>`
3. **显式释放**：每个 `xxx_new` / `xxx_create` 都有对应的 `xxx_free`
4. **无隐式生命周期**：句柄之间无依赖关系，可独立释放

### 5.3 字符串编码约定

| 约定 | 说明 |
|------|------|
| **输入编码** | UTF-8（强制） |
| **输出编码** | UTF-8（强制） |
| **字符串长度** | 字节长度（不含 null terminator） |
| **空字符串** | `ptr` 非 null，`len` = 0 |
| **null 处理** | 不允许 null 指针（用 `Option<PathBuf>` 等 Rust 类型在内部处理） |

### 5.4 换行符约定

| 约定 | 说明 |
|------|------|
| **输入换行** | 保留原始换行符（CRLF/LF/CR），在 `DocumentMetadata` 中记录 |
| **输出换行** | 默认 LF，可通过 `RenderOptions::line_ending` 配置 |
| **混合换行** | Core 内部统一为 LF，渲染时根据配置转换 |

---

## 六、数据流示例

### 6.1 完整处理流程

```
用户打开文件
    ↓
[Platform] FilePicker.pick_file()
    ↓
[Platform] DocumentSource.read(path) → Vec<u8>
    ↓
[Core] encoding_detect::detect(&bytes) → Encoding
    ↓
[Core] line_ending::detect(&bytes) → LineEnding
    ↓
[Core] front_matter::extract(&str) → (Option<FrontMatter>, &str)
    ↓
[Core] mdlens_parse(content) → DocumentHandle
    ↓
[Core] mdlens_analyze(doc) → AnalysisReportHandle
    ↓
[Core] mdlens_optimize(doc, report) → OptimizedDocumentHandle
    ↓
[Core] content_guard::verify(doc, optimized) → GuardResult
    ↓
[Core] mdlens_render(optimized) → MarkdownString
    ↓
[Core] line_ending::convert(&str, original_line_ending) → String
    ↓
[Core] front_matter::prepend(front_matter, &str) → String
    ↓
[Platform] DocumentStorage.write(path, bytes)
    ↓
完成
```

### 6.2 错误处理

```rust
#[repr(C)]
pub enum ErrorCode {
    Ok = 0,
    InvalidUtf8 = 1,
    ParseError = 2,
    AnalysisError = 3,
    OptimizationError = 4,
    RenderError = 5,
    ContentGuardFailed = 6,
    IoError = 7,
    NullPointer = 8,
}

/// 获取最后一次错误的错误码
#[no_mangle]
pub extern "C" fn mdlens_last_error() -> ErrorCode;

/// 获取最后一次错误的描述字符串
/// 
/// # Safety
/// - 返回的字符串在下次调用 Core 函数前有效
/// - 不需要释放（线程局部存储）
#[no_mangle]
pub extern "C" fn mdlens_last_error_message() -> *const c_char;
```

---

## 七、测试策略

### 7.1 单元测试

- 每个模块独立测试（`#[cfg(test)]`）
- 使用 corpus 文件进行集成测试
- 覆盖所有边界情况（空文件、超大文件、特殊字符等）

### 7.2 FFI 测试

- C 语言测试程序验证 FFI 边界
- 内存泄漏检测（Valgrind / AddressSanitizer）
- 跨语言调用测试（Python / Dart / Swift / Kotlin）

### 7.3 平台适配器测试

- Mock 平台接口进行单元测试
- 真机测试（Android / iOS）验证 SAF / Files 集成
- CI 覆盖主流平台（Windows / macOS / Linux）

---

## 八、性能目标

| 指标 | 目标 |
|------|------|
| 解析速度 | ≥ 10 MB/s（pulldown-cmark SIMD） |
| 分析速度 | ≥ 5 MB/s |
| 优化速度 | ≥ 2 MB/s |
| 内存占用 | ≤ 2x 原始文件大小 |
| FFI 开销 | < 1% 总处理时间 |

---

## 九、扩展性设计

### 9.1 插件系统（未来）

```rust
pub trait OptimizationPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn analyze(&self, doc: &Document) -> Vec<Finding>;
    fn optimize(&self, doc: &Document, findings: &[Finding]) -> Vec<Transformation>;
}

/// 注册自定义优化插件
#[no_mangle]
pub extern "C" fn mdlens_register_plugin(plugin: *mut OptimizationPlugin);
```

### 9.2 配置系统

```rust
pub struct OptimizationConfig {
    pub max_paragraph_length: usize,
    pub min_heading_level: u8,
    pub enable_list_reorganization: bool,
    pub enable_table_formatting: bool,
    pub preserve_html: bool,
    // ...
}
```

---

## 十、总结

本架构设计遵循以下原则：

1. **Core First**：所有平台共享 Rust 核心，平台特定代码最小化
2. **零拷贝 FFI**：通过句柄模式和显式内存管理，最小化跨边界开销
3. **内容不变**：`content_guard` 模块确保优化前后语义等价
4. **平台无关**：统一抽象层屏蔽 Android SAF / iOS Files 等差异
5. **可扩展**：插件系统和配置系统支持未来扩展

下一步：实现 Core 引擎骨架，优先完成 `markdown_parser` → `markdown_ast` → `document_model` 流水线。
