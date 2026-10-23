#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NDK_ROOT="/opt/homebrew/share/android-commandlinetools/ndk"
NDK="$(find "$NDK_ROOT" -mindepth 1 -maxdepth 1 -type d -print | sort -V | tail -n 1)"
if [[ -z "$NDK" ]]; then
  echo "Android NDK not found under $NDK_ROOT" >&2
  exit 1
fi
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/darwin-x86_64/bin"
for tool in aarch64-linux-android24-clang x86_64-linux-android24-clang llvm-ar; do
  [[ -x "$TOOLCHAIN/$tool" ]] || { echo "Missing NDK tool: $TOOLCHAIN/$tool" >&2; exit 1; }
done

OUT="$ROOT/dist/core"
mkdir -p "$OUT/android/arm64-v8a" "$OUT/android/x86_64" "$OUT/apple/ios-device" \
  "$OUT/apple/ios-simulator-arm64" "$OUT/include"
cp ffi/include/rcode.h "$OUT/include/rcode.h"
cat > "$OUT/include/module.modulemap" <<'EOF'
module RCodeCore {
    header "rcode.h"
    export *
}
EOF

CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TOOLCHAIN/aarch64-linux-android24-clang" \
CARGO_TARGET_AARCH64_LINUX_ANDROID_AR="$TOOLCHAIN/llvm-ar" \
  cargo build -p rcode-ffi --release --target aarch64-linux-android
cp target/aarch64-linux-android/release/librcode_ffi.so "$OUT/android/arm64-v8a/librcode.so"

CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$TOOLCHAIN/x86_64-linux-android24-clang" \
CARGO_TARGET_X86_64_LINUX_ANDROID_AR="$TOOLCHAIN/llvm-ar" \
  cargo build -p rcode-ffi --release --target x86_64-linux-android
cp target/x86_64-linux-android/release/librcode_ffi.so "$OUT/android/x86_64/librcode.so"

cargo build -p rcode-ffi --release --target aarch64-apple-ios
cp target/aarch64-apple-ios/release/librcode_ffi.a "$OUT/apple/ios-device/librcode.a"
cargo build -p rcode-ffi --release --target aarch64-apple-ios-sim
cp target/aarch64-apple-ios-sim/release/librcode_ffi.a "$OUT/apple/ios-simulator-arm64/librcode.a"

if command -v xcodebuild >/dev/null 2>&1; then
  XCFRAMEWORK="$OUT/apple/RCode.xcframework"
  TEMP_XCFRAMEWORK="$OUT/apple/RCode.new.$$.xcframework"
  BACKUP_XCFRAMEWORK="$OUT/apple/RCode.previous.$$.xcframework"
  if [[ -e "$BACKUP_XCFRAMEWORK" ]]; then
    echo "Temporary backup already exists: $BACKUP_XCFRAMEWORK" >&2
    exit 1
  fi
  xcodebuild -create-xcframework \
    -library "$OUT/apple/ios-device/librcode.a" -headers "$OUT/include" \
    -library "$OUT/apple/ios-simulator-arm64/librcode.a" -headers "$OUT/include" \
    -output "$TEMP_XCFRAMEWORK"
  if [[ -e "$XCFRAMEWORK" ]]; then
    mv "$XCFRAMEWORK" "$BACKUP_XCFRAMEWORK"
  fi
  if ! mv "$TEMP_XCFRAMEWORK" "$XCFRAMEWORK"; then
    if [[ -e "$BACKUP_XCFRAMEWORK" ]]; then
      mv "$BACKUP_XCFRAMEWORK" "$XCFRAMEWORK"
    fi
    exit 1
  fi
  if [[ -e "$BACKUP_XCFRAMEWORK" ]]; then
    rm -rf "$BACKUP_XCFRAMEWORK"
  fi
else
  echo "xcodebuild unavailable; preserving verified Apple static libraries without XCFramework" >&2
fi
