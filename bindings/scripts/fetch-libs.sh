#!/usr/bin/env bash
#
# Download prebuilt librockbox_ffi shared libraries from a GitHub Release and
# stage them into each binding's bundled location, so you can build and publish
# the Ruby / Python / npm packages locally without a Rust toolchain.
#
# Usage:
#   bindings/scripts/fetch-libs.sh [--tag <tag>] [--target <target> | --all] [--repo <owner/repo>]
#
#   (no args)            stage the current host's lib into all three bindings
#   --target <t>         stage a specific target (see list below) instead of host
#   --all                stage every target into typescript/npm/<t>/ (npm ships
#                        one package per platform); Ruby/Python are single-slot,
#                        so the host target is also staged for them
#   --tag <t>            release tag to pull from (default: latest bindings-v*)
#   --repo <owner/repo>  GitHub repo hosting the release (default: origin remote,
#                        or the GH_REPO env var). Needed because gh may resolve a
#                        different default repo when several remotes exist.
#
#   Targets: darwin-arm64 darwin-x64 linux-x64 linux-arm64 freebsd-x64 netbsd-x64
#
# Staging locations (per target <t>, ext = dylib on macOS, so elsewhere):
#   ruby       -> bindings/ruby/vendor/librockbox_ffi.<ext>
#   python     -> bindings/python/src/rockbox_ffi/_lib/librockbox_ffi.<ext>
#   typescript -> bindings/typescript/npm/<t>/librockbox_ffi.<ext>
#   kotlin     -> bindings/kotlin/src/main/resources/native/<t>/librockbox_ffi.<ext>
#   clojure    -> bindings/clojure/resources/native/<t>/librockbox_ffi.<ext>
#
# The JVM bindings (kotlin, clojure) bundle EVERY target in a single jar, so
# --all stages all targets for them; host mode stages just the host target.
#
# Requires the GitHub CLI (`gh`), authenticated and run from the repo checkout.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINDINGS_DIR="$(dirname "$SCRIPT_DIR")"

ALL=0
TARGET=""
TAG="${ROCKBOX_BINDINGS_TAG:-}"
REPO="${GH_REPO:-}"
ALL_TARGETS="darwin-arm64 darwin-x64 linux-x64 linux-arm64 freebsd-x64 netbsd-x64"

usage() { sed -n '2,37p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --all) ALL=1; shift ;;
    --target) TARGET="${2:?--target needs a value}"; shift 2 ;;
    --tag) TAG="${2:?--tag needs a value}"; shift 2 ;;
    --repo) REPO="${2:?--repo needs a value}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
done

command -v gh >/dev/null 2>&1 || {
  echo "error: the GitHub CLI (gh) is required — https://cli.github.com" >&2
  exit 1
}

# Resolve the repo from the origin remote if not given. gh picks its own default
# repo when several remotes exist (e.g. an upstream), which is often wrong here.
if [ -z "$REPO" ]; then
  url="$(git -C "$BINDINGS_DIR" remote get-url origin 2>/dev/null || true)"
  url="${url%.git}"
  case "$url" in
    git@github.com:*)       REPO="${url#git@github.com:}" ;;
    ssh://git@github.com/*) REPO="${url#ssh://git@github.com/}" ;;
    https://github.com/*)   REPO="${url#https://github.com/}" ;;
  esac
  [ -n "$REPO" ] || { echo "error: could not derive repo from origin; pass --repo owner/repo" >&2; exit 1; }
fi
echo "Repo: $REPO"

# Default tag: the most recent bindings-v* release in that repo.
if [ -z "$TAG" ]; then
  TAG="$(gh release list --repo "$REPO" --limit 50 --json tagName -q '.[].tagName' \
          | grep '^bindings-v' | head -1 || true)"
  [ -n "$TAG" ] || { echo "error: no bindings-v* release in $REPO; pass --tag" >&2; exit 1; }
fi
echo "Release: $TAG"

# Fail loudly if the release itself is missing (vs. a per-target asset absence).
if ! gh release view "$TAG" --repo "$REPO" >/dev/null 2>err.log; then
  echo "error: release $TAG not found in $REPO:" >&2
  sed 's/^/  /' err.log >&2; rm -f err.log
  exit 1
