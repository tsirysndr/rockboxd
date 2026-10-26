#!/usr/bin/env bash
set -euo pipefail
mkdir -p /out
: > /out/store-paths
nix flake show path:.
# A path flake includes submodules copied into the sandbox, without needing .git.
case "$FLAKE_TARGET" in
  .#*) FLAKE_TARGET="path:${FLAKE_TARGET}" ;;
  *) echo 'Expected a local flake attribute such as .#rockboxd' >&2; exit 2 ;;
esac
if out=$(nix eval --raw "${FLAKE_TARGET}.drvPath" 2>&1); then
  nix build --print-build-logs --no-link --print-out-paths "$FLAKE_TARGET" > /out/store-paths
  mapfile -t paths < /out/store-paths
  nix copy --to file:///out/store "${paths[@]}"
elif grep -q 'does not provide attribute' <<< "$out"; then
  echo "==> $FLAKE_TARGET is not packaged for this system — skipping."
else
  echo "$out" >&2
  exit 1
fi
