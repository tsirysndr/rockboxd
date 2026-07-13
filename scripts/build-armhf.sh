#!/usr/bin/env bash
# Cross-compiles rockboxd for arm-unknown-linux-gnueabihf (ARM hard-float Linux).
#
# What this does:
#   1. Builds the Docker image from Dockerfile.arm-unknown-linux-gnueabihf.
#   2. Inside Docker: configures build-armhf/ (target 208) and builds all
#      firmware .a files with the arm-linux-gnueabihf-gcc cross-compiler.
#   3. Inside Docker: extracts per-codec .o files and namespaces ogg_* symbols
#      in libopus.a (same as build-headless.sh steps 2.5 and 2.6).
#   4. On the host: builds the S3 admin web UI bundle (rust-embed reads it
#      from crates/s3/s3webui/dist/ at compile time — the directory is
#      gitignored, so a fresh checkout has no dist/ until this step runs).
#   5. On the host: builds Rust crates with `cross` (fts5 + cpal-sink features,
#      no typesense subprocess).
#   6. On the host: links everything with Zig targeting arm-linux-gnueabihf.
#
# Usage:
#   bash scripts/build-armhf.sh
#
# Prerequisites:
#   - Docker (for the arm-linux-gnueabihf cross-toolchain)
#   - cross  (cargo install cross)
#   - zig    (0.16.0)
#   - bun    (for the S3 admin web UI)
#
# Output:
#   zig/zig-out/bin/rockboxd  (ARM hard-float ELF binary)

set -euo pipefail

ROOTDIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOTDIR"

DOCKER_IMAGE="rockboxd-armhf"
BUILD_DIR="build-armhf"
RUST_TARGET="arm-unknown-linux-gnueabihf"

echo "==> Step 1: Build Docker image ($DOCKER_IMAGE)"
docker build --platform linux/amd64 -t "$DOCKER_IMAGE" -f Dockerfile.arm-unknown-linux-gnueabihf .

