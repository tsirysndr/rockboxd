#!/usr/bin/env bash
# Re-sync vendor/ from the rockbox source tree before publishing.
# The published crate builds from vendor/; in-repo builds use
# lib/rbcodec directly (see build.rs).
set -euo pipefail
cd "$(dirname "$0")"

mkdir -p vendor/metadata vendor/fixedpoint vendor/codecs/librm vendor/codecs/libasf

cp ../../lib/rbcodec/metadata/*.c ../../lib/rbcodec/metadata/*.h vendor/metadata/
cp ../../lib/fixedpoint/fixedpoint.c ../../lib/fixedpoint/fixedpoint.h vendor/fixedpoint/
cp ../../lib/rbcodec/platform.h vendor/platform.h
# asf.c / rm.c include <codecs/libasf/asf.h> / <codecs/librm/rm.h>
cp ../../lib/rbcodec/codecs/librm/rm.h ../../lib/rbcodec/codecs/librm/bytestream.h vendor/codecs/librm/
cp ../../lib/rbcodec/codecs/libasf/asf.h vendor/codecs/libasf/

echo "vendor/ synced."
