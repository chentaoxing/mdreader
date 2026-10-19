# DETERMINISM.md — 五平台输出一致性与 Unicode 风险清单

> 版本：1.0 (2026-10-05)  
> 目标：确保「MD文档阅读助手」在 Windows/macOS/Linux/Android/iOS 五平台输出完全一致

---

## 一、核心原则

**最高原则：内容不变，只改善阅读方式**

这意味着：
1. **语义等价**：优化前后的 Markdown 渲染结果必须完全相同
2. **字节级可预测**：相同输入在所有平台产生相同输出（字节级）
3. **可逆性**：用户应能理解优化前后的差异（通过 diff）

**挑战**：Unicode、换行符、编码等在不同平台有不同处理，必须严格管控。

---

## 二、Unicode 规范化（Normalization）风险

### 2.1 NFC vs NFD 问题

**背景：**
- **NFC** (Normalization Form C)：组合字符（如 `é` = U+00E9）
- **NFD** (Normalization Form D)：分解字符（如 `é` = `e` + `◌́` = U+0065 U+0301）

**风险：**
1. **macOS 文件系统**：默认使用 NFD（HFS+/APFS）
2. **Linux/Windows**：通常使用 NFC
3. **用户输入**：可能混合 NFC/NFD（从不同来源复制粘贴）

**绝对不能自动应用 NFC/NFD 的原因：**
1. **内容不变原则**：自动规范化会改变字节内容，违反最高原则
2. **用户意图**：用户可能有意使用特定形式（如 NFD 用于某些语言处理）
3. **不可逆性**：一旦规范化，无法恢复原始形式
4. **差异检测困难**：NFC/NFD 在视觉上相同，但字节不同，diff 会显示大量"假变化"

**示例：**
```markdown
# café (NFC: U+0063 U+0061 U+0066 U+00E9)
# café (NFD: U+0063 U+0061 U+0066 U+0065 U+0301)
```
视觉上完全相同，但字节不同。如果自动转换为 NFC，用户会看到"无意义的修改"。

### 2.2 应对策略

**Core 层处理：**
1. **不主动规范化**：保留原始 Unicode 形式
2. **比较时规范化**：在 `content_guard` 和 `content_fingerprint` 中，比较前统一转换为 NFC
3. **记录原始形式**：在 `DocumentMetadata` 中记录原始 Unicode 形式

**代码示例：**
```rust
pub struct DocumentMetadata {
    pub original_encoding: Encoding,
    pub original_line_ending: LineEnding,
    pub unicode_form: UnicodeForm,  // NFC / NFD / Mixed
    // ...
}

impl ContentGuard {
    pub fn verify(&self, original: &Document, optimized: &Document) -> GuardResult {
        // 比较时统一转换为 NFC
        let original_nfc = original.content.nfc().collect::<String>();
        let optimized_nfc = optimized.content.nfc().collect::<String>();
        
        if original_nfc != optimized_nfc {
            GuardResult::SemanticChanged {
                diff: compute_diff(&original_nfc, &optimized_nfc),
            }
        } else {
            GuardResult::Ok
        }
    }
}
```

**测试策略：**
- 使用 corpus 文件 `21-emoji-unicode.md` 测试 NFC/NFD 混合场景
- 验证优化前后 Unicode 形式保持不变
- 验证 `content_guard` 能正确检测语义变化（即使 Unicode 形式不同）

---

## 三、换行符（Line Ending）处理

### 3.1 平台差异

| 平台 | 默认换行符 | 说明 |
|------|-----------|------|
| Windows | CRLF (`\r\n`) | 历史原因（CP/M） |
| Linux/macOS | LF (`\n`) | Unix 传统 |
| 旧 macOS | CR (`\r`) | 已废弃（OS X 之前） |

**风险：**
1. **Git 自动转换**：`core.autocrlf` 可能导致意外转换
2. **跨平台协作**：团队成员使用不同平台，换行符混乱
3. **渲染差异**：某些 Markdown 渲染器对换行符敏感（如硬换行 vs 软换行）

### 3.2 应对策略

**Core 层处理：**
1. **检测原始换行符**：在读取文件时检测主要换行符类型
2. **内部统一为 LF**：Core 内部处理统一使用 LF
3. **输出时恢复**：渲染时根据原始换行符类型转换

**代码示例：**
```rust
pub enum LineEnding {
    LF,      // \n
    CRLF,    // \r\n
    CR,      // \r (旧 Mac)
    Mixed,   // 混合
}

pub fn detect_line_ending(content: &str) -> LineEnding {
    let crlf_count = content.matches("\r\n").count();
    let lf_count = content.matches('\n').count() - crlf_count;
    let cr_count = content.matches('\r').count() - crlf_count;
    
    if crlf_count > lf_count && crlf_count > cr_count {
        LineEnding::CRLF
    } else if lf_count > cr_count {
        LineEnding::LF
    } else if cr_count > 0 {
        LineEnding::CR
    } else {
        LineEnding::LF  // 默认
    }
}

pub fn convert_line_ending(content: &str, target: LineEnding) -> String {
    match target {
        LineEnding::LF => content.replace("\r\n", "\n").replace('\r', "\n"),
        LineEnding::CRLF => {
            content
                .replace("\r\n", "\n")  // 先统一为 LF
                .replace('\r', "\n")
                .replace('\n', "\r\n")  // 再转换为 CRLF
        }
        LineEnding::CR => {
            content
                .replace("\r\n", "\r")
                .replace('\n', "\r")
        }
        LineEnding::Mixed => content.to_string(),  // 保留混合
    }
}
```

