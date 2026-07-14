#!/usr/bin/env bash
# Build + publish the rockbox-wasm npm package to npmjs.com.
#
# Usage:
#   bash bindings/wasm/scripts/publish.sh             # publish
#   bash bindings/wasm/scripts/publish.sh --dry-run   # validate without publishing
#   bash bindings/wasm/scripts/publish.sh --tag next  # publish under a dist-tag
#
# Always rebuilds dist/ (wasm embedded) first, so the tarball ships a fresh,
# self-contained build. Bump the version in bindings/wasm/package.json first,
# and make sure you're logged in (`npm whoami`).

set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"   # bindings/wasm
cd "$HERE"

VERSION="$(node -p "require('./package.json').version")"

echo "==> Building rockbox-wasm@${VERSION} (embedded wasm)"
bash scripts/build.sh

echo ""
echo "==> Publishing rockbox-wasm@${VERSION}"
npm publish --access public "$@"

echo ""
echo "✔ Done. Verify: npm view rockbox-wasm version"
