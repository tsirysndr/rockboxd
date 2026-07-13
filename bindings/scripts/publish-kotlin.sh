#!/usr/bin/env bash
#
# Publish the Kotlin artifact to Maven Central (Sonatype Central Portal). Built
# from source here with the vanniktech maven-publish plugin
# (`gradle publishToMavenCentral`). The jar bundles the prebuilt librockbox_ffi
# for every desktop OS/arch under src/main/resources/native/<target>/, plus the
# Android arm64-v8a + x86_64 .so under src/main/resources/lib/<abi>/ (so an
# Android consumer gets them packed into its APK by AGP) — all staged from the
# GitHub Release by fetch-libs.sh, so no local Rust build is involved.
#
# Credentials (in ~/.gradle/gradle.properties or ORG_GRADLE_PROJECT_* env):
#   mavenCentralUsername / mavenCentralPassword      (a Central Portal token)
#   signingInMemoryKey / signingInMemoryKeyPassword  (ASCII-armored GPG key)
#
# Usage:
#   bindings/scripts/publish-kotlin.sh [--tag <tag>] [--repo <owner/repo>] [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"
ROOT="$(cd "$COMMON_DIR/../.." && pwd)"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }
resolve_repo_and_tag

cd "$ROOT/bindings/kotlin"
# Prefer a project wrapper if one appears later; fall back to system gradle.
if [ -x ./gradlew ]; then GRADLE=./gradlew; else
  command -v gradle >/dev/null 2>&1 || { echo "error: gradle not found (and no ./gradlew wrapper)" >&2; exit 1; }
  GRADLE=gradle
fi

echo "== Kotlin -> Maven Central =="
# Stage every platform's prebuilt shared lib into src/main/resources/native/<t>/.
run "$COMMON_DIR/fetch-libs.sh" --all --tag "$TAG" --repo "$REPO"
# The version is owned by build.gradle.kts (libVersion); bump it there.
echo "  publish io.github.tsirysndr:rockbox-ffi"
run "$GRADLE" publishToMavenCentral --no-configuration-cache
echo "Done."
