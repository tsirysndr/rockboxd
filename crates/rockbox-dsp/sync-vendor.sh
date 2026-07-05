#!/usr/bin/env bash
# Re-sync vendor/ from the rockbox source tree before publishing.
# The published crate builds from vendor/; in-repo builds use
# lib/rbcodec directly (see build.rs).
set -euo pipefail
cd "$(dirname "$0")"

cp ../../lib/rbcodec/dsp/*.c ../../lib/rbcodec/dsp/*.h vendor/dsp/
cp ../../lib/fixedpoint/fixedpoint.c ../../lib/fixedpoint/fixedpoint.h vendor/fixedpoint/
cp ../../lib/rbcodec/platform.h vendor/platform.h
cp ../../apps/fracmul.h vendor/fracmul.h

echo "vendor/ synced."
