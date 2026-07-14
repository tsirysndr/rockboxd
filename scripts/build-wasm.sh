#!/usr/bin/env bash
# Build the Rockbox decode + DSP core as a WebAssembly module for the browser.
#
# This is the *lightweight* WASM target: it compiles ONLY the extracted
# Rust crates
#
#     rockbox-codecs    audio decoders (FLAC, MP3, Vorbis, ALAC, …)
#     rockbox-dsp       EQ / crossfeed / tone / compressor / resampler
#     rockbox-metadata  tag + ReplayGain parser
#
# through the flat C ABI in `rockbox-ffi` (its cpal-backed `player` half is
# feature-gated off — the browser drives playback via Web Audio instead).
#
# There is NO firmware, no netstream, no playlist engine, no gRPC — just a
# decode-a-chunk / run-DSP / read-metadata surface. The player (queue,
# transport, scheduling, output) lives entirely in JavaScript (web/).
#
# What this does:
#   1. Builds rockbox-ffi as a wasm32-unknown-emscripten staticlib (the codec
#      and DSP C sources are compiled to wasm by emcc via the `cc` crate).
#   2. Links it into web/rockbox-core.js + web/rockbox-core.wasm with emcc.
#
# Prerequisites:
#   - Emscripten SDK installed and activated (emsdk activate latest)
#   - rustup target add wasm32-unknown-emscripten
#
# Usage:
#   source /path/to/emsdk/emsdk_env.sh
#   bash scripts/build-wasm.sh            # release
#   bash scripts/build-wasm.sh --debug    # debug (-O0 -g)
#
# Output:
#   web/rockbox-core.js    — Emscripten loader (MODULARIZE, EXPORT_NAME=RockboxModule)
#   web/rockbox-core.wasm  — WebAssembly binary
#
# The decoder spawns a codec thread (Condvar handshake), so the module is
# built with -pthread. WASM memory is therefore a SharedArrayBuffer, which
# means the page must be served with COOP/COEP headers — see
# scripts/wasm-dev-server.mjs.

set -euo pipefail

ROOTDIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOTDIR"

PROFILE="${PROFILE:-release}"
CARGO_FLAG="--release"
EMCC_OPT="${EMCC_OPT:--O2}"
if [ "$PROFILE" = "debug" ] || [[ "${1:-}" == "--debug" ]]; then
    PROFILE="debug"
    CARGO_FLAG=""
    EMCC_OPT="${EMCC_OPT:--O0 -g}"
fi

# ── Toolchain checks ──────────────────────────────────────────────────────────

if ! command -v emcc &>/dev/null; then
    echo "ERROR: emcc not found. Activate the Emscripten SDK first:"
    echo "  source /path/to/emsdk/emsdk_env.sh"
    exit 1
fi

if ! rustup target list --installed | grep -q "wasm32-unknown-emscripten"; then
    echo "ERROR: wasm32-unknown-emscripten Rust target not installed."
    echo "  rustup target add wasm32-unknown-emscripten"
    exit 1
fi

echo "==> Using: $(emcc --version 2>&1 | head -1)"

# ── Step 1: Build rockbox-ffi staticlib (wasm32-unknown-emscripten) ───────────
#
# --no-default-features drops the `player` feature (and with it the
# rockbox-playback + cpal dependency, which needs a real output device).
# `cargo rustc --crate-type staticlib` builds ONLY the .a — no cdylib link
# step (which would need a main()).

echo ""
echo "==> Step 1: Build rockbox-ffi (decode + DSP + metadata) for wasm"

RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals" \
    cargo rustc \
        $CARGO_FLAG \
        --target wasm32-unknown-emscripten \
        -p rockbox-ffi \
        --no-default-features \
        --lib --crate-type staticlib

RUST_LIB="$ROOTDIR/target/wasm32-unknown-emscripten/$PROFILE/librockbox_ffi.a"
if [ ! -f "$RUST_LIB" ]; then
    echo "ERROR: expected staticlib not found: $RUST_LIB"
    exit 1