fi
rm -f err.log

host_target() {
  local os arch
  os="$(uname -s)"; arch="$(uname -m)"
  case "$os" in
    Darwin)  case "$arch" in arm64) echo darwin-arm64 ;; x86_64) echo darwin-x64 ;; esac ;;
    Linux)   case "$arch" in x86_64) echo linux-x64 ;; aarch64|arm64) echo linux-arm64 ;; esac ;;
    FreeBSD) echo freebsd-x64 ;;
    NetBSD)  echo netbsd-x64 ;;
  esac
}

libext() { case "$1" in darwin-*) echo dylib ;; *) echo so ;; esac; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# download <target> -> prints the local path; non-zero if the asset is absent.
# A "no assets match" error means the target is simply missing from the release
# (skip); any other gh error is surfaced.
download() {
  local t="$1" ext src err
  ext="$(libext "$t")"
  src="$TMP/librockbox_ffi-${t}.${ext}"
  [ -f "$src" ] && { echo "$src"; return 0; }
  err="$TMP/gh-err.log"
  if gh release download "$TAG" --repo "$REPO" \
       --pattern "librockbox_ffi-${t}.${ext}" --dir "$TMP" --clobber >/dev/null 2>"$err"; then
    echo "$src"; return 0
  fi
  grep -qi "no assets match" "$err" || { echo "gh: $(cat "$err")" >&2; }
  return 1
}

stage() {  # stage <target> <dest-file>
  local t="$1" dest="$2" src
  src="$(download "$t")" || { echo "  skip ${t}: not in $TAG"; return 1; }
  mkdir -p "$(dirname "$dest")"
  cp "$src" "$dest"
  echo "  -> ${dest#"$BINDINGS_DIR"/}"
}

stage_npm()     { local t="$1"; stage "$t" "$BINDINGS_DIR/typescript/npm/${t}/librockbox_ffi.$(libext "$t")"; }
stage_ruby()    { local t="$1"; stage "$t" "$BINDINGS_DIR/ruby/vendor/librockbox_ffi.$(libext "$t")"; }
stage_python()  { local t="$1"; stage "$t" "$BINDINGS_DIR/python/src/rockbox_ffi/_lib/librockbox_ffi.$(libext "$t")"; }
stage_kotlin()  { local t="$1"; stage "$t" "$BINDINGS_DIR/kotlin/src/main/resources/native/${t}/librockbox_ffi.$(libext "$t")"; }
stage_clojure() { local t="$1"; stage "$t" "$BINDINGS_DIR/clojure/resources/native/${t}/librockbox_ffi.$(libext "$t")"; }

if [ "$ALL" -eq 1 ]; then
  echo "Staging all targets into typescript/npm/* + kotlin + clojure (one jar bundles all):"
  for t in $ALL_TARGETS; do
    stage_npm "$t" || true
    stage_kotlin "$t" || true
    stage_clojure "$t" || true
  done
  host="$(host_target)"
  if [ -n "$host" ]; then
    echo "Staging host ($host) into ruby + python (single-slot — rerun with --target for others):"
    stage_ruby "$host" || true
    stage_python "$host" || true
  fi
else
  t="${TARGET:-$(host_target)}"
  [ -n "$t" ] || { echo "error: unsupported host $(uname -sm); pass --target" >&2; exit 1; }
  echo "Staging $t into ruby + python + typescript/npm/$t + kotlin + clojure:"
  echo "  (JVM jars bundle every target — use --all before publishing kotlin/clojure)"
  stage_ruby "$t"
  stage_python "$t"
  stage_npm "$t"
  stage_kotlin "$t"
  stage_clojure "$t"
fi

echo
echo "Done. Build & publish each package (from its directory):"
echo "  ruby:    ROCKBOX_GEM_PLATFORM=<plat> gem build rockbox_ffi.gemspec && gem push *.gem"
echo "  python:  python -m build --wheel && twine upload dist/*.whl"
echo "  npm:     (cd npm/<target> && npm pack && npm publish --access public)"
echo "  kotlin:  ./gradlew publishToMavenCentral        (needs --all for full platform coverage)"
echo "  clojure: CLOJARS_USERNAME=… CLOJARS_PASSWORD=… clojure -T:build deploy"
