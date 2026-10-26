#!/usr/bin/env bash
# Ported from release-embed-lib.yml / build. Publishing stays in Actions.
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
  protobuf-compiler \
  cmake

)
load_step_env

# Set Rust flags
(
FLAGS="${RUSTFLAGS}"
echo "RUSTFLAGS=$FLAGS" >> $GITHUB_ENV

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

# Build headless firmware
(
bash scripts/build-headless.sh
)
load_step_env

# Build rockbox-embed
(
rm -f target/release/librockbox_embed.a
cargo build --release -p rockbox-embed
echo "--- verify rb_daemon_start is present ---"
nm --defined-only target/release/librockbox_embed.a 2>/dev/null \
  | grep rb_daemon_start \
  || { echo "rb_daemon_start NOT in librockbox_embed.a — abort"; exit 1; }

)
load_step_env

# Build librockboxd.a
(
rm -f zig/zig-out/lib/librockboxd.a
cd zig
zig build lib -Doptimize=ReleaseFast ${ZIG_CPU}
cd ..
echo "--- verify rb_daemon_start survived into librockboxd.a ---"
nm --defined-only zig/zig-out/lib/librockboxd.a 2>/dev/null \
  | grep rb_daemon_start \
  || { echo "rb_daemon_start NOT in librockboxd.a — abort"; exit 1; }

)
load_step_env

# Clean build artifacts
(
cp zig/zig-out/lib/librockboxd.a /tmp/librockboxd.a
rm -rf target build-lib build-headless zig/.zig-cache
mkdir -p zig/zig-out/lib
mv /tmp/librockboxd.a zig/zig-out/lib/librockboxd.a

)
load_step_env

# Package archive
(
mkdir -p dist/lib dist/include
cp zig/zig-out/lib/librockboxd.a dist/lib/
cp include/rockboxd.h             dist/include/
ARCHIVE="${ARCHIVE}"
tar -C dist -czf "$ARCHIVE" lib include
if command -v sha256sum &>/dev/null; then
  sha256sum "$ARCHIVE" > "${ARCHIVE}.sha256"
else
  shasum -a 256 "$ARCHIVE" > "${ARCHIVE}.sha256"
fi
echo "Archive contents:"
tar -tzf "$ARCHIVE"

)
load_step_env

collect '*.tar.gz*'
