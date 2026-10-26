#!/usr/bin/env bash
# The former plugin build: SDL firmware, CLI, daemon, codecs and installed assets.
set -euo pipefail
source /ci/scripts/common.sh
apt-get update
apt-get install -y --no-install-recommends libusb-dev libsdl2-dev libfreetype6-dev libunwind-dev libasound2-dev libdbus-1-dev protobuf-compiler
(cd webui/rockbox && deno install && deno run build)
(cd crates/s3/s3webui && bun install --frozen-lockfile && bun run build)
mkdir -p build
(cd build && ../tools/configure --target=sdlapp --type=N --lcdwidth=320 --lcdheight=240 --prefix=/usr/local && make ziginstall -j"$(nproc)")
mkdir -p /out/runtime/usr/local/{bin,lib,share}
cp /usr/local/bin/{rockbox,rockboxd} /out/runtime/usr/local/bin/
cp -a /usr/local/lib/rockbox /out/runtime/usr/local/lib/
cp -a /usr/local/share/rockbox /out/runtime/usr/local/share/
collect