fi
echo "    librockbox_ffi.a: $(ls -lh "$RUST_LIB" | awk '{print $5}')"

# ── Step 2: emcc link → web/rockbox-core.{js,wasm} ────────────────────────────

echo ""
echo "==> Step 2: Link rockbox-core.{js,wasm} with emcc"

OUTPUT_DIR="$ROOTDIR/web"
mkdir -p "$OUTPUT_DIR"

# Every rb_* symbol JS calls. A missing entry is silently dead-stripped and
# Module._rb_foo becomes undefined at runtime, so keep this in sync with the
# rockbox-ffi C ABI (see include/rockbox_ffi.h).
EXPORTED_FUNCTIONS='["_malloc","_free","_rb_ffi_abi_version",
    "_rb_decoder_open","_rb_decoder_free","_rb_decoder_metadata_json",
    "_rb_decoder_next_chunk","_rb_decoder_seek_ms","_rb_decoder_elapsed_ms",
    "_rb_decoder_finished",
    "_rb_stream_new","_rb_stream_feed","_rb_stream_close",
    "_rb_stream_available","_rb_stream_free","_rb_decoder_open_stream",
    "_rb_dsp_new","_rb_dsp_free","_rb_dsp_set_input_frequency","_rb_dsp_flush",
    "_rb_dsp_eq_enable","_rb_dsp_set_eq_band","_rb_dsp_set_eq_precut",
    "_rb_dsp_set_tone","_rb_dsp_set_tone_cutoffs","_rb_dsp_set_surround",
    "_rb_dsp_set_channel_config","_rb_dsp_set_stereo_width",
    "_rb_dsp_set_compressor","_rb_dsp_set_replaygain",
    "_rb_dsp_set_replaygain_gains","_rb_dsp_set_replaygain_gains_raw",
    "_rb_dsp_process",
    "_rb_meta_read_json","_rb_meta_probe",
    "_rb_string_free","_rb_buffer_free"]'
EXPORTED_FUNCTIONS="$(echo "$EXPORTED_FUNCTIONS" | tr -d '\n ')"

RUNTIME_METHODS='["UTF8ToString","stringToUTF8","lengthBytesUTF8",
    "HEAP8","HEAPU8","HEAP16","HEAPU16","HEAP32","HEAPU32","HEAPF32",
    "getValue","setValue","FS"]'
RUNTIME_METHODS="$(echo "$RUNTIME_METHODS" | tr -d '\n ')"

emcc \
    -o "$OUTPUT_DIR/rockbox-core.js" \
    $EMCC_OPT \
    -pthread \
    -sPTHREAD_POOL_SIZE=4 \
    -sPTHREAD_POOL_SIZE_STRICT=0 \
    -sALLOW_MEMORY_GROWTH=1 \
    -sINITIAL_MEMORY=67108864 \
    -sSTACK_SIZE=2097152 \
    -sMODULARIZE=1 \
    -sEXPORT_NAME=RockboxModule \
    -sEXPORT_ES6=0 \
    -sNO_EXIT_RUNTIME=1 \
    -sENVIRONMENT=web,worker \
    -Wl,--no-check-features \
    "-sEXPORTED_FUNCTIONS=$EXPORTED_FUNCTIONS" \
    "-sEXPORTED_RUNTIME_METHODS=$RUNTIME_METHODS" \
    "$RUST_LIB"

echo ""
echo "✔ Build complete:"
echo "    $OUTPUT_DIR/rockbox-core.js   ($(ls -lh "$OUTPUT_DIR/rockbox-core.js"   | awk '{print $5}'))"
echo "    $OUTPUT_DIR/rockbox-core.wasm ($(ls -lh "$OUTPUT_DIR/rockbox-core.wasm" | awk '{print $5}'))"
echo ""
echo "Serve web/ with COOP/COEP headers (required for SharedArrayBuffer):"
echo "  node scripts/wasm-dev-server.mjs"
