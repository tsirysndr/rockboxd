#!/usr/bin/env bash
# Build the headless macOS / Linux / *BSD rockboxd binary.
#
# What this does:
#   1. Creates and configures build-headless/ (headless host target, target 206).
#   2. Builds all firmware .a files (no SDL, BINFMT_STATIC codecs).
#   3. Compiles Rust crates with the cpal-sink feature.
#   4. Links everything into zig-out/bin/rockboxd via Zig.
#
# Usage:
#   bash scripts/build-headless.sh
#   bash scripts/build-headless.sh --release   # default; pass --debug for debug
#
# The binary ends up at:
#   zig/zig-out/bin/rockboxd
#
# Runtime:
#   ./zig/zig-out/bin/rockboxd
#   # cpal uses the system default audio device; configure in settings.toml
#   # with audio_output = "cpal" or leave blank for the default builtin sink.

set -euo pipefail

ROOTDIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOTDIR"

PROFILE="${PROFILE:-release}"
ZIG_OPT="ReleaseFast"
CARGO_FLAG="--release"
if [ "$PROFILE" = "debug" ] || [[ "${1:-}" == "--debug" ]]; then
    PROFILE="debug"
    ZIG_OPT="Debug"
    CARGO_FLAG=""
fi

echo "==> Step 1: Configure build-headless/ (target 206 — headless host)"
mkdir -p build-headless
if [ ! -f build-headless/Makefile ]; then
    (cd build-headless && \
     printf "206\nN\n" | ../tools/configure)
else
    echo "    build-headless/Makefile already exists, skipping configure"
fi

# On macOS, GNU binutils objcopy corrupts Mach-O 64-bit object files by
# converting MH_MAGIC_64 (0xFEEDFACF) to MH_MAGIC (0xFEEDFACE) and removing
# the 4-byte reserved field from the mach_header_64.  The CODECS_STATIC build
# runs objcopy --redefine-sym on each codec's .o, so we must use llvm-objcopy
# instead.  Search for it in common Homebrew paths.
LLVM_OBJCOPY=""
if [[ "$(uname)" == "Darwin" ]]; then
    # Prefer llvm@21 — llvm@22 crashes in setSymbolInRelocationInfo on some codecs.
    for _oc in /opt/homebrew/opt/llvm@21/bin/llvm-objcopy \
               /usr/local/opt/llvm@21/bin/llvm-objcopy \
               /opt/homebrew/opt/llvm/bin/llvm-objcopy \
               /usr/local/opt/llvm/bin/llvm-objcopy; do
        if [ -x "$_oc" ]; then
            LLVM_OBJCOPY="$_oc"
            break
        fi
    done
    if [ -z "$LLVM_OBJCOPY" ]; then
        echo "WARNING: llvm-objcopy not found; codec archives may be corrupted."
        echo "  Install via: brew install llvm"
    else
        echo "    Using llvm-objcopy: $LLVM_OBJCOPY"
        # Delete the per-codec .a files so Make rebuilds them with the correct tool.
        CODEC_OBJ_DIR="$ROOTDIR/build-headless/lib/rbcodec/codecs"
        for name in a52 a52_rm aac aac_bsf adx aiff alac ape \
                    atrac3_oma atrac3_rm au cook flac mod mpa mpc \
                    opus raac shorten smaf speex tta vorbis vox \
                    wav wav64 wavpack wma wmapro; do
            rm -f "$CODEC_OBJ_DIR/$name.o" "$CODEC_OBJ_DIR/$name.a" \
                  "$CODEC_OBJ_DIR/$name-crt0.o"
        done
    fi
fi

echo "==> Step 2: Build firmware static libs"
MAKE_EXTRA_ARGS=""
if [ -n "$LLVM_OBJCOPY" ]; then
    MAKE_EXTRA_ARGS="OC=$LLVM_OBJCOPY"
