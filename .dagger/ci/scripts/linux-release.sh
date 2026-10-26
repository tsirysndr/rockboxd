#!/usr/bin/env bash
# Ported from ci.yml / publish. Publishing stays in Actions.
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
  curl \
  wget \
  zip \
  unzip \
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
echo "RUSTFLAGS=-C target-cpu=x86-64 -A warnings" >> $GITHUB_ENV
)
load_step_env

# Build rockboxd (headless)
(
export TAG="${TAG}"
bash scripts/build-headless.sh
)
load_step_env

# Build rockbox CLI
(
export TAG="${TAG}"
cargo build -p rockbox --release
)
load_step_env

# Clean build artifacts
(
mkdir -p /tmp/rb-bins
cp target/release/rockbox   /tmp/rb-bins/
cp zig/zig-out/bin/rockboxd /tmp/rb-bins/
rm -rf target build-lib build-headless zig/.zig-cache
mkdir -p target/release zig/zig-out/bin
mv /tmp/rb-bins/rockbox   target/release/rockbox
mv /tmp/rb-bins/rockboxd  zig/zig-out/bin/rockboxd

)
load_step_env

# Package binaries
(
VERSION="${TAG}"
cp target/release/rockbox .
cp zig/zig-out/bin/rockboxd .
tar czvf rockbox_${VERSION}_x86_64-linux.tar.gz rockbox rockboxd
sha256sum rockbox_${VERSION}_x86_64-linux.tar.gz \
  > rockbox_${VERSION}_x86_64-linux.tar.gz.sha256

)
load_step_env

collect '*.tar.gz*'