echo "==> Step 2: Configure and build firmware for ARM inside Docker"
docker run --rm --platform linux/amd64 \
    -v "$ROOTDIR:/src" \
    -w /src \
    "$DOCKER_IMAGE" \
    bash -c "
        set -euo pipefail

        NCPU=\"\$(nproc 2>/dev/null || echo 4)\"

        # Build Linux x86_64 host tools into /tmp/hosttools/ so the macOS
        # Mach-O binaries in the volume-mounted tools/ are never executed.
        HOSTTOOLS=/tmp/hosttools
        if [ ! -d \"\$HOSTTOOLS\" ]; then
            mkdir -p \"\$HOSTTOOLS\"
            # Symlink everything from tools/ so make dependency checks on
            # .c/.h/.make files still work relative to TOOLSDIR.
            for f in /src/tools/*; do
                ln -sf \"\$f\" \"\$HOSTTOOLS/\$(basename \"\$f\")\"
            done
            # Compile/replace the binaries with Linux-native versions.
            printf '#define APPLICATION_NAME \"bmp2rb\"\n' > /tmp/appname.h
            gcc -O2 -include /tmp/appname.h /src/tools/bmp2rb.c -o \"\$HOSTTOOLS/bmp2rb\"
            gcc -O2 /src/tools/convbdf.c -o \"\$HOSTTOOLS/convbdf\"
            gcc -O2 /src/tools/rdf2binary.c -o \"\$HOSTTOOLS/rdf2binary\"
            gcc -O2 /src/tools/codepages.c /src/tools/codepage_tables.c -o \"\$HOSTTOOLS/codepages\"
            # iaudio_bl_flash.c is generated from iaudio_bl_flash.bmp by bmp2rb.
            # It exists on local machines (generated once and cached) but is not
            # tracked in git, so CI starts with a fresh checkout that lacks it.
            # Generate it now using the bmp2rb we just compiled.
            if [ ! -f /src/tools/iaudio_bl_flash.c ]; then
                (cd /src/tools && \"\$HOSTTOOLS/bmp2rb\" -f 7 -h . iaudio_bl_flash.bmp \
                    > iaudio_bl_flash.c)
            fi
            gcc -O2 /src/tools/scramble.c /src/tools/iriver.c /src/tools/mi4.c \
                /src/tools/gigabeat.c /src/tools/gigabeats.c /src/tools/telechips.c \
                /src/tools/iaudio_bl_flash.c /src/tools/creative.c \
                /src/tools/hmac-sha1.c /src/tools/rkw.c \
                -o \"\$HOSTTOOLS/scramble\"
            gcc -O2 /src/tools/mkboot.c -o \"\$HOSTTOOLS/mkboot\"
            echo '    Host tools compiled for Linux x86_64'
        fi

        # Reconfigure if autoconf.h is missing or was generated for the wrong target.
        mkdir -p $BUILD_DIR
        if ! grep -q 'ARMHFHOST' $BUILD_DIR/autoconf.h 2>/dev/null; then
            (cd $BUILD_DIR && printf '208\nN\n' | ../tools/configure)
        else
            echo '    $BUILD_DIR already configured for ARMHFHOST, skipping configure'
        fi

        # Patch BMP2RB_MONO/NATIVE in the Makefile to use the Linux bmp2rb.
        sed -i \"s|/src/tools/bmp2rb|\$HOSTTOOLS/bmp2rb|g\" $BUILD_DIR/Makefile

        # Pre-create output dirs that ar rcs requires before the parallel build
        # creates them via object-file compilation.
        mkdir -p $BUILD_DIR/lib $BUILD_DIR/firmware $BUILD_DIR/apps

        (cd $BUILD_DIR && make -j\"\$NCPU\" TOOLSDIR=\"\$HOSTTOOLS\" lib)

        echo '==> Step 2.05: Add __isoc23_* compat shim to libfirmware.a'
        # The cross-rs image ships glibc >= 2.38, whose headers redirect
        # strtol/strtoul/strtoll/fscanf/sscanf/scanf to __isoc23_* variants
        # (enabled by the -D_GNU_SOURCE=1 the ARMHFHOST config sets). Real
        # ARMv6 targets run older glibc (< 2.38) that lacks those symbols, so
        # forward them to the classic functions and bake the shim into
        # libfirmware.a. Keeps the binary runnable on glibc >= 2.31.
        LIBFW=\"/src/$BUILD_DIR/firmware/libfirmware.a\"
        # Match the ARMHFHOST firmware's exact ARM flags (armhfhostcc in
        # tools/configure): ARMv6 + ARM mode + hard-float VFP ABI. The hard
        # float-abi is the gnueabihf default (so gnu/stubs-hard.h resolves) and
        # tags the object with the same EF_ARM_ABI_FLOAT_HARD flag as the rest
        # of libfirmware.a, avoiding a float-ABI-mismatch link error.
        arm-linux-gnueabihf-gcc -c -O2 -std=gnu11 \
            -march=armv6 -marm -mfpu=vfp -mfloat-abi=hard \
            /src/scripts/isoc23-compat.c -o /tmp/isoc23-compat.o
        # Match the firmware objects' ARMv6 attributes (see Step 2.7 notes on
        # LLD taking the MAX arch and emitting ARMv7 thunks otherwise).
        arm-linux-gnueabihf-objcopy --remove-section=.ARM.attributes \
            /tmp/isoc23-compat.o 2>/dev/null || true
        arm-linux-gnueabihf-ar r \"\$LIBFW\" /tmp/isoc23-compat.o
        echo '    __isoc23_* shim archived into libfirmware.a'

        echo '==> Step 2.1: Build libcodec.a'
        LIBCODEC=\"/src/$BUILD_DIR/lib/rbcodec/codecs/libcodec.a\"
        if [ ! -f \"\$LIBCODEC\" ]; then
            (cd $BUILD_DIR && make -j\"\$NCPU\" TOOLSDIR=\"\$HOSTTOOLS\" \"\$LIBCODEC\")
        fi

        echo '==> Step 2.5: Extract per-codec .o files for direct Zig linking'
        CODEC_DIR=\"/src/$BUILD_DIR/lib/rbcodec/codecs\"
        OBJECTS_DIR=\"\$CODEC_DIR/codec-objects\"
        mkdir -p \"\$OBJECTS_DIR\"
        for name in a52 a52_rm aac aac_bsf adx aiff alac ape \
                    atrac3_oma atrac3_rm au cook flac mod mpa mpc \
                    opus raac shorten smaf speex tta vorbis vox \
                    wav wav64 wavpack wma wmapro; do
            obj_dir=\"\$OBJECTS_DIR/\$name\"
            rm -rf \"\$obj_dir\" && mkdir -p \"\$obj_dir\"
            (cd \"\$obj_dir\" && ar x \"\$CODEC_DIR/\$name.a\")
        done
        echo '    Done — codec objects extracted'

        echo '==> Step 2.6: Namespace ogg_* symbols in libopus.a'
        LIBOPUS_A=\"\$CODEC_DIR/libopus.a\"
        OPUS_O=\"\$OBJECTS_DIR/opus/opus.o\"
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
        for sym in \"\${OGG_SYMS[@]}\"; do
            REDEFINE_ARGS+=(--redefine-sym \"\${sym}=libopus_\${sym}\")
        done
        TMP_LIBOPUS=\"\$(mktemp -d)\"
        (cd \"\$TMP_LIBOPUS\" && ar x \"\$LIBOPUS_A\")
        for obj in \"\$TMP_LIBOPUS\"/*.o; do
            [ -f \"\$obj\" ] && arm-linux-gnueabihf-objcopy \"\${REDEFINE_ARGS[@]}\" \"\$obj\" 2>/dev/null || true
        done
        rm -f \"\$LIBOPUS_A\"
        ar rcs \"\$LIBOPUS_A\" \"\$TMP_LIBOPUS\"/*.o
        rm -rf \"\$TMP_LIBOPUS\"
        [ -f \"\$OPUS_O\" ] && arm-linux-gnueabihf-objcopy \"\${REDEFINE_ARGS[@]}\" \"\$OPUS_O\" 2>/dev/null || true
        echo \"    Done — ogg_* symbols namespaced in libopus.a + opus.o\"

        echo '==> Step 2.7: Copy ARM sysroot libraries for Zig cross-linker'
        SYSLIBS=/src/$BUILD_DIR/syslibs
        mkdir -p \"\$SYSLIBS\"
        ARMLIB=/usr/lib/arm-linux-gnueabihf
        # Copy the unversioned .so linker stub (not the .a static archive) so
        # Zig uses dynamic linking and avoids transitive static deps.
        # Use cp -L to dereference symlinks — the -dev packages install
        # libXXX.so as a symlink into /lib/arm-linux-gnueabihf/ which would be
        # a broken symlink on the host if copied with -P.
        for lib in dbus-1 asound unwind; do
            so=\"\$ARMLIB/lib\${lib}.so\"
            if [ -L \"\$so\" ] || [ -f \"\$so\" ]; then
                cp -L \"\$so\" \"\$SYSLIBS/lib\${lib}.so\" 2>/dev/null || true
            fi
        done
        # Ubuntu armhf .so files carry Tag_CPU_arch = v7 in .ARM.attributes.
        # LLD takes the MAX arch across all inputs, so one ARMv7 input makes
        # LLD emit __ARMv7ABSLongThunk__ (movw/movt) which SIGILL on ARMv6.
        # Strip .ARM.attributes so LLD falls back to ARMv4/5-style thunks
        # (ldr pc, =addr) that are valid on ARMv6 and all later cores.
        for lib in dbus-1 asound unwind; do
            so=\"\$SYSLIBS/lib\${lib}.so\"
            [ -f \"\$so\" ] && arm-linux-gnueabihf-objcopy \
                --remove-section=.ARM.attributes \"\$so\" 2>/dev/null || true
        done
        echo \"    Copied + stripped .ARM.attributes: \$(ls \$SYSLIBS)\"
    "

echo "==> Step 3: Build S3 admin web UI bundle"
# rust-embed in crates/s3/src/admin.rs reads crates/s3/s3webui/dist/ at
# compile time. The directory is gitignored, so without this step a fresh
# checkout fails the rockbox-server build with "no such file or directory".
(cd crates/s3/s3webui && bun install --frozen-lockfile && bun run build)

echo "==> Step 4: Build Rust crates with cross (fts5 + alsa-sink, no typesense subprocess)"
cross build --release \
    --target "$RUST_TARGET" \
    --features fts5,alsa-sink \
    -p rockbox-cli
cross build --release \
    --target "$RUST_TARGET" \
    --features fts5 \
    -p rockbox-server

echo "==> Step 5: Link rockboxd with Zig for arm-linux-gnueabihf"
# Clear Zig's build cache so it re-links even when input timestamps are unchanged.
rm -rf zig/.zig-cache zig/zig-out
(cd zig && zig build \
    -Dtarget=arm-linux-gnueabihf \
    -Dcpu=arm1176jzf_s \
    -Dheadless=true \
    -Dfw-dir=../build-armhf \
    -Drust-triple="$RUST_TARGET" \
    -Dsyslibs-dir=../build-armhf/syslibs \
    -Doptimize=ReleaseFast)

echo "==> Diagnostic: check ARM CPU arch attribute in final binary"
docker run --rm --platform linux/amd64 \
    -v "$ROOTDIR:/src" \
    "$DOCKER_IMAGE" \
    bash -c "arm-linux-gnueabihf-readelf -A /src/zig/zig-out/bin/rockboxd 2>/dev/null \
        | grep -E 'Tag_CPU_arch|File Attributes|Section Attributes' | head -20 \
        || echo '(readelf not found or no .ARM.attributes section)'"

echo ""
echo "Build complete: zig/zig-out/bin/rockboxd (ARM hard-float)"
