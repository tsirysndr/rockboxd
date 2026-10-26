#!/usr/bin/env bash
# Source after mounting the SDK: host absolute paths must not leak into the container.
export ANDROID_HOME=/opt/android-sdk ANDROID_SDK_ROOT=/opt/android-sdk
ANDROID_NDK_HOME=$(find "$ANDROID_SDK_ROOT/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -1)
[[ -n "$ANDROID_NDK_HOME" ]] || { echo 'The supplied Android SDK has no side-by-side NDK' >&2; exit 2; }
export ANDROID_NDK_HOME ANDROID_NDK_ROOT="$ANDROID_NDK_HOME"
export JAVA_HOME=/usr/lib/jvm/java-17-openjdk-amd64
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$PATH"
