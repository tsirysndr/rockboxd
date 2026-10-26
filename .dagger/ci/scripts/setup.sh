#!/usr/bin/env bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends bash build-essential curl ca-certificates git pkg-config xz-utils unzip zip wget cmake perl python3 file jq
# Preserve the existing workflows' rolling stable Rust toolchain.
if [[ "$TOOLCHAIN" != web && "$TOOLCHAIN" != npm && "$TOOLCHAIN" != python && "$TOOLCHAIN" != ruby && "$TOOLCHAIN" != packages ]]; then
  curl --fail --show-error --silent https://sh.rustup.rs -o /tmp/rustup.sh
  bash /tmp/rustup.sh -y --profile minimal --default-toolchain stable
fi
if [[ "$TOOLCHAIN" == rust ]]; then rustup component add rustfmt; fi
case "$TOOLCHAIN" in
  full|cross|android|web|npm)
    curl --fail --show-error --silent https://bun.sh/install -o /tmp/install-bun.sh
    bash /tmp/install-bun.sh bun-v1.4.3
    ln -s /root/.bun/bin/bun /usr/local/bin/bun
    ln -s /root/.bun/bin/bun /usr/local/bin/bunx
    ;;
esac
case "$TOOLCHAIN" in
  full|cross|android)
    # Resolve the latest Deno 2 release, matching the former v2.x setup action.
    curl -fsSL 'https://api.github.com/repos/denoland/deno/releases?per_page=100' -o /tmp/deno-releases.json
    deno_version=$(jq -er '[.[] | select(.prerelease == false and (.tag_name | startswith("v2.")))][0].tag_name' /tmp/deno-releases.json)
    curl -fsSL https://deno.land/install.sh -o /tmp/install-deno.sh
    bash /tmp/install-deno.sh "$deno_version"
    ln -s /root/.deno/bin/deno /usr/local/bin/deno
    zig_arch=$(uname -m)
    curl -fsSL "https://ziglang.org/download/0.16.0/zig-${zig_arch}-linux-0.16.0.tar.xz" -o /tmp/zig.tar.xz
    mkdir -p /opt/zig
    tar -xJf /tmp/zig.tar.xz --strip-components=1 -C /opt/zig
    ln -s /opt/zig/zig /usr/local/bin/zig
    ;;
esac
if [[ "$TOOLCHAIN" == android ]]; then
  apt-get install -y --no-install-recommends openjdk-17-jdk-headless clang libclang-dev
  rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
fi
if [[ "$TOOLCHAIN" == cross ]]; then
  rustup target add arm-unknown-linux-gnueabihf
fi
if [[ "$TOOLCHAIN" == go ]]; then
  # A software ALSA device works without kernel modules or privileged containers.
  printf 'pcm.!default { type null }\n' > /root/.asoundrc
fi
if [[ "$TOOLCHAIN" == packages ]]; then apt-get install -y dpkg-dev rpm; fi
rm -rf /var/lib/apt/lists/* /tmp/zig.tar.xz
