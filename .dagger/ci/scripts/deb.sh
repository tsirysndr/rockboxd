#!/usr/bin/env bash
set -euo pipefail
source /ci/scripts/download-release.sh
for arch in amd64 arm64; do
  root="/tmp/rbrelease/$arch"
  cp -a "dist/debian/$arch/DEBIAN" "$root/"
  # These are SDL packages; Slint/Typesense dependencies belong to linux-*-release.
  sed -i -e "s/^Version:.*/Version: ${TAG#v}/" \
    -e 's/^Depends:.*/Depends: libsdl2-2.0-0, libunwind8, libasound2, libdbus-1-3/' "$root/DEBIAN/control"
  if [[ -d "dist/debian/$arch/usr/lib" ]]; then
    mkdir -p "$root/usr/lib"
    cp -a "dist/debian/$arch/usr/lib/." "$root/usr/lib/"
  fi
  dpkg-deb --build --root-owner-group "$root" "/out/rockbox_${TAG}_${arch}.deb"
done
(cd /out && sha256sum ./*.deb > SHA256SUMS)
