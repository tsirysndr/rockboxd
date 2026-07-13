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
#   go         -> bindings/go/lib/librockbox_ffi.<ext>
#   typescript -> bindings/typescript/npm/<t>/librockbox_ffi.<ext>
#   kotlin     -> bindings/kotlin/src/main/resources/native/<t>/librockbox_ffi.<ext>
#   clojure    -> bindings/clojure/resources/native/<t>/librockbox_ffi.<ext>
#
# The JVM bindings (kotlin, clojure) bundle EVERY target in a single jar, so
# --all stages all targets for them; host mode stages just the host target.
#
# Elixir NIF — does NOT use librockbox_ffi directly; it ships a self-contained
# erl_nif shared object (rockbox_ffi_nif-<triple>.so, with librockbox_ffi
# statically linked in) that the loader picks by host triple. It lives in its OWN
# release, not the bindings-v* one:
#   elixir -> bindings/elixir/priv/rockbox_ffi_nif-<triple>.so  (from the rolling
#             `rockbox-ffi-nif` release; extracted from the per-target tarball)
# <triple> is the Rust target triple (e.g. aarch64-apple-darwin). --all stages
# every target; host mode stages just the host triple. Best-effort — a missing
# elixir release only warns, it does not abort the librockbox_ffi staging.
# Override the tag with ROCKBOX_ELIXIR_TAG if needed.
#
# (The Gleam package is NOT handled here: it downloads its NIF at runtime on
# first load — see bindings/scripts/publish-gleam.sh — so nothing to stage.)
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

# Print the leading comment block (everything from line 2 up to the first
# non-comment line) so the usage text stays in sync as the header grows.
usage() { awk 'NR>=2 && /^#/ {sub(/^# ?/,""); print; next} NR>=2 {exit}' "${BASH_SOURCE[0]}"; }

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

# --- Elixir NIF release — resolved independently of the main bindings-v* release
# above. Best-effort: unresolved/missing only warns later. (Gleam downloads its
# NIF at runtime, so it isn't staged here.) ---
ELIXIR_TAG="${ROCKBOX_ELIXIR_TAG:-rockbox-ffi-nif}"   # rolling release, one tag for all versions
ELIXIR_NIF_VERSION="2.17"                             # matches make_precompiler_nif_versions in mix.exs
ELIXIR_VERSION="${ROCKBOX_ELIXIR_VERSION:-}"
if [ -z "$ELIXIR_VERSION" ]; then
  ELIXIR_VERSION="$(sed -n 's/^[[:space:]]*@version[[:space:]]*"\(.*\)"/\1/p' "$BINDINGS_DIR/elixir/mix.exs" 2>/dev/null | head -1)"
fi
echo "Elixir NIF release: ${ELIXIR_TAG} (v${ELIXIR_VERSION:-<unresolved — elixir skipped>})"

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
stage_go()      { local t="$1"; stage "$t" "$BINDINGS_DIR/go/lib/librockbox_ffi.$(libext "$t")"; }
stage_kotlin()  { local t="$1"; stage "$t" "$BINDINGS_DIR/kotlin/src/main/resources/native/${t}/librockbox_ffi.$(libext "$t")"; }
stage_clojure() { local t="$1"; stage "$t" "$BINDINGS_DIR/clojure/resources/native/${t}/librockbox_ffi.$(libext "$t")"; }

# Map a fetch-libs target to the Rust target triple used in the NIF filenames
# (rockbox_ffi_nif-<triple>.so). Prints nothing for an unmapped target.
nif_triple() {
  case "$1" in
    darwin-arm64) echo aarch64-apple-darwin ;;
    darwin-x64)   echo x86_64-apple-darwin ;;
    linux-x64)    echo x86_64-linux-gnu ;;
    linux-arm64)  echo aarch64-linux-gnu ;;
    freebsd-x64)  echo x86_64-unknown-freebsd ;;
    netbsd-x64)   echo x86_64-unknown-netbsd ;;
  esac
}

