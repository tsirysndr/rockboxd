#!/usr/bin/env bash
# Ported from android-release.yml / build. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh
export CARGO_TERM_COLOR=always
export RUSTFLAGS='-A warnings'

# Install system dependencies
(
apt-get update -qq
apt-get install -y --no-install-recommends \
  protobuf-compiler cmake ninja-build

)
load_step_env

# Add Android Rust targets
(
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
)
load_step_env

# Install cargo-ndk
(
cargo install cargo-ndk --locked
)
load_step_env

# Build Web UI
(
cd webui/rockbox
deno install
deno run build
deno clean
rm -rf node_modules

)
load_step_env

# Install admin UI dependencies
(
cd "crates/s3/s3webui"
bun install --frozen-lockfile
)
load_step_env

# Build admin UI
(
cd "crates/s3/s3webui"
bun run build
)
load_step_env

# Build Android firmware (C static libs)
(
mkdir -p build-android-arm64
rm -f build-android-arm64/make.dep
cd build-android-arm64
printf "205\n" | \
  ANDROID_TARGET_ABI=arm64-v8a ANDROID_API_LEVEL=26 \
  ARG_LCDWIDTH=320 ARG_LCDHEIGHT=480 \
  ../tools/configure
make -j$(nproc) lib

)
load_step_env

# Install Expo dependencies
(
cd "expo"
bun install
)
load_step_env

# Generate Android project (expo prebuild)
(
cd "expo"
bunx expo prebuild --platform android --no-install
)
load_step_env

# Write local.properties
(
echo "sdk.dir=$ANDROID_SDK_ROOT" > expo/android/local.properties
)
load_step_env

# Build Rust cdylib (rockbox-expo)
(
export FEATURES=embedded-daemon
export PROFILE=release
bash expo/modules/rockbox-rpc/scripts/build-android.sh
)
load_step_env

# Show built .so files
(
find expo/modules/rockbox-rpc/android/src/main/jniLibs -name '*.so' -exec ls -lh {} \;
)
load_step_env

# Build APK
(
cd "expo/android"
./gradlew assembleRelease \
  -PreactNativeArchitectures=arm64-v8a \
  --no-daemon \
  --stacktrace

)
load_step_env

# Show APK
(
find expo/android/app/build/outputs/apk -name '*.apk' -exec ls -lh {} \;
)
load_step_env

collect expo/android/app/build/outputs/apk/release/app-release.apk