**测试策略：**
- 使用 corpus 文件 `29-crlf-line-endings.md`、`30-crlf-line-endings-2.md`、`31-cr-line-endings.md` 测试
- 验证优化后换行符与原始一致
- 验证跨平台输出一致性（相同输入 → 相同输出）

---

## 四、BOM（Byte Order Mark）处理

### 4.1 背景

**BOM** 是 UTF-8/UTF-16 文件开头的特殊标记：
- **UTF-8 BOM**：`EF BB BF`（3 字节）
- **UTF-16 LE BOM**：`FF FE`
- **UTF-16 BE BOM**：`FE FF`

**风险：**
1. **Windows 编辑器**：某些 Windows 编辑器（如 Notepad）默认添加 UTF-8 BOM
2. **Unix 工具**：某些 Unix 工具不识别 BOM，导致问题（如 shell 脚本 `#!/bin/bash` 前有 BOM 会失败）
3. **Web 服务器**：某些 Web 服务器对 BOM 敏感

### 4.2 应对策略

**Core 层处理：**
1. **检测 BOM**：读取文件时检测 BOM
2. **保留 BOM**：如果原始文件有 BOM，输出时保留
3. **不主动添加**：不主动为无 BOM 文件添加 BOM

**代码示例：**
```rust
pub struct DocumentMetadata {
    pub has_bom: bool,
    pub encoding: Encoding,
    // ...
}

pub fn detect_bom(bytes: &[u8]) -> (bool, Encoding) {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        (true, Encoding::Utf8)
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        (true, Encoding::Utf16Le)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        (true, Encoding::Utf16Be)
    } else {
        (false, Encoding::Utf8)  // 默认 UTF-8 无 BOM
    }
}

pub fn write_with_bom(content: &str, has_bom: bool) -> Vec<u8> {
    let mut bytes = if has_bom {
        vec![0xEF, 0xBB, 0xBF]
    } else {
        vec![]
    };
    bytes.extend_from_slice(content.as_bytes());
    bytes
}
```

**测试策略：**
- 使用 corpus 文件 `32-utf8-bom.md` 测试
- 验证优化后 BOM 保留
- 验证无 BOM 文件不会被添加 BOM

---

## 五、Emoji 与代理对（Surrogate Pairs）/ 字素簇（Grapheme Clusters）

### 5.1 背景

**Emoji 和复杂 Unicode 字符** 涉及多个概念：
1. **代理对**：UTF-16 编码中，超出 BMP（Basic Multilingual Plane）的字符用两个 16 位单元表示
2. **字素簇**：用户感知的"一个字符"，可能由多个 Unicode 码位组成

**示例：**
- 👨‍👩‍👧‍👦 (family) = 7 个码位，1 个字素簇
- 🏳️‍🌈 (rainbow flag) = 4 个码位，1 个字素簇
- 🇺🇸 (US flag) = 2 个区域指示符，1 个字素簇

### 5.2 风险

1. **字符串操作**：按字节或码位操作可能破坏字素簇
2. **长度计算**：`String::len()` 返回字节数，不是字素簇数
3. **截断风险**：截断字符串时可能切断字素簇

**示例：**
```rust
let family = "👨‍👩‍👧‍👦";
println!("{}", family.len());  // 25 字节
println!("{}", family.chars().count());  // 7 码位
println!("{}", family.graphemes(true).count());  // 1 字素簇
```

### 5.3 应对策略

**Core 层处理：**
1. **使用字素簇感知操作**：所有字符串操作使用 `unicode-segmentation` crate
2. **不截断字素簇**：截断时确保不切断字素簇
3. **长度计算**：使用字素簇数而非字节数

**代码示例：**
```rust
use unicode_segmentation::UnicodeSegmentation;

pub fn safe_truncate(content: &str, max_graphemes: usize) -> String {
    content
        .graphemes(true)
        .take(max_graphemes)
        .collect::<String>()
}

pub fn grapheme_length(content: &str) -> usize {
    content.graphemes(true).count()
}
```

**测试策略：**
- 使用 corpus 文件 `21-emoji-unicode.md` 测试
- 验证 Emoji 和复杂字符不被破坏
- 验证截断操作不切断字素簇

---

## 六、平台路径大小写敏感性

### 6.1 背景

| 平台 | 路径大小写敏感 | 说明 |
|------|:-------------:|------|
| Windows | ❌ 不敏感 | `README.md` == `readme.md` |
| macOS | ❌ 默认不敏感 | HFS+/APFS 默认不敏感（可配置） |
| Linux | ✅ 敏感 | `README.md` != `readme.md` |