# fetch_asset <tag> <pattern> <dest-dir> — download one release asset into
# <dest-dir> keeping its original name. Non-zero if the asset is simply absent
# (skip); any other gh error is surfaced.
fetch_asset() {
  local tag="$1" pattern="$2" dir="$3" err="$TMP/gh-err.log"
  if gh release download "$tag" --repo "$REPO" \
       --pattern "$pattern" --dir "$dir" --clobber >/dev/null 2>"$err"; then
    return 0
  fi
  grep -qi "no assets match\|release not found\|not found" "$err" || echo "gh: $(cat "$err")" >&2
  return 1
}

# The elixir NIF is published as a per-target tarball (containing ./rockbox_ffi_nif.so)
# on the rolling `rockbox-ffi-nif` release. Extract it and stage under the same
# triple-suffixed name the loader (src/rockbox_ffi_nif.erl) probes for.
stage_elixir() {  # stage_elixir <target>
  local t="$1" triple asset ex dest
  [ -n "$ELIXIR_VERSION" ] || { echo "  skip elixir ${t}: no elixir version resolved"; return 1; }
  triple="$(nif_triple "$t")"; [ -n "$triple" ] || { echo "  skip elixir ${t}: no triple mapping"; return 1; }
  asset="rockbox_ex_ffi-nif-${ELIXIR_NIF_VERSION}-${triple}-${ELIXIR_VERSION}.tar.gz"
  ex="$TMP/elixir-${triple}"
  dest="$BINDINGS_DIR/elixir/priv/rockbox_ffi_nif-${triple}.so"
  if [ ! -f "$ex/rockbox_ffi_nif.so" ]; then
    fetch_asset "$ELIXIR_TAG" "$asset" "$TMP" \
      || { echo "  skip elixir ${t}: $asset not in $ELIXIR_TAG"; return 1; }
    mkdir -p "$ex"
    tar xzf "$TMP/$asset" -C "$ex"
  fi
  [ -f "$ex/rockbox_ffi_nif.so" ] || { echo "  skip elixir ${t}: tarball had no rockbox_ffi_nif.so" >&2; return 1; }
  mkdir -p "$(dirname "$dest")"
  cp "$ex/rockbox_ffi_nif.so" "$dest"
  echo "  -> ${dest#"$BINDINGS_DIR"/}"
}

if [ "$ALL" -eq 1 ]; then
  echo "Staging all targets into typescript/npm/* + kotlin + clojure + elixir/priv:"
  for t in $ALL_TARGETS; do
    stage_npm "$t" || true
    stage_kotlin "$t" || true
    stage_clojure "$t" || true
    stage_elixir "$t" || true
  done
  host="$(host_target)"
  if [ -n "$host" ]; then
    echo "Staging host ($host) into ruby + python + go (single-slot — rerun with --target for others):"
    stage_ruby "$host" || true
    stage_python "$host" || true
    stage_go "$host" || true
  fi
else
  t="${TARGET:-$(host_target)}"
  [ -n "$t" ] || { echo "error: unsupported host $(uname -sm); pass --target" >&2; exit 1; }
  echo "Staging $t into ruby + python + go + typescript/npm/$t + kotlin + clojure + elixir/priv:"
  echo "  (JVM jars bundle every target — use --all before publishing kotlin/clojure)"
  stage_ruby "$t"
  stage_python "$t"
  stage_go "$t"
  stage_npm "$t"
  stage_kotlin "$t"
  stage_clojure "$t"
  stage_elixir "$t" || true
fi

echo
echo "Done. Build & publish each package (from its directory):"
echo "  ruby:    ROCKBOX_GEM_PLATFORM=<plat> gem build rockbox_ffi.gemspec && gem push *.gem"
echo "  python:  python -m build --wheel && twine upload dist/*.whl"
echo "  npm:     (cd npm/<target> && npm pack && npm publish --access public)"
echo "  kotlin:  ./gradlew publishToMavenCentral        (needs --all for full platform coverage)"
echo "  clojure: CLOJARS_USERNAME=… CLOJARS_PASSWORD=… clojure -T:build deploy"
echo "  elixir:  bindings/scripts/publish-elixir.sh      (generates checksum.exs from the same release)"
echo "  gleam:   bindings/scripts/publish-gleam.sh       (writes a checksum manifest; NIFs download on first use)"
