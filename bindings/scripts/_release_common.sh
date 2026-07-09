# Shared helpers for the publish-*.sh scripts. Sourced, not executed.
#
# Resolves the GitHub repo + release tag and provides download / dry-run
# helpers. Parses --tag / --repo / --dry-run from the caller's args and leaves
# any unrecognised args in REST[] for the caller to handle.

# shellcheck shell=bash

TAG="${ROCKBOX_BINDINGS_TAG:-}"
REPO="${GH_REPO:-}"
DRY=0
REST=()

parse_common_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --tag) TAG="${2:?--tag needs a value}"; shift 2 ;;
      --repo) REPO="${2:?--repo needs a value}"; shift 2 ;;
      --dry-run) DRY=1; shift ;;
      *) REST+=("$1"); shift ;;
    esac
  done
}

# run <cmd...> — execute, or just print it under --dry-run.
run() {
  if [ "$DRY" -eq 1 ]; then printf '  [dry-run] '; printf '%q ' "$@"; printf '\n'
  else "$@"; fi
}

resolve_repo_and_tag() {
  command -v gh >/dev/null 2>&1 || { echo "error: gh (GitHub CLI) is required — https://cli.github.com" >&2; exit 1; }

  # Resolve repo from origin — gh may pick a different default when several
  # remotes exist (e.g. an upstream).
  if [ -z "$REPO" ]; then
    local url; url="$(git -C "$COMMON_DIR" remote get-url origin 2>/dev/null || true)"; url="${url%.git}"
    case "$url" in
      git@github.com:*)       REPO="${url#git@github.com:}" ;;
      ssh://git@github.com/*) REPO="${url#ssh://git@github.com/}" ;;
      https://github.com/*)   REPO="${url#https://github.com/}" ;;
    esac
    [ -n "$REPO" ] || { echo "error: could not derive repo from origin; pass --repo owner/repo" >&2; exit 1; }
  fi

  # Resolve tag — latest bindings-v* in that repo.
  if [ -z "$TAG" ]; then
    TAG="$(gh release list --repo "$REPO" --limit 50 --json tagName -q '.[].tagName' \
            | grep '^bindings-v' | head -1 || true)"
    [ -n "$TAG" ] || { echo "error: no bindings-v* release in $REPO; pass --tag" >&2; exit 1; }
  fi

  echo "Repo:    $REPO"
  echo "Release: $TAG"
  [ "$DRY" -eq 1 ] && echo "Mode:    DRY RUN (nothing will be pushed)"
  echo
}

# download <dir> <pattern...> — pull matching release assets into <dir>.
download_assets() {
  local dir="$1"; shift
  local args=()
  local p; for p in "$@"; do args+=(--pattern "$p"); done
  gh release download "$TAG" --repo "$REPO" --dir "$dir" --clobber "${args[@]}"
}