fi
# Ensure codecs/lib is a directory, not a stale empty file (can be left by a
# previous aborted build before the directory was first created).
CODEC_LIB_DIR="$ROOTDIR/build-headless/lib/rbcodec/codecs/lib"
if [ -f "$CODEC_LIB_DIR" ] && [ ! -d "$CODEC_LIB_DIR" ]; then
    rm -f "$CODEC_LIB_DIR"
fi
NCPU="$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || sysctl -n hw.ncpuonline 2>/dev/null || sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"
# Use -k (keep going) because spc and asap crash llvm-objcopy@21 on macOS
# with a segfault in setSymbolInRelocationInfo.  Neither codec is linked into
# rockboxd (they are absent from the codec-objects extraction loop), so their
# build failures are harmless.
(cd build-headless && make -j"$NCPU" -k lib $MAKE_EXTRA_ARGS) || {
    if [[ "$(uname)" == "Darwin" ]]; then
        echo "    Note: spc/asap build failures are expected on macOS (llvm-objcopy crash — codecs are unused)."
    else
        echo "ERROR: 'make lib' failed"; exit 1
    fi
}

echo "==> Step 2.1: Build libcodec.a (codeclib.c — codec_init, bs_*, ff_*)"
# libcodec.a is excluded from per-codec archives in CODECS_STATIC mode but
# must still be linked into the final binary.
LIBCODEC="$ROOTDIR/build-headless/lib/rbcodec/codecs/libcodec.a"
if [ ! -f "$LIBCODEC" ]; then
    (cd build-headless && make -j"$NCPU" "$LIBCODEC" $MAKE_EXTRA_ARGS)
fi
echo "    libcodec.a: $(ls -lh "$LIBCODEC" | awk '{print $5}')"

echo "==> Step 2.5: Extract per-codec .o files for direct Zig linking"
# Zig's MachO linker does not scan archives for symbols referenced only via
# data-section relocations (lc_static_table -> __header_* pointers).
# Extracting the codec objects and linking them directly bypasses this.
CODEC_DIR="$ROOTDIR/build-headless/lib/rbcodec/codecs"
OBJECTS_DIR="$CODEC_DIR/codec-objects"
mkdir -p "$OBJECTS_DIR"
for name in a52 a52_rm aac aac_bsf adx aiff alac ape \
            atrac3_oma atrac3_rm au cook flac mod mpa mpc \
            opus raac shorten smaf speex tta vorbis vox \
            wav wav64 wavpack wma wmapro; do
    obj_dir="$OBJECTS_DIR/$name"
    rm -rf "$obj_dir" && mkdir -p "$obj_dir"
    (cd "$obj_dir" && ar x "$CODEC_DIR/$name.a")
done
echo "    Done — codec objects in $OBJECTS_DIR"

echo "==> Step 2.6: Namespace ogg_* symbols in libopus.a to avoid ABI conflict with libtremor"
# libopus bundles Ogg framing with a different ABI than libtremor (different
# ogg_stream_state layout, extra copy_body arg on ogg_stream_pagein).
# Rename every ogg_* symbol in libopus.a and in opus.o to libopus_ogg_* so
# the Opus codec gets its own Ogg implementation and the Vorbis codec keeps
# libtremor's — both correct, no lld duplicate-symbol errors on Linux.
OBJCOPY="${LLVM_OBJCOPY:-objcopy}"
LIBOPUS_A="$CODEC_DIR/libopus.a"
OPUS_O="$OBJECTS_DIR/opus/opus.o"

OGG_SYMS=(
    ogg_packet_clear
    ogg_page_bos ogg_page_checksum_set ogg_page_continued ogg_page_eos
    ogg_page_granulepos ogg_page_packets ogg_page_pageno ogg_page_serialno
    ogg_page_version
    ogg_stream_check ogg_stream_clear ogg_stream_destroy ogg_stream_eos
    ogg_stream_flush ogg_stream_flush_fill ogg_stream_init ogg_stream_iovecin
    ogg_stream_packetin ogg_stream_packetout ogg_stream_packetpeek
    ogg_stream_pagein ogg_stream_pageout ogg_stream_pageout_fill
    ogg_stream_reset ogg_stream_reset_serialno
    ogg_sync_buffer ogg_sync_check ogg_sync_clear ogg_sync_destroy
    ogg_sync_init ogg_sync_pageout ogg_sync_pageseek ogg_sync_reset
    ogg_sync_wrote
)

