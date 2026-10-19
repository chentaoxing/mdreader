# assets/brand —— 图标与品牌资产

一枚图标两个版本，墨蓝 `#0B3B5C` 圆角底 + 白色文档（右上折角）+ 向下箭头：

| 文件 | 用途 |
| --- | --- |
| `icon-master.png` (1024) | 细节版：文档里带 `#` 与正在被重新排齐的文本线。用于商店页、关于页、桌面大图 |
| `icon-simple.png` (1024) | 简化版：只有文档轮廓 + 箭头。小尺寸的母版 |
| `png/16…512.png` | 尺寸梯队。**≤64px 用简化版，≥128px 用细节版** |
| `app.ico` | Windows 图标。16/24/32/48 为 BMP 项（老 shell 也认），128/256 为 PNG 项 |
| `favicon.ico` | 16/32/48，给 `report` 生成的 HTML 与以后可能的站内页面用 |
| `brand-sheet.png` | 尺寸对照表，含深色（任务栏）与浅色（文件管理器）两种底的实际效果 |

## 它已经进了构建

`crates/mdfile-cli/build.rs` + `resources/mdfile.rc` 会把 `app.ico` 与版本信息编进 Windows exe。
`.rc` 里图标位置是占位符 `@MDFILE_ICON@`，由 build.rs 换成绝对路径后再交给 `windres` / `rc.exe` ——
因为 windres 按当前目录解析相对路径、rc.exe 按 .rc 所在目录解析，两边不一样。
资源编译器缺失时只发 `cargo:warning`，不会让构建失败。

换图只需要替换 `app.ico`（或换母版后重新导出），路径不用改。

## 还没做的

- macOS `.icns`、Android 自适应图标、iOS AppIcon 集：本机无 Xcode / Android 工具链。
  `png/` 那套尺寸是它们的直接输入（`iconutil`、Image Asset、Xcode AppIcon）。
- 母版来自 AI 生成，原图角标已通过"按蓝底投影裁到图块边界"去掉；
  正式商用前建议改画矢量版（位图缩放到 16px 有轻微毛刺）。

命名与查重见交付包根目录的 `核名与图标-2026-10-08.md`。
