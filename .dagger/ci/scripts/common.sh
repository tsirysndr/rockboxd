#!/usr/bin/env bash
# Shared by the ported steps. No GitHub service or credentials are required.
export GITHUB_WORKSPACE=/src GITHUB_ENV=/tmp/step-env GITHUB_OUTPUT=/tmp/step-output
export CI=true RUNNER_OS=Linux
: > "$GITHUB_ENV"
: > "$GITHUB_OUTPUT"
mkdir -p /out
cd /src
# GitHub's single-line environment protocol, without evaluating shell code.
load_step_env() {
  local entry
  while IFS= read -r entry; do
    [[ "$entry" =~ ^[A-Za-z_][A-Za-z0-9_]*= ]] || { echo "Invalid step environment entry" >&2; return 1; }
    export "$entry"
  done < "$GITHUB_ENV"
  : > "$GITHUB_ENV"
}
collect() {
  cd /src
  local pattern path found
  for pattern in "$@"; do
    found=false
    while IFS= read -r path; do
      found=true
      cp -a --parents "$path" /out/
    done < <(compgen -G "$pattern" || true)
    "$found" || { echo "Missing output: $pattern" >&2; return 1; }
  done
  mkdir -p /out/.dagger-results
  printf '%s passed\n' "$JOB" > "/out/.dagger-results/$JOB.txt"
}
[[ "$TAG" =~ ^[A-Za-z0-9][A-Za-z0-9.+_~-]*$ ]] || { echo 'Invalid tag' >&2; exit 2; }
