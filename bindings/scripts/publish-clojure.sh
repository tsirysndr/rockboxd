#!/usr/bin/env bash
#
# Publish the Clojure jar to Clojars. Unlike the npm/ruby/python scripts (which
# push assets CI already packed), the JVM bindings are built from source here:
# `clojure -T:build deploy` builds the jar and deploys it to Clojars. The jar
# bundles the prebuilt librockbox_ffi for every OS/arch under
# resources/native/<target>/ — staged from the GitHub Release by fetch-libs.sh
# (a JVM jar ships every platform in one artifact), so no local Rust build is
# involved for this binding.
#
# Authenticate first:
#   export CLOJARS_USERNAME=<user> CLOJARS_PASSWORD=<clojars-deploy-token>
# The io.github.tsirysndr group must be verified on Clojars (Verified Groups).
#
# Usage:
#   bindings/scripts/publish-clojure.sh [--tag <tag>] [--repo <owner/repo>] [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"
ROOT="$(cd "$COMMON_DIR/../.." && pwd)"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }
resolve_repo_and_tag

command -v clojure >/dev/null 2>&1 || { echo "error: clojure CLI not found — https://clojure.org/guides/install_clojure" >&2; exit 1; }

echo "== Clojure -> Clojars =="
# Stage every platform's prebuilt shared lib into resources/native/<t>/ so the
# jar is cross-platform.
run "$COMMON_DIR/fetch-libs.sh" --all --tag "$TAG" --repo "$REPO"
cd "$ROOT/bindings/clojure"
# The version is owned by build.clj (VERSION env, default 0.2.0); bump it there.
echo "  deploy io.github.tsirysndr/rockbox-clj-ffi"
run clojure -T:build deploy
echo "Done."
