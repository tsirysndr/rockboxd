#!/usr/bin/env bash
# Ported from release-slint-desktop.yml / build. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Install Linux dependencies
(
apt-get update
apt-get install -y \
  build-essential \
  libunwind-dev \
  libasound2-dev \
  libdbus-1-dev \
  libfontconfig1-dev \
  libxkbcommon-dev \
  protobuf-compiler \
  curl \
  wget \
  zip \
  unzip \
  cmake

)
load_step_env

# Set Rust flags
(
echo "RUSTFLAGS=-A warnings" >> $GITHUB_ENV
)
load_step_env

# Build Web UI
(
cd webui/rockbox
deno install
deno run build
rm -r node_modules

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
rm -r node_modules

)
load_step_env

# Build headless firmware
(
bash scripts/build-headless.sh
)
load_step_env

# Build rockbox-embed
(
rm -f target/release/librockbox_embed.a
cargo build --release --locked -p rockbox-embed
nm --defined-only target/release/librockbox_embed.a 2>/dev/null \
  | grep rb_daemon_start \
  || { echo "rb_daemon_start NOT in librockbox_embed.a — abort"; exit 1; }

)
load_step_env

# Build librockboxd.a
(
rm -f zig/zig-out/lib/librockboxd.a
cd zig
zig build lib -Doptimize=ReleaseFast
cd ..
nm --defined-only zig/zig-out/lib/librockboxd.a 2>/dev/null \
  | grep rb_daemon_start \
  || { echo "rb_daemon_start NOT in librockboxd.a — abort"; exit 1; }

)
load_step_env

# Build desktop app
(
cd "desktop"
cargo build --release --locked
)
load_step_env

# Package (Linux tarball)
(
STAGE=rockbox-desktop-${TAG}-${TARGET}
mkdir -p "$STAGE"
cp desktop/target/release/rockbox-desktop "$STAGE/"
cp desktop/README.md "$STAGE/"
cp -r desktop/skins "$STAGE/skins"
tar -czf "${ARCHIVE}" "$STAGE"

)
load_step_env

collect '*.tar.gz'
