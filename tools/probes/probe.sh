#!/usr/bin/env bash
# 内容不变性探针：构造 Guard 可能漏掉的中文/边界语料，比较「check 的结论」与「真实字节差」
set -u
B=${B:?需要 B=mdfile-cli 路径}
W=${W:-/tmp/probe}
rm -rf "$W"; mkdir -p "$W"; cd "$W"
FWY='\343\200\200'   # U+3000 全角空格
NBSP='\302\240'      # U+00A0 不间断空格

printf '# T\n\n甲'"$FWY"'乙'"$FWY"'丙\n' > p1-mid-fullwidth.md
printf '# T\n\n第一段\n'"$FWY"'\n第二段\n' > p2-only-fullwidth.md
printf '# T\n\n行尾全角'"$FWY$FWY"'\n下一行\n' > p3-eol-fullwidth.md
printf -- '- [ ] 待办事项\n- [x] 已完成事项\n\n\n\n'"$FWY"'正文\n' > p4-tasklist.md
printf '正文有脚注[^1]标记。\n\n'"$FWY$FWY"'\n[^1]: 脚注内容\n' > p5-footnote.md
printf '# T\n\n正文\n\n\n' > p6-eof-blank.md
printf 'a\000b\000\n\000' > p7-utf16-ish.md
printf '# T\n\n正文'"$NBSP$NBSP"'\n' > p8-eol-nbsp.md
printf -- '- 项目一\n  - 子项\n\n\n\n'"$FWY"'中文缩进段\n' > p9-list-indent.md

for f in p*.md; do
  chk=$("$B" check "$f" 2>&1 | head -1)
  "$B" optimize "$f" --stdout > "$f.out" 2>"$f.err"
  bi=$(wc -c < "$f"); bo=$(wc -c < "$f.out")
  ndiff=$(cmp -l "$f" "$f.out" 2>/dev/null | wc -l)
  printf '%-24s 原%4s -> 优化%4s 字节 | cmp差异%3s\n' "$f" "$bi" "$bo" "$ndiff"
  printf '%-24s check: %s\n' "" "$chk"
  if [ "$ndiff" -gt 0 ]; then
    printf '%-24s 差异明细(偏移/原字节->新字节): %s\n' "" \
      "$(cmp -l "$f" "$f.out" 2>/dev/null | head -6 | awk '{printf "%s:%s->%s ", $1, $2, $3}')"
  fi
done
echo
echo "=== 逐例展示（原文 vs 优化后，od 前 3 行）==="
for f in p2-only-fullwidth.md p4-tasklist.md p5-footnote.md p8-eol-nbsp.md p7-utf16-ish.md; do
  echo "--- $f"; od -An -c "$f" | head -3; echo "    ->"; od -An -c "$f.out" | head -3
  head -2 "$f.err"
done
