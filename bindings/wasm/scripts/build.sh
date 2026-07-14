#!/usr/bin/env bash
# Build the rockbox-wasm npm package into dist/.
#
#   dist/rockbox.js                 ESM facade (package entry)
#   dist/rockbox-core.js            Emscripten glue with the .wasm EMBEDDED
#                                   (SINGLE_FILE — no separate binary to serve)
#   dist/rockbox-decoder-worker.js  decoder Worker
#   dist/rockbox-audio-worklet.js   AudioWorklet
#
# Prereqs: the Emscripten SDK + `rustup target add wasm32-unknown-emscripten`
# (same as scripts/build-wasm.sh).

set -euo pipefail

HERE="$(cd "$(dirname "$0")/.." && pwd)"   # bindings/wasm
ROOT="$(cd "$HERE/../.." && pwd)"          # repo root
DIST="$HERE/dist"

mkdir -p "$DIST"

echo "==> Building wasm-embedded core into dist/ (SINGLE_FILE)"
OUTPUT_DIR="$DIST" SINGLE_FILE=1 bash "$ROOT/scripts/build-wasm.sh"

echo ""
echo "==> Copying JS facade + worker + worklet"
cp "$HERE/src/rockbox.js"                 "$DIST/"
cp "$HERE/src/rockbox-decoder-worker.js"  "$DIST/"
cp "$HERE/src/rockbox-audio-worklet.js"   "$DIST/"

echo ""
echo "✔ Packaged rockbox-wasm → $DIST"
ls -lh "$DIST"
