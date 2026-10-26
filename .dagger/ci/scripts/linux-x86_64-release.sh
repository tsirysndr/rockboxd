#!/usr/bin/env bash
# Ported from linux-x86_64-build.yml / publish. Publishing stays in Actions.
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
  libfdk-aac-dev \
  libfreetype6-dev \
  libfontconfig1-dev \
  libxkbcommon-dev \
  libxkbcommon-x11-dev \
  libxcb1-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  protobuf-compiler \
  curl \
  wget \
  zip \
  unzip \
  cmake \
  dpkg-dev \
  rpm

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

# Build rockbox-embed
(
rm -f target/release/librockbox_embed.a
cargo build --release -p rockbox-embed

)
load_step_env

# Build librockboxd.a
(
rm -f zig/zig-out/lib/librockboxd.a
cd zig
zig build lib -Doptimize=ReleaseFast

)
load_step_env

# Flatten librockboxd.a (belt-and-suspenders)
(
bash scripts/flatten-archive.sh zig/zig-out/lib/librockboxd.a rb_daemon_start
echo "--- members containing daemon ---"
ar t zig/zig-out/lib/librockboxd.a | grep -i daemon || true
echo "--- rb_daemon_start symbol ---"
nm --defined-only zig/zig-out/lib/librockboxd.a 2>/dev/null \
  | grep rb_daemon_start \
  || { echo "rb_daemon_start NOT in librockboxd.a — abort"; exit 1; }

)
load_step_env

# Build rockbox CLI
(
export TAG="${TAG}"
cargo build -p rockbox --release
)
load_step_env

# Build rockbox-desktop (Linux)
(
bash desktop/package-linux.sh
)
load_step_env

# Download typesense-server
(
set -euo pipefail
url="https://dl.typesense.org/releases/${TYPESENSE_VERSION}/typesense-server-${TYPESENSE_VERSION}-linux-amd64.tar.gz"
curl -fsSL "$url" -o /tmp/typesense.tar.gz
mkdir -p /tmp/typesense
tar -xzf /tmp/typesense.tar.gz -C /tmp/typesense
install -m 0755 /tmp/typesense/typesense-server /tmp/typesense-server

)
load_step_env

# Package tarball
(
VERSION="${TAG}"
cp target/release/rockbox .
cp zig/zig-out/bin/rockboxd .
tar czvf rockbox_${VERSION}_amd64-linux.tar.gz rockbox rockboxd
sha256sum rockbox_${VERSION}_amd64-linux.tar.gz \
  > rockbox_${VERSION}_amd64-linux.tar.gz.sha256

)
load_step_env

# Package librockboxd.a
(
VERSION="${TAG}"
mkdir -p dist/lib dist/include
cp zig/zig-out/lib/librockboxd.a dist/lib/
cp include/rockboxd.h            dist/include/
ARCHIVE="librockboxd_${VERSION}_amd64-linux.tar.gz"
tar -C dist -czf "$ARCHIVE" lib include
sha256sum "$ARCHIVE" > "${ARCHIVE}.sha256"

)
load_step_env

# Build .deb (amd64)
(
set -euo pipefail
VERSION="${TAG}"
pkg_root="dist/debian/amd64"
mkdir -p "$pkg_root/usr/local/bin" "$pkg_root/usr/bin" \
         "$pkg_root/usr/share/applications" "$pkg_root/usr/share/pixmaps"
install -m 0755 target/release/rockbox    "$pkg_root/usr/local/bin/rockbox"
install -m 0755 zig/zig-out/bin/rockboxd  "$pkg_root/usr/local/bin/rockboxd"
install -m 0755 /tmp/typesense-server     "$pkg_root/usr/local/bin/typesense-server"
# desktop/package-linux.sh stages: desktop/dist/usr/bin/rockbox-desktop,
# desktop/dist/usr/share/applications/rockbox-desktop.desktop,
# desktop/dist/usr/share/pixmaps/rockbox-desktop.png
cp -r desktop/dist/usr/. "$pkg_root/usr/"
sed -i "s/^Version: .*/Version: ${VERSION}/" "$pkg_root/DEBIAN/control"
dpkg-deb --build --root-owner-group "$pkg_root" "rockbox_${VERSION}_amd64.deb"
sha256sum "rockbox_${VERSION}_amd64.deb" > "rockbox_${VERSION}_amd64.deb.sha256"

)
load_step_env

# Build .rpm (amd64)
(
set -euo pipefail
VERSION="${TAG}"
src_root="dist/rpm/amd64"
mkdir -p "${src_root}/usr/local/bin" "${src_root}/usr/bin" \
         "${src_root}/usr/share/applications" "${src_root}/usr/share/pixmaps"
install -m 0755 target/release/rockbox    "${src_root}/usr/local/bin/rockbox"
install -m 0755 zig/zig-out/bin/rockboxd  "${src_root}/usr/local/bin/rockboxd"
install -m 0755 /tmp/typesense-server     "${src_root}/usr/local/bin/typesense-server"
cp -r desktop/dist/usr/. "${src_root}/usr/"
sed -i "s/^Version:        .*/Version:        ${VERSION}/" "${src_root}/rockbox.spec"

rpm_top="$(pwd)/rpmbuild"
mkdir -p "${rpm_top}"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
cp -r "${src_root}" "${rpm_top}/SOURCES/amd64"
rpmbuild --define "_topdir ${rpm_top}" \
         --define "_sourcedir ${rpm_top}/SOURCES" \
         --target x86_64 \
         -bb "${src_root}/rockbox.spec"
find "${rpm_top}/RPMS" -name '*.rpm' -exec mv {} . \;
for rpm in rockbox-*.rpm; do
  [ -e "$rpm" ] || continue
  sha256sum "$rpm" > "${rpm}.sha256"
done

)
load_step_env

collect '*.tar.gz*' '*.deb*' '*.rpm*'
