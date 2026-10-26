#!/usr/bin/env bash
set -euo pipefail
source /ci/scripts/common.sh
cd webui/rockbox
bun install
bun run test --run
collect
