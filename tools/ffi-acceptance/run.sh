#!/usr/bin/env bash
# 真实 C 客户端验收：编译 ffi_client_test.c 并链接本层产出的动态库。
# HANDOFF 声称「C 客户端已实测通过」但仓库里一直缺少这份代码，这里补上，任何平台都能自己跑一遍。
#
#   ./run.sh              # 用本机 gcc（Linux/macOS/MSYS2 都行）
#   ./run.sh docker       # 本机没有 Rust/gcc 时，用容器构建 + 验收
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
TARGET=${CARGO_TARGET_DIR:-$ROOT/target}

if [ "${1:-}" = "docker" ]; then
  MSYS_NO_PATHCONV=1 MSYS2_ARG_CONV_EXCL='*' docker run --rm \
    -v "$ROOT:/src" -v "$HERE:/here" \
    -e CARGO_TARGET_DIR=/tmp/target rust:1-slim bash -c '
      set -e; cd /src
      cargo build --release -p mdcore-ffi 2>&1 | tail -2
      gcc -std=c11 -I crates/mdcore-ffi/include /here/ffi_client_test.c \
          -L /tmp/target/release -lmdcore_ffi -Wl,-rpath,/tmp/target/release -o /tmp/ffitest
      for t in t1 t2 t3 t4 t5 t6; do echo "--- $t"; /tmp/ffitest "$t"; done
      echo "--- 释放「无错误」返回值（旧实现会段错误）"; /tmp/ffitest free_empty'
  exit $?
fi

cargo build --release -p mdcore-ffi
gcc -std=c11 -I "$ROOT/crates/mdcore-ffi/include" "$HERE/ffi_client_test.c" \
    -L "$TARGET/release" -lmdcore_ffi \
    -Wl,-rpath,"$TARGET/release" -o /tmp/ffitest
for t in t1 t2 t3 t4 t5 t6; do echo "--- $t"; /tmp/ffitest "$t"; done
echo "--- free_empty"; /tmp/ffitest free_empty; echo "exit=$?"
