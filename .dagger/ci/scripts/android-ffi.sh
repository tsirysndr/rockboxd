#!/usr/bin/env bash
# Ported from bindings-release.yml / build-lib-android. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Install oboe-sys build deps (libclang for bindgen)
(
apt-get update
apt-get install -y clang libclang-dev

)
load_step_env

# Install cargo-ndk
(
cargo install cargo-ndk --locked
)
load_step_env

# Build librockbox_ffi.so (matrix target)
(
cargo ndk -t ${ABI} --platform 26 -o jniout \
  build --release -p rockbox-ffi

)
load_step_env

# Stage binary
(
mkdir -p out
cp "jniout/${ABI}/librockbox_ffi.so" out/librockbox_ffi.so

)
load_step_env

collect 'out/*'
