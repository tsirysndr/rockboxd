#!/usr/bin/env bash
#
# Publish the Python wheels + sdist from a GitHub Release to PyPI. Downloads the
# macOS + manylinux wheels CI already built (each bundling the prebuilt
# librockbox_ffi) plus the sdist, and uploads them with twine. BSD wheels are
# skipped — PyPI always rejects those platform tags.
#
# Authenticate first (any twine method):
#   ~/.pypirc   or   export TWINE_USERNAME=__token__ TWINE_PASSWORD=pypi-xxx
#
# Usage:
#   bindings/scripts/publish-python.sh [--tag <tag>] [--repo <owner/repo>] [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }
resolve_repo_and_tag

command -v twine >/dev/null 2>&1 || { echo "error: twine not found (pip install twine)" >&2; exit 1; }

TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT

echo "== Python -> PyPI =="
download_assets "$TMP" '*.whl' '*.tar.gz'
shopt -s nullglob

# PyPI (Warehouse) accepts macOS + manylinux wheels and the sdist. It always
# rejects the freebsd_/netbsd_ platform tags, so BSD wheels are never uploaded —
# those users install from the sdist.
main=("$TMP"/*macosx*.whl "$TMP"/*manylinux*.whl "$TMP"/*.tar.gz)

[ ${#main[@]} -gt 0 ] || { echo "  no wheels/sdist in $TAG" >&2; exit 1; }

# Prompt for the token ONCE if no non-interactive credentials are configured,
# so both twine invocations reuse it instead of prompting per call.
if [ "$DRY" -eq 0 ] && [ -z "${TWINE_PASSWORD:-}" ] && [ ! -f "$HOME/.pypirc" ]; then
  export TWINE_USERNAME="${TWINE_USERNAME:-__token__}"
  read -rsp "  PyPI API token (input hidden): " TWINE_PASSWORD; echo
  export TWINE_PASSWORD
fi

echo "  upload macOS + manylinux wheels + sdist"
run twine upload --skip-existing "${main[@]}"
echo "Done."
