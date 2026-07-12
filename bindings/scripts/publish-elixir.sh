#!/usr/bin/env bash
#
# Publish the Elixir package to Hex. Unlike the JVM bindings, the BEAM bindings
# ship SOURCE (lib + c_src + Makefile) — the consumer compiles the NIF via
# elixir_make. But `mix hex.publish` compiles the project first, and that link
# step needs the local static archive target/release/librockbox_ffi.a, so we
# (re)build rockbox-ffi first (cargo is incremental — a no-op when up to date).
# The published version comes from mix.exs.
#
# Authenticate first:  mix hex.user auth   (or export HEX_API_KEY=...)
#
# Usage:
#   bindings/scripts/publish-elixir.sh [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"
ROOT="$(cd "$COMMON_DIR/../.." && pwd)"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }

command -v mix >/dev/null 2>&1 || { echo "error: mix (Elixir) not found" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "error: cargo not found" >&2; exit 1; }

echo "== Elixir -> Hex =="
# Rebuild the native FFI static archive if needed (compile-time dep of the NIF).
run cargo build --release -p rockbox-ffi --manifest-path "$ROOT/Cargo.toml"
cd "$ROOT/bindings/elixir"
run mix deps.get
# hex.publish uploads both the package and the ExDoc docs to HexDocs. The
# explicit `hex.publish docs` afterwards is idempotent and refreshes HexDocs
# even when the package version already exists (re-run docs without a bump).
run mix hex.publish --yes
run mix hex.publish docs --yes
echo "Done."