### 6.2 风险

1. **文件查找**：在 Windows 上查找 `readme.md` 可能匹配 `README.md`
2. **跨平台协作**：Git 仓库中同时存在 `README.md` 和 `readme.md` 会导致问题
3. **路径比较**：直接比较路径字符串可能出错

### 6.3 应对策略

**Core 层处理：**
1. **不假设大小写**：路径比较使用平台特定方法
2. **规范化路径**：在比较前规范化路径（使用 `std::fs::canonicalize`）
3. **记录原始路径**：保留用户输入的原始路径

**代码示例：**
```rust
pub fn normalize_path(path: &Path) -> Result<PathBuf> {
    // 规范化路径（解析符号链接、相对路径等）
    std::fs::canonicalize(path).map_err(Into::into)
}

pub fn paths_equal(a: &Path, b: &Path) -> bool {
    // 规范化后比较
    match (normalize_path(a), normalize_path(b)) {
        (Ok(a_norm), Ok(b_norm)) => a_norm == b_norm,
        _ => a == b,  // 降级为字符串比较
    }
}
```

**测试策略：**
- 测试不同平台的路径处理
- 验证路径比较正确性
- 验证符号链接处理

---

## 七、综合测试策略

### 7.1 单元测试

**每个风险点独立测试：**
1. **Unicode 规范化**：NFC/NFD 混合文件
2. **换行符**：CRLF/LF/CR 文件
3. **BOM**：有 BOM 和无 BOM 文件
4. **Emoji**：复杂 Emoji 和字素簇
5. **路径**：大小写敏感和不敏感场景

### 7.2 集成测试

**使用 corpus 文件进行端到端测试：**
```rust
#[test]
fn test_cross_platform_consistency() {
    let corpus_dir = Path::new("/data/mdlens/tests/corpus");
    
    for entry in fs::read_dir(corpus_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().map_or(false, |ext| ext == "md") {
            let content = fs::read_to_string(&path).unwrap();
            
            // 在 Core 中处理
            let doc = mdlens_parse(&content);
            let report = mdlens_analyze(&doc);
            let optimized = mdlens_optimize(&doc, &report);
            let result = mdlens_render(&optimized);
            
            // 验证内容不变
            let guard_result = content_guard::verify(&content, &result);
            assert!(guard_result.is_ok());
            
            // 验证元信息保留
            assert_eq!(doc.metadata.line_ending, optimized.metadata.line_ending);
            assert_eq!(doc.metadata.has_bom, optimized.metadata.has_bom);
        }
    }
}
```

### 7.3 跨平台 CI 测试

**GitHub Actions 配置：**
```yaml
name: Cross-Platform Tests

on: [push, pull_request]

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    
    runs-on: ${{ matrix.os }}
    
    steps:
      - uses: actions/checkout@v3
      
      - name: Run tests
        run: cargo test
      
      - name: Run corpus tests
        run: cargo test --test corpus_tests
```

### 7.4 模糊测试（Fuzzing）

**使用 `cargo-fuzz` 进行模糊测试：**
```rust
#[cfg(fuzzing)]
fn fuzz_target(data: &[u8]) {
    if let Ok(content) = std::str::from_utf8(data) {
        let doc = mdlens_parse(content);
        let report = mdlens_analyze(&doc);
        let optimized = mdlens_optimize(&doc, &report);
        let result = mdlens_render(&optimized);
        
        // 验证不崩溃
        let guard_result = content_guard::verify(content, &result);
        // 不验证结果，只验证不崩溃
    }
}
```

---

## 八、风险优先级

| 风险 | 严重性 | 发生概率 | 优先级 |
|------|:------:|:-------:|:------:|
| Unicode 规范化自动应用 | 高 | 中 | **P0** |
| 换行符转换错误 | 中 | 高 | **P0** |
| BOM 丢失或误添加 | 中 | 中 | **P1** |
| Emoji/字素簇破坏 | 高 | 低 | **P1** |
| 路径大小写问题 | 中 | 低 | **P2** |

---

## 九、总结

**核心原则：**
1. **不主动改变内容**：保留原始 Unicode 形式、换行符、BOM
2. **比较时规范化**：在 `content_guard` 中统一转换为 NFC 比较
3. **字素簇感知**：所有字符串操作使用字素簇而非字节/码位
4. **平台感知**：路径处理使用平台特定方法

**测试策略：**
1. **单元测试**：覆盖每个风险点
2. **集成测试**：使用 32 个 corpus 文件端到端测试
3. **跨平台 CI**：在 Windows/macOS/Linux 上运行测试
4. **模糊测试**：发现边界情况

**下一步：**
- 实现 `unicode_utils` 模块（NFC/NFD 检测、字素簇操作）
- 实现 `line_ending` 模块（检测、转换）
- 实现 `encoding_detect` 模块（BOM 检测）
- 编写 corpus 集成测试
