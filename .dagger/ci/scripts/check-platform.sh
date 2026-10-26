#!/usr/bin/env bash
# Architecture-named jobs must never silently produce mislabeled release files.
case "$JOB:$PLATFORM" in
  linux-x86_64-*:linux/arm64|linux-aarch64-*:linux/amd64|linux-release:linux/arm64|android-*:linux/arm64)
    echo "$JOB does not support $PLATFORM" >&2; exit 2 ;;
esac
