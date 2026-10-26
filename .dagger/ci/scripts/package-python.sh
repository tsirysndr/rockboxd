#!/usr/bin/env bash
# Ported from bindings-release.yml / python. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Stamp wheel version from release tag
(
cd "bindings/python"
set -euxo pipefail
# Pin the first (project) `version = "..."` in pyproject.toml to the tag.
sed -i "0,/^version = .*/s//version = \"${BINDING_VERSION}\"/" pyproject.toml
grep -m1 '^version = ' pyproject.toml

)
load_step_env

# Install build tooling
(
pip install build wheel
)
load_step_env

# Build wheels
(
cd "bindings/python"
set -euxo pipefail
declare -A LIB=(
  [darwin-arm64]=librockbox_ffi.dylib
  [darwin-x64]=librockbox_ffi.dylib
  [linux-x64]=librockbox_ffi.so
  [linux-arm64]=librockbox_ffi.so
  [freebsd-x64]=librockbox_ffi.so
  [netbsd-x64]=librockbox_ffi.so
)
declare -A TAG=(
  [darwin-arm64]=macosx_11_0_arm64
  [darwin-x64]=macosx_10_12_x86_64
  # Both Linux runners are ubuntu-24.04 (glibc 2.39); tag one manylinux
  # level below so the wheels also install on glibc 2.38 hosts.
  # libasound is a system dependency, not vendored (see note above).
  [linux-x64]=manylinux_2_38_x86_64
  [linux-arm64]=manylinux_2_38_aarch64
  [freebsd-x64]=freebsd_13_amd64
  [netbsd-x64]=netbsd_10_amd64
)
mkdir -p wheelhouse
python -m build --sdist --outdir wheelhouse
for target in darwin-arm64 darwin-x64 linux-x64 linux-arm64 freebsd-x64 netbsd-x64; do
  src="$GITHUB_WORKSPACE/artifacts/librockbox_ffi-${target}/${LIB[$target]}"
  if [ ! -f "$src" ]; then echo "skip ${target}: no artifact"; continue; fi
  rm -rf src/rockbox_ffi/_lib dist && mkdir -p src/rockbox_ffi/_lib
  cp "$src" src/rockbox_ffi/_lib/
  python -m build --wheel --outdir dist
  python -m wheel tags --platform-tag "${TAG[$target]}" --remove dist/*.whl
  cp dist/*.whl wheelhouse/
done
rm -rf src/rockbox_ffi/_lib
ls -l wheelhouse

)
load_step_env

collect 'bindings/python/wheelhouse/*'
