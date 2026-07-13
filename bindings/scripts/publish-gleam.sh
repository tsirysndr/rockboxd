#!/usr/bin/env bash
#
# Publish the Gleam package (rockbox_ffi) to Hex from a local machine (correct
# OTP + interactive Hex auth — `gleam publish` can't do Hex 2FA in CI).
#
# This package is now PURE GLEAM — only the ergonomic wrappers. The native NIF
# lives in the shared `rockbox_ffi_nif` package (bindings/erlang), which must be
# published to Hex FIRST via `bindings/scripts/publish-erlang.sh` (that script
# also writes the checksum manifest the loader downloads against).
#
# For monorepo development gleam.toml depends on rockbox_ffi_nif via a path dep
# (`{ path = "../erlang" }`). Hex rejects path deps, so this script temporarily
# rewrites that line to the released Hex version requirement, publishes, then
# restores the original gleam.toml (a trap restores it even on failure).
#
# Authenticate first:  gleam hex authenticate   (or export HEXPM_API_KEY=...)
#
# Usage:
#   bindings/scripts/publish-gleam.sh [--repo owner/repo] [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"
ROOT="$(cd "$COMMON_DIR/../.." && pwd)"
GLEAM_DIR="$ROOT/bindings/gleam"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }

command -v gleam >/dev/null 2>&1 || { echo "error: gleam not found — https://gleam.run" >&2; exit 1; }

# The rockbox_ffi_nif version the published package must depend on.
NIF_VERSION="$(sed -n 's/.*{vsn, *"\([^"]*\)".*/\1/p' \
  "$ROOT/bindings/erlang/src/rockbox_ffi_nif.app.src" | head -1)"
[ -n "$NIF_VERSION" ] || { echo "error: could not read rockbox_ffi_nif version from app.src" >&2; exit 1; }
NIF_MAJOR="${NIF_VERSION%%.*}"
NEXT_MAJOR=$(( NIF_MAJOR + 1 ))

echo "== Gleam -> Hex =="
echo "rockbox_ffi_nif dep: >= $NIF_VERSION and < $NEXT_MAJOR.0.0 (publish that package first if you haven't)"
[ "$DRY" -eq 1 ] && echo "Mode:    DRY RUN (nothing will be pushed)"
echo

# Swap the path dep for the Hex version requirement, restoring on exit. Gleam
# rejects path deps at publish; the restore keeps the monorepo working tree on
# the path dep for local development.
GLEAM_TOML="$GLEAM_DIR/gleam.toml"
BACKUP="$(mktemp)"
cp "$GLEAM_TOML" "$BACKUP"
restore_toml() { cp "$BACKUP" "$GLEAM_TOML"; rm -f "$BACKUP"; }
trap restore_toml EXIT

# Replace the whole `rockbox_ffi_nif = ...` line (path dep) with a Hex req.
python3 - "$GLEAM_TOML" "$NIF_VERSION" "$NEXT_MAJOR" <<'PY'
import re, sys
path, ver, nextmajor = sys.argv[1], sys.argv[2], sys.argv[3]
src = open(path).read()
new = re.sub(
    r'(?m)^rockbox_ffi_nif\s*=.*$',
    f'rockbox_ffi_nif = ">= {ver} and < {nextmajor}.0.0"',
    src,
)
if new == src:
    sys.exit("error: could not find the rockbox_ffi_nif dependency line in gleam.toml")
open(path, "w").write(new)
PY
echo "  gleam.toml: rockbox_ffi_nif -> Hex version requirement (restored after publish)"

cd "$GLEAM_DIR"
# `gleam publish` uploads the package and its HexDocs. `gleam docs publish`
# afterwards is idempotent and refreshes HexDocs even when the version already
# exists (re-run docs without a version bump).
run gleam publish --yes
run gleam docs publish
echo "Done."
