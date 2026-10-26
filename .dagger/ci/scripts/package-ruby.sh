#!/usr/bin/env bash
# Ported from bindings-release.yml / ruby. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Stamp gem version from release tag
(
cd "bindings/ruby"
set -euxo pipefail
# gem build reads RockboxFFI::VERSION from version.rb; keep it in lock-
# step with the -o filename (BINDING_VERSION) so the gem's internal
# version matches its filename and the release tag.
printf 'module RockboxFFI\n  VERSION = "%s"\nend\n' "$BINDING_VERSION" \
  > lib/rockbox_ffi/version.rb

)
load_step_env

# Build platform gems
(
cd "bindings/ruby"
set -euxo pipefail
declare -A PLAT=(
  [darwin-arm64]=arm64-darwin
  [darwin-x64]=x86_64-darwin
  [linux-x64]=x86_64-linux
  [linux-arm64]=aarch64-linux
  [freebsd-x64]=x86_64-freebsd
  [netbsd-x64]=x86_64-netbsd
)
declare -A LIB=(
  [darwin-arm64]=librockbox_ffi.dylib
  [darwin-x64]=librockbox_ffi.dylib
  [linux-x64]=librockbox_ffi.so
  [linux-arm64]=librockbox_ffi.so
  [freebsd-x64]=librockbox_ffi.so
  [netbsd-x64]=librockbox_ffi.so
)
mkdir -p pkg
for target in "${!PLAT[@]}"; do
  src="$GITHUB_WORKSPACE/artifacts/librockbox_ffi-${target}/${LIB[$target]}"
  if [ ! -f "$src" ]; then echo "skip ${target}: no artifact"; continue; fi
  rm -rf vendor && mkdir vendor
  cp "$src" vendor/
  ROCKBOX_GEM_PLATFORM="${PLAT[$target]}" gem build rockbox_ffi.gemspec -o "pkg/rockbox_ffi-${BINDING_VERSION}-${PLAT[$target]}.gem"
done
rm -rf vendor
# source gem (fallback for unlisted platforms / repo checkouts)
gem build rockbox_ffi.gemspec -o "pkg/rockbox_ffi-${BINDING_VERSION}.gem"
ls -l pkg

)
load_step_env

collect 'bindings/ruby/pkg/*.gem'
