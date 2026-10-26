#!/usr/bin/env bash
set -euo pipefail
source /ci/scripts/common.sh
cargo fmt --all --check
collect
