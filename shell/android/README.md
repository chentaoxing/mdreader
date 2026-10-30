# MD文档阅读助手 · Android

Windows 桌面版的手机端。同一个 Rust 核心（`mdcore-ffi`），同一个硬约束：
**只调排版、一个字的内容都不改**；`Cargo.lock` 与 `fingerprint-linux-baseline.txt`
是三个平台（Linux/Windows/Android）逐字节一致的证据链。

## 结构

```text
shell/android/
├── app/src/main/java/com/chentaoxing/mdreader/
│   ├── core/MdCore.kt        # JNI 入口（libmdcore_jni.so），信封 JSON 契约
│   ├── core/Models.kt        # Wire 解析：optimize/rebuild payload、u64 校验和字符串
│   ├── core/TextDecode.kt    # GB18030/UTF-16 内存转码（与 Windows 壳同一套顺序）
│   ├── render/ReadingView.kt # 阅读视图（纯显示层：标题节奏/列表缩进/表格/任务项/front matter）
│   └── MainActivity.kt       # SAF 打开/另存、预设菜单、改动勾选、三页签
└── app/src/main/jniLibs/     # cargo-ndk 产物（arm64-v8a / armeabi-v7a / x86_64）
```

## 构建

前置：JDK 17、Android SDK（API 35）+ NDK 28.2、Rust + cargo-ndk。

```bash
# 1. 原生库（三架构，产物落 jniLibs/）
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o shell/android/app/src/main/jniLibs \
  build --release -p mdcore-jni

# 2. APK（注意：AGP 拒绝非 ASCII 项目路径，中文 Windows 用户请先建 ASCII junction）
cd shell/android && gradle :app:assembleDebug
```

注意：
- AGP 会拒绝中文路径（`android.overridePathCheck=true` 可以跳过检查但 aapt2 仍会炸），
  在 `E:\Agent` 下建一个 ASCII junction 再构建（`New-Item -ItemType Junction`）。
- `jniLibs/` 的 .so 是构建产物但**随仓库分发**（clone 即可构建 APK，无需 Rust 工具链）；
  改了核心代码后按上面第 1 步重建。

## 验证

```bash
gradle :app:assembleDebugAndroidTest
adb shell am instrument -w -e class com.chentaoxing.mdreader.FingerprintParityTest \
  com.chentaoxing.mdreader.test/androidx.test.runner.AndroidJUnitRunner
```

`FingerprintParityTest` 是五平台一致性门的 Android 侧：33 份语料在设备上跑出的
输入正文校验和必须与桌面基线逐字节一致（API 35 x86_64 模拟器实测通过，2026-10-31）。

## 已知限制

- 阅读视图当前是「原样」档：`**加粗**` 按字面显示。语法符号弱化/隐藏两档
  （Windows 壳已有）是下一版的显示层工作。
- `--preset` 菜单切换会重跑优化；大批量勾选规则时逐条重建（与桌面交互一致）。
