#!/usr/bin/env bash
set -euo pipefail
source /ci/scripts/common.sh
export ELECTRON_BUILDER_ALLOW_UNRESOLVED_DEPENDENCIES=true
cd webui/rockbox
bun install
# Publishing is deliberately a separate host step, including on tag builds.
bun run build:electron --publish never
collect 'webui/rockbox/release/*/*.AppImage' 'webui/rockbox/release/*/*.tar.gz'
