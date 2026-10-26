#!/usr/bin/env bash
# Ported from linux-aarch64-check.yml / build. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Install dependencies
(
apt-get update
apt-get install -y \
  build-essential \
  libunwind-dev \
  libasound2-dev \
  libdbus-1-dev \
  protobuf-compiler \
  cmake

)
load_step_env

# Build Web UI
(
cd webui/rockbox
deno install
deno run build

)
load_step_env

# Install admin UI dependencies
(
cd "crates/s3/s3webui"
bun install --frozen-lockfile
)
load_step_env

# Build admin UI
(
cd "crates/s3/s3webui"
bun run build
)
load_step_env

# Rust flags
(
echo "RUSTFLAGS=-A warnings" >> $GITHUB_ENV
)
load_step_env

# Build rockboxd (headless)
(
bash scripts/build-headless.sh
)
load_step_env

# Build rockbox CLI
(
cargo build -p rockbox --release
)
load_step_env

# Show artifacts
(
ls -lh target/release/rockbox zig/zig-out/bin/rockboxd
)
load_step_env

collect target/release/rockbox zig/zig-out/bin/rockboxd
