#!/usr/bin/env bash
# Ported from bindings-release.yml / build-lib. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Install Linux audio deps
(
apt-get update
apt-get install -y libasound2-dev pkg-config

)
load_step_env

# Build librockbox_ffi
(
cargo build --release -p rockbox-ffi
)
load_step_env

# Stage binaries
(
mkdir -p out
# dynamic lib (Ruby/Python/npm dlopen it) + static archive (the Swift
# RockboxFFI product bundles it). Both fall out of the one build.
cp "target/release/${LIBFILE}" "out/${LIBFILE}"
cp "target/release/librockbox_ffi.a" "out/librockbox_ffi.a"

)
load_step_env

collect 'out/*'
