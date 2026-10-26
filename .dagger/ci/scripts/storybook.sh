#!/usr/bin/env bash
set -euo pipefail
source /ci/scripts/common.sh
cd webui/rockbox
bun install
bun run build-storybook
collect 'webui/rockbox/storybook-static'
