#!/usr/bin/env bash
set -euo pipefail
source /ci/scripts/check-platform.sh
case "$PLATFORM" in
  linux/amd64) export ARCH=x86_64 ZIG_CPU=-Dcpu=x86_64 RUSTFLAGS='-C target-cpu=x86-64 -A warnings' ;;
  linux/arm64) export ARCH=aarch64 ZIG_CPU='' RUSTFLAGS='-A warnings' ;;
  *) echo "Unsupported platform: $PLATFORM" >&2; exit 2 ;;
esac
export OS_LABEL=linux TARGET="linux-$ARCH"
case "$JOB" in
  nif) export TARGET="$ARCH-linux-gnu" ;;
  embed) export ARCHIVE="librockboxd_${TAG}_${ARCH}-linux.tar.gz" ;;
  desktop) export ARCHIVE="rockbox-desktop-${TAG}-${TARGET}.tar.gz" ;;
esac
if [[ "$TOOLCHAIN" == android ]]; then
  source /ci/scripts/android-env.sh
  # Host CPU-specific flags must never affect Android target compilation.
  export RUSTFLAGS='-A warnings'
  [[ "$ARCHITECTURES" =~ ^(arm64-v8a|armeabi-v7a|x86_64)(,(arm64-v8a|armeabi-v7a|x86_64))*$ ]] || { echo 'Invalid Android architectures' >&2; exit 2; }
  case "$ABI" in
    arm64-v8a) export RUST_TARGET=aarch64-linux-android ;;
    x86_64) export RUST_TARGET=x86_64-linux-android ;;
    *) echo 'Supported FFI ABIs: arm64-v8a, x86_64' >&2; exit 2 ;;
  esac
fi
exec bash "/ci/scripts/$JOB.sh"
