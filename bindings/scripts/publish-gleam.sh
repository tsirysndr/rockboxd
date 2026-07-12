#!/usr/bin/env bash
#
# Publish the Gleam package to Hex. Like the Elixir binding it ships SOURCE
# (src + c_src + Makefile) — the consumer compiles the NIF. `gleam publish`
# builds the project first, so we (re)build the local static archive
# target/release/librockbox_ffi.a beforehand (cargo is incremental — a no-op
# when up to date). The published version comes from gleam.toml.
#
# Authenticate first:  gleam hex authenticate   (or export HEXPM_API_KEY=...)
#
# Usage:
#   bindings/scripts/publish-gleam.sh [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"
ROOT="$(cd "$COMMON_DIR/../.." && pwd)"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }

command -v gleam >/dev/null 2>&1 || { echo "error: gleam not found — https://gleam.run" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "error: cargo not found" >&2; exit 1; }

echo "== Gleam -> Hex =="
# Rebuild the native FFI static archive if needed (compile-time dep of the NIF).
run cargo build --release -p rockbox-ffi --manifest-path "$ROOT/Cargo.toml"
cd "$ROOT/bindings/gleam"
# `gleam publish` uploads the package and its HexDocs. `gleam docs publish`
# afterwards is idempotent and refreshes HexDocs even when the version already
# exists (re-run docs without a version bump).
run gleam publish --yes
run gleam docs publish
echo "Done."
