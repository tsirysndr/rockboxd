#!/usr/bin/env bash
# Flatten a Zig-produced static archive into one containing only .o members.
#
# `zig build lib` packs each input static archive into librockboxd.a. Depending
# on the host toolchain the inputs can survive as nested `.a` (and even `.so`)
# members instead of the underlying .o objects, and the archive index that the
# consumer's linker relies on is whatever the packer happened to emit.
#
# Rebuilding the archive ourselves — extract every object, re-archive, rebuild
# the index — makes the output identical on every host:
#
#   * GNU/Linux linkers (including rust-lld) don't unpack nested archive
#     members, so consumers saw "is neither ET_REL nor LLVM bitcode" warnings
#     and undefined symbols from the nested archives.
#   * macOS previously repacked in place with `libtool -static -o X X`. Feeding
#     libtool its own output is fragile — on the macOS 26 runner image that
#     silently produced an archive whose Rust half (librockbox_embed.a /
#     librockbox_server.a) was unreachable, so every consumer failed with
#     "Undefined symbols: _rb_daemon_start" while the firmware members linked
#     fine. Extract-then-repack from a temp dir removes the aliasing entirely.
#
# Delegates the actual parsing to flatten-archive.py — `ar xN <occ>` proved
# hopeless against archives whose member names are absolute paths (which is
# what llvm-ar stores when zig adds them via b.path(...)), so we parse the
# ar format directly. Handles both the GNU (`//` string table) and the BSD /
# Darwin (`#1/<len>`) member-name encodings.
#
# Usage:
#   flatten-archive.sh <archive.a> [required-symbol]
#
# `required-symbol` is a substring matched against the defined symbols of the
# rebuilt archive; the script exits non-zero if it is absent. Pass it to turn a
# silently-truncated archive into a build failure at the point it happens
# instead of an undefined symbol in a consumer's link a few minutes later.
set -euo pipefail

AR="${AR:-ar}"
NM="${NM:-nm}"

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <archive.a> [required-symbol]" >&2
  exit 64
fi

archive="$1"
require_symbol="${2:-}"

if [ ! -f "$archive" ]; then
  echo "flatten-archive: not found: $archive" >&2
  exit 1
fi
# `realpath` is not on every host (older macOS); cd+pwd is.
archive="$(cd "$(dirname "$archive")" && pwd)/$(basename "$archive")"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

python3 "$script_dir/flatten-archive.py" "$archive" "$work/objs"

find "$work/objs" -name '*.o' -print > "$work/objs.txt"
nobjs="$(wc -l < "$work/objs.txt" | tr -d ' ')"
if [ "$nobjs" -eq 0 ]; then
  echo "flatten-archive: no objects extracted from $archive — aborting" >&2
  exit 1
fi

out="$work/flat.a"
if [ "$(uname)" = "Darwin" ]; then
  # libtool builds the Darwin-format archive + sorted __.SYMDEF in one pass.
  # -filelist keeps us clear of ARG_MAX with ~5k objects.
  libtool -static -no_warning_for_no_symbols -o "$out" -filelist "$work/objs.txt"
else
  # `ar -rcs` in xargs batches: each batch appends, the index is rebuilt on
  # every call, so the final archive carries a complete index.
  tr '\n' '\0' < "$work/objs.txt" | xargs -0 "$AR" -rcs "$out"
fi

mv "$out" "$archive"

members="$("$AR" t "$archive" | wc -l | tr -d ' ')"
echo "flatten-archive: $nobjs object(s) extracted, $members member(s) -> $archive"

if [ -n "$require_symbol" ]; then
  # `set +o pipefail` in a subshell: grep -q exits at the first match and
  # SIGPIPEs nm, which under pipefail would mark the whole pipeline failed and
  # report a present symbol as missing.
  if (set +o pipefail; "$NM" --defined-only "$archive" 2>/dev/null | grep -q -- "$require_symbol"); then
    echo "flatten-archive: verified '$require_symbol' is defined"
  else
    echo "flatten-archive: '$require_symbol' NOT defined in $archive — aborting" >&2
    exit 1
  fi
fi
