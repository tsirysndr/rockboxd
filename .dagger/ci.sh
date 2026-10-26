#!/usr/bin/env bash
# Shared thin adapter for GitHub Actions. Build logic lives in the module.
set -euo pipefail
cd "$(dirname "$0")/.."
job=${1:?Expected a CI job; see .dagger/README.md}
case "$job" in
  linux-aarch64-*) platform=${PLATFORM:-linux/arm64} ;;
  *) platform=${PLATFORM:-linux/amd64} ;;
esac
args=(--source "${SOURCE:-.}" --platform "$platform")
case "$job" in
  sdl) function=sdl-build ;;
  linux-x86_64-check) function=linux-check; args+=() ;;
  linux-aarch64-check) function=linux-check; args+=() ;;
  linux-x86_64-release) function=linux-release; args+=(--tag "${TAG:-latest}") ;;
  linux-aarch64-release) function=linux-release; args+=(--tag "${TAG:-latest}") ;;
  linux-armhf-release) function=linux-armhf-release; args+=(--tag "${TAG:-latest}") ;;
  linux-release) function=build; args+=(--tag "${TAG:-latest}") ;;
  embed) function=embed; args+=(--tag "${TAG:-latest}") ;;
  desktop) function=desktop; args+=(--tag "${TAG:-latest}") ;;
  go-test) function=go-test; args+=() ;;
  ffi) function=ffi; args+=() ;;
  nif) function=nif; args+=() ;;
  android-check) function=android-check; args+=(--android-sdk "${ANDROID_SDK_ROOT:?Set ANDROID_SDK_ROOT to an SDK containing an NDK}") ;;
  android-apk) function=android-apk; args+=(--features "${FEATURES:-embedded-daemon}" --architectures "${ARCHITECTURES:-arm64-v8a}" --android-sdk "${ANDROID_SDK_ROOT:?Set ANDROID_SDK_ROOT to an SDK containing an NDK}") ;;
  android-release) function=android-release; args+=(--tag "${TAG:-latest}" --android-sdk "${ANDROID_SDK_ROOT:?Set ANDROID_SDK_ROOT to an SDK containing an NDK}") ;;
  android-ffi) function=android-ffi; args+=(--abi "${ABI:-arm64-v8a}" --android-sdk "${ANDROID_SDK_ROOT:?Set ANDROID_SDK_ROOT to an SDK containing an NDK}") ;;
  package-ruby) function=ruby; args+=(--artifacts "${ARTIFACTS:-artifacts}" --version "${BINDING_VERSION:?Set BINDING_VERSION}") ;;
  package-python) function=python; args+=(--artifacts "${ARTIFACTS:-artifacts}" --version "${BINDING_VERSION:?Set BINDING_VERSION}") ;;
  package-npm) function=npm; args+=(--artifacts "${ARTIFACTS:-artifacts}" --version "${BINDING_VERSION:?Set BINDING_VERSION}") ;;
  format) function=format; args+=() ;;
  web-test) function=web-test; args+=() ;;
  deb) function=deb; args+=(--tag "${TAG:-latest}") ;;
  rpm) function=rpm; args+=(--tag "${TAG:-latest}") ;;
  legacy-release) function=release; args+=(--tag "${TAG:-latest}") ;;
  electron) function=electron; args+=() ;;
  storybook) function=storybook; args+=() ;;
  *) echo "Unknown job: $job" >&2; exit 2 ;;
esac
# Keep paths relative to the checkout so existing upload actions stay unchanged.
dagger api call -m .dagger/ci "$function" "${args[@]}" export --path "${OUTPUT:-.}"