REDEFINE_ARGS=()
for sym in "${OGG_SYMS[@]}"; do
    REDEFINE_ARGS+=(--redefine-sym "${sym}=libopus_${sym}")
    # Mach-O symbols have a leading underscore; pass both forms.
    REDEFINE_ARGS+=(--redefine-sym "_${sym}=_libopus_${sym}")
done

# Extract all objects from libopus.a, rename, re-archive.
TMP_LIBOPUS="$(mktemp -d)"
(cd "$TMP_LIBOPUS" && ar x "$LIBOPUS_A")
for obj in "$TMP_LIBOPUS"/*.o; do
    [ -f "$obj" ] && "$OBJCOPY" "${REDEFINE_ARGS[@]}" "$obj" 2>/dev/null || true
done
rm -f "$LIBOPUS_A"
ar rcs "$LIBOPUS_A" "$TMP_LIBOPUS"/*.o
rm -rf "$TMP_LIBOPUS"

# Apply the same renames to the opus codec object so it calls libopus_ogg_*.
[ -f "$OPUS_O" ] && "$OBJCOPY" "${REDEFINE_ARGS[@]}" "$OPUS_O" 2>/dev/null || true

echo "    Done — ${#OGG_SYMS[@]} ogg_* symbols namespaced in libopus.a + opus.o"

# On the BSDs there is no prebuilt Typesense binary to download/spawn, so
# search must run in-process against SQLite FTS5. The `fts5` feature has to
# be passed to BOTH staticlibs (rockbox-cli and rockbox-server) since they
# are linked into the same Zig binary.
CLI_FEATURES="cpal-sink"
SERVER_FEATURES=""
case "$(uname)" in
    FreeBSD|NetBSD)
        # cpal's ALSA backend is Linux-only, so the BSDs use the direct
        # libasound sink (rockbox-alsa-sink + pcm-alsa.c) instead of cpal.
        # Also enable fts5 — there is no prebuilt Typesense for the BSDs.
        echo "    BSD host detected — using alsa-sink + fts5 (no cpal, no Typesense)"
        CLI_FEATURES="alsa-sink,fts5"
        SERVER_FEATURES="fts5"
        ;;
    OpenBSD)
        # OpenBSD has no ALSA and cpal has no sndio backend, so use the direct
        # libsndio sink (rockbox-sndio-sink + pcm-sndio.c). fts5 for the same
        # reason as the other BSDs (no prebuilt Typesense).
        echo "    OpenBSD host detected — using sndio-sink + fts5 (no cpal, no Typesense)"
        CLI_FEATURES="sndio-sink,fts5"
        SERVER_FEATURES="fts5"
        ;;
esac

echo "==> Step 3: Build Rust crates (cli features: $CLI_FEATURES)"
cargo build $CARGO_FLAG --features "$CLI_FEATURES" -p rockbox-cli
if [ -n "$SERVER_FEATURES" ]; then
    cargo build $CARGO_FLAG --features "$SERVER_FEATURES" -p rockbox-server
else
    cargo build $CARGO_FLAG -p rockbox-server
fi

echo "==> Step 4: Link rockboxd with Zig (headless)"
ZIG_EXTRA_ARGS=""
if [ "$(uname -m)" = "x86_64" ]; then
    ZIG_EXTRA_ARGS="-Dcpu=x86_64"
fi
(cd zig && zig build -Dheadless=true "-Doptimize=$ZIG_OPT" $ZIG_EXTRA_ARGS)

echo ""
echo "✔ Build complete: zig/zig-out/bin/rockboxd"
echo ""
echo "Run with:"
echo "  ./zig/zig-out/bin/rockboxd"
echo "  RUST_LOG=info ./zig/zig-out/bin/rockboxd"
