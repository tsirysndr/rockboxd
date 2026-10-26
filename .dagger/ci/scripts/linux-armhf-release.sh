#!/usr/bin/env bash
# Ported from linux-armhf-build.yml / publish. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

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

# Build rockboxd for arm-unknown-linux-gnueabihf
(
export TAG="${TAG}"
bash /ci/scripts/armhf.sh
)
load_step_env

# Package binary
(
VERSION="${TAG}"
cp zig/zig-out/bin/rockboxd .
tar czvf rockbox_${VERSION}_armhf-linux.tar.gz rockboxd
sha256sum rockbox_${VERSION}_armhf-linux.tar.gz \
  > rockbox_${VERSION}_armhf-linux.tar.gz.sha256

)
load_step_env

# Install dpkg-dev
(
apt-get update
apt-get install -y dpkg-dev
)
load_step_env

# Build .deb (armhf)
(
set -euo pipefail
VERSION="${TAG}"
pkg_root="dist/debian/armhf"
mkdir -p "$pkg_root/usr/local/bin"
install -m 0755 zig/zig-out/bin/rockboxd "$pkg_root/usr/local/bin/rockboxd"
sed -i "s/^Version: .*/Version: ${VERSION}/" "$pkg_root/DEBIAN/control"
dpkg-deb --build --root-owner-group "$pkg_root" "rockbox_${VERSION}_armhf.deb"
sha256sum "rockbox_${VERSION}_armhf.deb" > "rockbox_${VERSION}_armhf.deb.sha256"

)
load_step_env

collect '*.tar.gz*' '*.deb*'
