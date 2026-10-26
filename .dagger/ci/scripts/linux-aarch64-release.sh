#!/usr/bin/env bash
# Ported from linux-aarch64-build.yml / publish. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh
export TYPESENSE_VERSION=30.1

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
  cmake \
  dpkg-dev

)
load_step_env

# Build Web UI
(
cd webui/rockbox
deno install
deno run build

)
load_step_env

# Rust flags
(
echo "RUSTFLAGS=-A warnings" >> $GITHUB_ENV
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

# Build rockbox-embed
(
cargo build --release -p rockbox-embed
)
load_step_env

# Build librockboxd.a
(
cd zig && zig build lib -Doptimize=ReleaseFast
)
load_step_env

# Package binaries
(
VERSION="${TAG}"
cp target/release/rockbox .
cp zig/zig-out/bin/rockboxd .
tar czvf rockbox_${VERSION}_aarch64-linux.tar.gz rockbox rockboxd
sha256sum rockbox_${VERSION}_aarch64-linux.tar.gz \
  > rockbox_${VERSION}_aarch64-linux.tar.gz.sha256

)
load_step_env

# Package librockboxd.a
(
VERSION="${TAG}"
mkdir -p dist/lib dist/include
cp zig/zig-out/lib/librockboxd.a dist/lib/
cp include/rockboxd.h             dist/include/
ARCHIVE="librockboxd_${VERSION}_aarch64-linux.tar.gz"
tar -C dist -czf "$ARCHIVE" lib include
sha256sum "$ARCHIVE" > "${ARCHIVE}.sha256"

)
load_step_env

# Download typesense-server
(
set -euo pipefail
url="https://dl.typesense.org/releases/${TYPESENSE_VERSION}/typesense-server-${TYPESENSE_VERSION}-linux-arm64.tar.gz"
curl -fsSL "$url" -o /tmp/typesense.tar.gz
mkdir -p /tmp/typesense
tar -xzf /tmp/typesense.tar.gz -C /tmp/typesense
install -m 0755 /tmp/typesense/typesense-server /tmp/typesense-server

)
load_step_env

# Build .deb (arm64)
(
set -euo pipefail
VERSION="${TAG}"
pkg_root="dist/debian/arm64"
mkdir -p "$pkg_root/usr/local/bin"
install -m 0755 target/release/rockbox    "$pkg_root/usr/local/bin/rockbox"
install -m 0755 zig/zig-out/bin/rockboxd  "$pkg_root/usr/local/bin/rockboxd"
install -m 0755 /tmp/typesense-server     "$pkg_root/usr/local/bin/typesense-server"
sed -i "s/^Version: .*/Version: ${VERSION}/" "$pkg_root/DEBIAN/control"
dpkg-deb --build --root-owner-group "$pkg_root" "rockbox_${VERSION}_arm64.deb"
sha256sum "rockbox_${VERSION}_arm64.deb" > "rockbox_${VERSION}_arm64.deb.sha256"

)
load_step_env

collect '*.tar.gz*' '*.deb*'
