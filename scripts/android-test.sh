#!/usr/bin/env bash
# Rust unit-testlarini (golden testi bilan birga) ulangan Android qurilma yoki emulatorda ishga tushiradi.
#   scripts/android-test.sh [aarch64|armv7|x86_64] [test filtri]
# Bir nechta qurilma ulangan bo'lsa: ANDROID_SERIAL=<serial> scripts/android-test.sh ...
# Talab: NDK (ANDROID_HOME/ndk/* yoki NDK_HOME), rustup target, modellar (scripts/fetch-models.sh).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAURI="$ROOT/frontend/src-tauri"
ARCH="${1:-aarch64}"
FILTER="${2:-}"
API=24

case "$ARCH" in
  aarch64) TARGET=aarch64-linux-android; CLANG=aarch64-linux-android$API-clang ;;
  armv7)   TARGET=armv7-linux-androideabi; CLANG=armv7a-linux-androideabi$API-clang ;;
  x86_64)  TARGET=x86_64-linux-android; CLANG=x86_64-linux-android$API-clang ;;
  *) echo "noma'lum arxitektura: $ARCH (aarch64 | armv7 | x86_64)" >&2; exit 2 ;;
esac

SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK="${NDK_HOME:-$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)}"
BIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
[[ -x "$BIN/$CLANG" ]] || { echo "NDK topilmadi: $BIN/$CLANG" >&2; exit 1; }

# cc crate (tract-linalg) va rustc uchun NDK kompilyatori
T_UP="$(echo "$TARGET" | tr 'a-z-' 'A-Z_')"
T_LO="${TARGET//-/_}"
export "CARGO_TARGET_${T_UP}_LINKER=$BIN/$CLANG"
export "CC_${T_LO}=$BIN/$CLANG"
export "CXX_${T_LO}=$BIN/$CLANG++"
export "AR_${T_LO}=$BIN/llvm-ar"

echo "== yig'ish: $TARGET"
EXE="$(cd "$TAURI" && cargo test --lib --no-run --target "$TARGET" --message-format=json 2>/dev/null \
  | python3 -c 'import sys,json
for l in sys.stdin:
    m=json.loads(l)
    if m.get("reason")=="compiler-artifact" and m.get("executable") and m["target"]["kind"]==["staticlib","cdylib","rlib"]: print(m["executable"])' | tail -1)"
[[ -n "$EXE" ]] || { echo "test binarini yig'ib bo'lmadi; xatoni ko'rish uchun:" >&2
  echo "  (cd $TAURI && cargo test --lib --no-run --target $TARGET)" >&2; exit 1; }

DEV=/data/local/tmp/khoface-test
echo "== qurilma: $(adb shell getprop ro.product.model | tr -d '\r'), Android $(adb shell getprop ro.build.version.release | tr -d '\r'), ABI $(adb shell getprop ro.product.cpu.abi | tr -d '\r')"
adb shell "rm -rf $DEV && mkdir -p $DEV/models $DEV/fixtures"
adb push -q "$EXE" "$DEV/tests"
adb push -q "$TAURI"/resources/models/*.onnx "$DEV/models/"
adb push -q "$TAURI"/tests/fixtures/. "$DEV/fixtures/"

echo "== testlar"
set +e
adb shell "cd $DEV && chmod 755 tests && KHOFACE_MODELS=$DEV/models KHOFACE_FIXTURES=$DEV/fixtures KHOFACE_REQUIRE_MODELS=1 TMPDIR=$DEV ./tests --test-threads=1 --nocapture $FILTER; echo \"exit=\$?\"" \
  | tee /dev/stderr | grep -q '^exit=0'
rc=$?
set -e
adb shell "rm -rf $DEV"
exit $rc
