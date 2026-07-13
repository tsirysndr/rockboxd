#!/usr/bin/env bash
#
# Publish the Elixir package (rockbox_ex_ffi) to Hex from a workstation.
#
# The PRECOMPILED NIFs are built and uploaded to the rolling `rockbox-ffi-nif`
# GitHub release by the `bindings-elixir-release` CI workflow. Hex publishing is
# deliberately NOT done in CI: Hex 2FA/OTP is interactive and can't be entered
# on a runner. This script does the two remaining, download-based steps locally:
#
#   1. `mix elixir_make.checksum --all` downloads every target's NIF tarball
#      from that release and writes checksum.exs (shipped in the package and
#      verified by consumers on install). This replaces a from-source build.
#   2. `mix hex.publish` packages the source + checksum.exs and uploads it;
#      its compile step downloads only the LOCAL target's NIF (no cargo needed).
#      You'll be prompted for your Hex OTP here — enter it interactively.
#
# The published version comes from mix.exs (@version). Run this only AFTER the
# CI workflow has finished uploading that version's artifacts to the release.
#
# Authenticate first:  mix hex.user auth   (or export HEX_API_KEY=...)
#
# Usage:
#   bindings/scripts/publish-elixir.sh [--dry-run]

set -euo pipefail
COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_release_common.sh
. "$COMMON_DIR/_release_common.sh"
ROOT="$(cd "$COMMON_DIR/../.." && pwd)"

parse_common_args "$@"
[ ${#REST[@]} -eq 0 ] || { echo "unknown argument: ${REST[0]}" >&2; exit 2; }

command -v mix >/dev/null 2>&1 || { echo "error: mix (Elixir) not found" >&2; exit 1; }

echo "== Elixir -> Hex =="
cd "$ROOT/bindings/elixir"
run mix deps.get

# Download all targets' precompiled NIFs from the rolling GitHub release and
# generate checksum.exs. --ignore-unavailable so a missing best-effort BSD
# target doesn't block the release.
run mix elixir_make.checksum --all --ignore-unavailable

# hex.publish uploads both the package and the ExDoc docs to HexDocs. The
# explicit `hex.publish docs` afterwards is idempotent and refreshes HexDocs
# even when the package version already exists (re-run docs without a bump).
run mix hex.publish --yes
run mix hex.publish docs --yes
echo "Done."
