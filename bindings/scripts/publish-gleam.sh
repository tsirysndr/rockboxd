#!/usr/bin/env bash
#
# Publish the Gleam package to Hex from a local machine (correct OTP + interactive
# Hex auth — `gleam publish` can't do Hex 2FA in CI).
#
# Gleam ships prebuilt multi-arch NIFs: one priv/rockbox_ffi_nif-<target>.so per
# platform. Those are built by the `bindings-gleam-release.yml` GitHub Actions
# workflow and uploaded to the `gleam-v<version>` release. This script downloads
# them into priv/ so `gleam publish` bundles the whole fatball, then publishes.
#
# Authenticate first:  gleam hex authenticate   (or export HEXPM_API_KEY=...)
#
# Usage:
#   bindings/scripts/publish-gleam.sh [--tag gleam-vX.Y.Z] [--repo owner/repo] [--dry-run]
#
# The tag defaults to gleam-v<version> read from gleam.toml.

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"
ROOT="$(cd "$COMMON_DIR/../.." && pwd)"
GLEAM_DIR="$ROOT/bindings/gleam"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }

command -v gleam >/dev/null 2>&1 || { echo "error: gleam not found — https://gleam.run" >&2; exit 1; }
command -v gh    >/dev/null 2>&1 || { echo "error: gh (GitHub CLI) required — https://cli.github.com" >&2; exit 1; }

# Resolve repo from origin (gh may pick a different default with multiple remotes).
if [ -z "$REPO" ]; then
  url="$(git -C "$COMMON_DIR" remote get-url origin 2>/dev/null || true)"; url="${url%.git}"
  case "$url" in
    git@github.com:*)       REPO="${url#git@github.com:}" ;;
    ssh://git@github.com/*) REPO="${url#ssh://git@github.com/}" ;;
    https://github.com/*)   REPO="${url#https://github.com/}" ;;
  esac
  [ -n "$REPO" ] || { echo "error: could not derive repo from origin; pass --repo owner/repo" >&2; exit 1; }
fi

# Resolve tag: --tag, else gleam-v<version-from-gleam.toml> (matches the workflow).
if [ -z "$TAG" ]; then
  VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$GLEAM_DIR/gleam.toml" | head -1)"
  [ -n "$VERSION" ] || { echo "error: could not read version from gleam.toml; pass --tag" >&2; exit 1; }
  TAG="gleam-v$VERSION"
fi

echo "== Gleam -> Hex =="
echo "Repo:    $REPO"
echo "Release: $TAG"
[ "$DRY" -eq 1 ] && echo "Mode:    DRY RUN (nothing will be pushed)"
echo

# Pull the precompiled multi-arch NIFs into priv/ so `gleam publish` bundles them.
# Tolerate a missing/empty release here so the guard below gives an actionable
# message (gh exits non-zero with a terse "no assets to download" otherwise).
echo "  downloading precompiled NIFs into priv/"
mkdir -p "$GLEAM_DIR/priv"
download_assets "$GLEAM_DIR/priv" 'rockbox_ffi_nif-*.so' || true
shopt -s nullglob
sos=("$GLEAM_DIR/priv"/rockbox_ffi_nif-*.so)
[ ${#sos[@]} -gt 0 ] || {
  echo "error: no rockbox_ffi_nif-*.so found in release $TAG." >&2
  echo "       Run the bindings-gleam-release.yml workflow first (it uploads them)." >&2
  exit 1
}
ls -l "${sos[@]}"

cd "$GLEAM_DIR"
# `gleam publish` uploads the package and its HexDocs. `gleam docs publish`
# afterwards is idempotent and refreshes HexDocs even when the version already
# exists (re-run docs without a version bump).
run gleam publish --yes
run gleam docs publish
echo "Done."
