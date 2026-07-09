#!/usr/bin/env bash
#
# Publish the npm packages from a GitHub Release to npmjs. Downloads the
# tarballs CI already packed (the main `rockbox-ffi` package plus the six
# `@rockbox-ffi/<platform>` binary packages, each bundling the prebuilt
# librockbox_ffi) and `npm publish`es them — platform packages first so the
# main package's optionalDependencies resolve.
#
# Authenticate first:  npm login   (or an authToken in ~/.npmrc).
# The @rockbox-ffi scope/org must exist on npmjs and your token must own it.
#
# Usage:
#   bindings/scripts/publish-npm.sh [--tag <tag>] [--repo <owner/repo>] [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }
resolve_repo_and_tag

command -v npm >/dev/null 2>&1 || { echo "error: npm not found" >&2; exit 1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

echo "== npm -> npmjs =="
download_assets "$TMP" '*.tgz'
shopt -s nullglob

main_tgz="" platform_tgz=()
for t in "$TMP"/*.tgz; do
  # The main package packs to rockbox-ffi-<version>.tgz (a digit right after
  # the dash); platform packages are rockbox-ffi-<platform>-<version>.tgz.
  if [[ "$(basename "$t")" =~ ^rockbox-ffi-[0-9] ]]; then main_tgz="$t"; else platform_tgz+=("$t"); fi
done

[ ${#platform_tgz[@]} -gt 0 ] || [ -n "$main_tgz" ] || { echo "  no .tgz assets in $TAG" >&2; exit 1; }

for t in "${platform_tgz[@]}"; do
  echo "  publish $(basename "$t")"
  run npm publish "$t" --access public
done
if [ -n "$main_tgz" ]; then
  echo "  publish $(basename "$main_tgz")"
  run npm publish "$main_tgz" --access public
fi
echo "Done."
