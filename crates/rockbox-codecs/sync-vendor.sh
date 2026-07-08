#!/usr/bin/env bash
# Re-sync vendor/ from the rockbox source tree before publishing.
# The published crate builds from vendor/ (mirroring the repo layout);
# in-repo builds use lib/ directly (see build.rs).
set -euo pipefail
cd "$(dirname "$0")"

rm -rf vendor
mkdir -p vendor/lib/rbcodec/codecs vendor/lib/rbcodec/metadata \
         vendor/lib/rbcodec/dsp vendor/lib/fixedpoint vendor/lib/tlsf/src

# codec wrappers + entry stub + api headers (keep in sync with CODECS in
# build.rs)
for c in wav aiff au smaf vox wav64 flac shorten wavpack alac ape tta \
         mpa vorbis aac aac_bsf opus mpc speex wma wmapro a52 \
         cook raac a52_rm atrac3_rm atrac3_oma adx mod; do
    cp "../../lib/rbcodec/codecs/$c.c" vendor/lib/rbcodec/codecs/
done
cp ../../lib/rbcodec/codecs/codec_crt0.c \
   ../../lib/rbcodec/codecs/*.h \
   vendor/lib/rbcodec/codecs/

# shared codec helper lib + decoder libraries (keep in sync with LIBS in
# build.rs; SOURCES files are parsed by build.rs and must be kept)
for d in lib libpcm libffmpegFLAC libmad libtremor libwavpack libalac \
         libm4a libfaad libasf librm liba52 libatrac libcook libwma \
         libwmapro libmusepack libspeex libtta libopus; do
    cp -R "../../lib/rbcodec/codecs/$d" vendor/lib/rbcodec/codecs/
done
mkdir -p vendor/lib/rbcodec/codecs/demac
cp -R ../../lib/rbcodec/codecs/demac/libdemac vendor/lib/rbcodec/codecs/demac/

# platform + headers the codec api pulls in
cp ../../lib/rbcodec/platform.h vendor/lib/rbcodec/
cp ../../lib/rbcodec/metadata/*.h vendor/lib/rbcodec/metadata/
cp ../../lib/rbcodec/dsp/*.h vendor/lib/rbcodec/dsp/
cp ../../lib/fixedpoint/fixedpoint.h vendor/lib/fixedpoint/
cp ../../lib/tlsf/src/tlsf.c ../../lib/tlsf/src/*.h vendor/lib/tlsf/src/

# drop assembly and make fragments — hosted builds take the C paths;
# SOURCES files stay (build.rs reads them)
find vendor \( -name '*.S' -o -name '*.make' \) -delete

echo "vendor/ synced: $(find vendor -type f | wc -l | tr -d ' ') files"
