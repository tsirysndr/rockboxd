#!/usr/bin/env bash
#
# Publish the Elixir package (rockbox_ex_ffi) to Hex from a workstation.
#
# This package is now PURE ELIXIR — only the ergonomic wrappers. The native NIF
# lives in the shared `rockbox_ffi_nif` package (bindings/erlang), which must be
# published to Hex FIRST via `bindings/scripts/publish-erlang.sh`. There is no
# per-arch NIF build here anymore.
#
# The published tarball must depend on the RELEASED rockbox_ffi_nif Hex version
# (Hex rejects the `../erlang` path dep used for monorepo dev). mix.exs reads the
# ROCKBOX_NIF_HEX env var: when set, the dep becomes
# `{:rockbox_ffi_nif, ">= X and < <nextmajor>.0.0"}` (same range as the Gleam
# binding). This script exports it from the shared package's version so the two
# always match. Hex publishing is done locally — Hex 2FA/OTP is interactive.
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

# Pin the rockbox_ffi_nif dependency to the shared package's published version.
NIF_VERSION="$(sed -n 's/.*{vsn, *"\([^"]*\)".*/\1/p' \
  "$ROOT/bindings/erlang/src/rockbox_ffi_nif.app.src" | head -1)"
[ -n "$NIF_VERSION" ] || { echo "error: could not read rockbox_ffi_nif version from app.src" >&2; exit 1; }
export ROCKBOX_NIF_HEX="$NIF_VERSION"

NIF_MAJOR="${NIF_VERSION%%.*}"
echo "== Elixir -> Hex =="
echo "rockbox_ffi_nif dep: >= $NIF_VERSION and < $((NIF_MAJOR + 1)).0.0 (publish that package first if you haven't)"
[ "$DRY" -eq 1 ] && echo "Mode:    DRY RUN (nothing will be pushed)"
echo

cd "$ROOT/bindings/elixir"
run mix deps.get

# hex.publish uploads both the package and the ExDoc docs to HexDocs. The
# explicit `hex.publish docs` afterwards is idempotent and refreshes HexDocs
# even when the package version already exists (re-run docs without a bump).
run mix hex.publish --yes
run mix hex.publish docs --yes
echo "Done."
