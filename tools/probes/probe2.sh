#!/usr/bin/env bash
# 第二轮探针：把审阅提出的每个"Guard 可能漏掉"的触发样本跑一遍，看结论与真实字节/语义
set -u
B=${B:?}; W=${W:-/tmp/probe2}
rm -rf "$W"; mkdir -p "$W"; cd "$W"
FWY='\343\200\200'

show() {  # $1=文件
  local f=$1
  echo "===== $f ====="
  echo "--- 原文:"; od -An -c "$f"
  "$B" check "$f" 2>&1 | sed -n '1p'
  "$B" optimize "$f" --stdout > "$f.out" 2>"$f.err"
  echo "--- 优化后:"; od -An -c "$f.out"
  echo "--- diff 分类:"; "$B" diff "$f" 2>&1 | sed -n '1,12p'
  echo "--- 字节数: $(wc -c < "$f") -> $(wc -c < "$f.out")"
}

# X1 R09：段落下的 *** 被规范化后变成 setext 下划线 -> 段落被"读成"标题
printf '文字\n***\n正文\n' > x1-thematic-break.md
# X2 R05：多行 setext 标题只改最后一行 -> 前一行被降级成段落
printf '标题一\n标题二\n===\n\n正文\n' > x2-multiline-setext.md
# X3 TOML front matter（+++ 不被识别）
printf '+++\ntitle = "说明：甲、乙、丙"\n\n\n\na = 1\n+++\n\n正文\n' > x3-toml-frontmatter.md
# X4 行尾全角空格（debug_assert 声称只允许空格/制表）
printf '正文'"$FWY"'\n' > x4-eol-fullwidth.md
# X5 只含空格的空行是否被压缩
printf '正文\n   \n   \n   \n段落\n' > x5-space-only-blanks.md
# X6 YAML front matter + 后面紧跟内容（无空行）
printf -- '---\ntitle: 说明：甲、乙\n---\n正文\n' > x6-yaml-frontmatter.md
# X7 缩进代码块里的中文（protected?）
printf '# T\n\n    \344\273\243\347\240\201   \n    \346\255\243\346\226\207\n' > x7-indented-code.md
# X8 表格里的全角空格
printf '| A | B |\n|---|---|\n| \346\226\207"$FWY"\345\214\226 | x |\n' > x8-table-fullwidth.md

for f in x*.md; do show "$f"; done
