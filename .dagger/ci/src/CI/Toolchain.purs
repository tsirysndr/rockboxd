module CI.Toolchain where

import Prelude
import CI.Base as Base
import CI.Graph as G
import Dagger (Client)
import Dagger.Container as C
import Dagger.Query as Q
import Dagger.Types (Container, Directory, Platform)

data Toolchain = Full | Cross | Android | Rust | Go | Erlang | Ruby | Python | Npm | Web | Packages

derive instance eqToolchain :: Eq Toolchain

image :: Toolchain -> String
image = case _ of
  Go -> "golang:1.26.2-bookworm"
  Erlang -> "erlang:27"
  Ruby -> "ruby:3.3-bookworm"
  Python -> "python:3.12-bookworm"
  _ -> "ubuntu:24.04"

apt :: Array String -> Container -> Container
apt packages = G.exec [ "apt-get", "update" ] >>> G.exec ([ "apt-get", "install", "-y", "--no-install-recommends" ] <> packages)

prepare :: Client -> Platform -> String -> Directory -> Toolchain -> Container
prepare client platform arch _source family =
  let
    base = if family == Cross then crossBase client platform else Base.container client platform (image family)
    fromImage name = Base.container client platform name
    node = fromImage "node:22-bookworm-slim"
    rust = fromImage "rust:bookworm"
    bun = fromImage "oven/bun:1.4.3"
    deno = fromImage "ghcr.io/denoland/deno:2.9.7"
    -- Deno enables libffi-sys/system, including in host build scripts.
    baseTools = base
      # C.withEnvVariable "DEBIAN_FRONTEND" "noninteractive"
      # apt [ "build-essential", "ca-certificates", "git", "pkg-config", "xz-utils", "unzip", "zip", "cmake", "perl", "python3", "file", "binutils", "curl", "libffi-dev" ]
      # C.withDirectory "/usr/local" (C.directory "/usr/local" node)
      # C.withEnvVariable "PATH" "/usr/local/cargo/bin:/usr/local/go/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    withRust = case family of
      Web -> baseTools
      Npm -> baseTools
      Python -> baseTools
      Ruby -> baseTools
      Packages -> baseTools
      _ -> baseTools
        # C.withDirectory "/usr/local/cargo" (C.directory "/usr/local/cargo" rust)
        # C.withDirectory "/usr/local/rustup" (C.directory "/usr/local/rustup" rust)
        # C.withEnvVariable "CARGO_HOME" "/usr/local/cargo"
        # C.withEnvVariable "RUSTUP_HOME" "/usr/local/rustup"
    withBun = case family of
      Full -> installBun withRust
      Cross -> installBun withRust
      Android -> installBun withRust
      Web -> installBun withRust
      Npm -> installBun withRust
      _ -> withRust
    installBun container = container # C.withFile "/usr/local/bin/bun" (C.file "/usr/local/bin/bun" bun)
      # G.exec [ "ln", "-sf", "/usr/local/bin/bun", "/usr/local/bin/bunx" ]
    withZig = if family == Full || family == Cross || family == Android then withBun
      # C.withFile "/usr/local/bin/deno" (C.file "/usr/bin/deno" deno)
      # C.withFile "/tmp/zig.tar.xz" (Q.http ("https://ziglang.org/download/0.16.0/zig-" <> arch <> "-linux-0.16.0.tar.xz") client)
      # G.exec [ "mkdir", "-p", "/opt/zig" ]
      # G.exec [ "tar", "-xJf", "/tmp/zig.tar.xz", "--strip-components=1", "-C", "/opt/zig" ]
      # G.exec [ "ln", "-sf", "/opt/zig/zig", "/usr/local/bin/zig" ]
      else withBun
  in case family of
    Android -> withZig
      # apt [ "openjdk-17-jdk-headless", "clang", "libclang-dev", "protobuf-compiler", "ninja-build" ]
      # G.exec [ "rustup", "target", "add", "aarch64-linux-android", "armv7-linux-androideabi", "x86_64-linux-android" ]
      # G.exec [ "cargo", "install", "cargo-ndk", "--locked" ]
    Cross -> withZig # G.exec [ "rustup", "target", "add", "arm-unknown-linux-gnueabihf" ]
    Rust -> withZig # G.exec [ "rustup", "component", "add", "rustfmt" ]
    Go -> withZig # C.withNewFile "/root/.asoundrc" "pcm.!default { type null }\n"
    Packages -> withZig # apt [ "dpkg-dev", "rpm" ]
    _ -> withZig

-- The ARMv6 toolchain is itself a Dagger graph, with no Dockerfile RUN scripts.
crossBase :: Client -> Platform -> Container
crossBase client platform = Base.container client platform "ghcr.io/cross-rs/arm-unknown-linux-gnueabihf:main"
  # G.exec [ "dpkg", "--add-architecture", "armhf" ]
  # G.exec [ "rm", "-f", "/etc/apt/preferences.d/all-packages" ]
  # apt [ "curl", "unzip", "zip", "cmake", "perl", "ca-certificates", "gcc-arm-linux-gnueabihf", "g++-arm-linux-gnueabihf", "binutils-arm-linux-gnueabihf", "libasound2-dev:armhf", "libdbus-1-dev:armhf", "libunwind-dev:armhf" ]
  # C.withFile "/tmp/protoc.zip" (Q.http "https://github.com/protocolbuffers/protobuf/releases/download/v25.1/protoc-25.1-linux-x86_64.zip" client)
  # G.exec [ "unzip", "-qo", "/tmp/protoc.zip", "-d", "/usr/local" ]
  # C.withEnvVariable "PKG_CONFIG_PATH" "/usr/lib/arm-linux-gnueabihf/pkgconfig"
  # C.withEnvVariable "PKG_CONFIG_LIBDIR" "/usr/lib/arm-linux-gnueabihf/pkgconfig"
  # C.withEnvVariable "PKG_CONFIG_SYSROOT_DIR" "/"
  # C.withEnvVariable "PKG_CONFIG_ALLOW_CROSS" "1"
  # C.withEnvVariable "CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_RUSTFLAGS" "-C target-cpu=arm1176jzf-s -C link-arg=-Wl,-rpath-link=/usr/lib/arm-linux-gnueabihf"
