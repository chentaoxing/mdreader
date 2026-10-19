#!/usr/bin/env bash
# 在容器内跑：cargo test + 构建 + 指纹对账 + C 客户端验收（一条命令完成全链验证）
set -uo pipefail
SRC=${SRC:?}; WB=${WB:?}
export CARGO_TARGET_DIR=/tmp/target
rm -rf /tmp/app && cp -r "$SRC/mdfile-assistant" /tmp/app
cd /tmp/app
echo "===== rustc ====="; rustc --version
echo "===== cargo test --release ====="
cargo test --release 2>&1 | tail -40
echo "===== 构建产物 ====="
cargo build --release -p mdcore-ffi -p mdfile-cli 2>&1 | tail -3
echo "===== check 33 份语料 ====="
/tmp/target/release/mdfile-cli check tests/corpus/*.md 2>&1 | tail -3
echo "===== fingerprint 对账 ====="
/tmp/target/release/mdfile-cli fingerprint tests/corpus > /tmp/mine.txt 2>/tmp/fp.err; echo "exit=$?"
cat /tmp/fp.err
if diff -q /tmp/mine.txt "$SRC/mdfile-assistant/fingerprint-linux-baseline.txt" >/dev/null; then
  echo "RESULT: 指纹与云端基线逐字节一致（本机 rustc 版本见上）"
else
  echo "RESULT: 与基线不一致，差异如下"; diff /tmp/mine.txt "$SRC/mdfile-assistant/fingerprint-linux-baseline.txt" | head -20
fi
echo "===== CLI 冒烟：曾经崩溃/误用参数的写法 ====="
for c in "" "--help" "rules" "-h"; do
  /tmp/target/release/mdfile-cli $c >/dev/null 2>&1; echo "  args=[$c] exit=$?"
done
/tmp/target/release/mdfile-cli check tests/corpus/01-readme-nodejs.md --newline lf 2>&1 | tail -2
/tmp/target/release/mdfile-cli preview tests/corpus/05-readme-linux.md --rule R01 2>&1 | head -3
/tmp/target/release/mdfile-cli fingerprint tests/corpus/05-readme-linux.md --preset 技术文档 2>&1 | tail -1
echo "===== C ABI 验收 ====="
gcc -std=c11 -I /tmp/app/crates/mdcore-ffi/include "$WB/ffi_c_test.c" \
    -L /tmp/target/release -lmdcore_ffi -Wl,-rpath,/tmp/target/release -o /tmp/ffitest 2>&1 | head -20
for t in t1 t2 t3 t4 t5 t6; do echo "--- $t"; /tmp/ffitest "$t" 2>&1 | tail -18; done
echo "--- free_empty"; /tmp/ffitest free_empty 2>&1; echo "exit=$?"
echo "--- double_free"; /tmp/ffitest double_free 2>&1; echo "exit=$?"
echo "===== 中文/边界探针复跑 ====="
B=/tmp/target/release/mdfile-cli bash "$WB/probe.sh" 2>&1 | head -16
B=/tmp/target/release/mdfile-cli bash "$WB/probe2.sh" 2>&1 | sed -n '/x1/,/x4/p' | head -40
