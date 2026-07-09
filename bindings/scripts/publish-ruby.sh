#!/usr/bin/env bash
#
# Publish the Ruby gems from a GitHub Release to RubyGems. Downloads the
# platform gems + source gem CI already built (each bundling the prebuilt
# librockbox_ffi) and `gem push`es them.
#
# Authenticate first:  gem signin   (or export GEM_HOST_API_KEY=rubygems_xxx)
#
# Usage:
#   bindings/scripts/publish-ruby.sh [--tag <tag>] [--repo <owner/repo>] [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }
resolve_repo_and_tag

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

echo "== Ruby -> RubyGems =="
download_assets "$TMP" '*.gem'
shopt -s nullglob
gems=("$TMP"/*.gem)
[ ${#gems[@]} -gt 0 ] || { echo "  no .gem assets in $TAG" >&2; exit 1; }
for g in "${gems[@]}"; do
  echo "  push $(basename "$g")"
  run gem push "$g"
done
echo "Done."
