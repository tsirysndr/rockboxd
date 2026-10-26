#!/usr/bin/env bash
# Assemble both architectures from the four-archive SDL release format.
set -euo pipefail
source /ci/scripts/common.sh
for arch in x86_64 aarch64; do
  if [[ "$arch" == x86_64 ]]; then deb_arch=amd64; else deb_arch=arm64; fi
  dest="/tmp/rbrelease/$deb_arch/usr/local"
  mkdir -p "$dest/bin" "$dest/lib/rockbox" "$dest/share/rockbox"
  archives=("rockbox_${TAG}_${arch}-linux.tar.gz" "rockboxd_${TAG}_${arch}-linux.tar.gz" "rockbox-codecs-${TAG}-${arch}-linux.tar.gz" "rockbox-assets-${TAG}-${arch}-linux.tar.gz")
  destinations=("$dest/bin" "$dest/bin" "$dest/lib/rockbox" "$dest/share/rockbox")
  for i in "${!archives[@]}"; do
    archive=${archives[$i]}
    url="https://github.com/tsirysndr/rockboxd/releases/download/$TAG/$archive"
    curl -fL --retry 3 "$url" -o "/tmp/$archive"
    curl -fL --retry 3 "$url.sha256" -o "/tmp/$archive.sha256"
    (cd /tmp && sha256sum -c "$archive.sha256")
    tar xzf "/tmp/$archive" -C "${destinations[$i]}"
  done
done
