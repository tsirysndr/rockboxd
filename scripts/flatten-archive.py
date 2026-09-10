#!/usr/bin/env python3
"""Extract every .o member of a static archive (recursing into nested
archives), writing each to a uniquely-named file under an output directory.

Why not just `ar`? `zig build lib` adds inputs to librockboxd.a via paths
like /home/runner/.../librockbox.a — llvm-ar stores them with that absolute
path as the member name. GNU `ar xN <occ> archive <name>` then matches by
exact stored name; passing the full path doesn't work, passing the basename
doesn't work, the script silently extracts zero members, and the final
librockboxd.a comes out empty. Parsing the ar format ourselves sidesteps
the whole class of issue.

Drops `.so` members on the way through — those are dynamic library refs
that the consumer's link step already handles via `-l` flags.
"""
from __future__ import annotations

import struct
import sys
from pathlib import Path
from typing import Iterator, Tuple

ARCHIVE_MAGIC = b"!<arch>\n"


def parse_archive(path: Path) -> Iterator[Tuple[int, str, bytes]]:
    """Yield (index, resolved_name, content) for each non-special member.

    Skips the GNU `//` string table and the symbol index (`/`, `__.SYMDEF`).
    """
    with path.open("rb") as f:
        magic = f.read(8)
        if magic != ARCHIVE_MAGIC:
            raise SystemExit(f"flatten-archive: not an ar archive: {path}")

        long_names: bytes | None = None
        index = 0
        while True:
            header = f.read(60)
            if not header:
                break
            if len(header) < 60:
                raise SystemExit(f"flatten-archive: truncated header in {path}")

            raw_name = header[0:16].decode("ascii", errors="replace").rstrip()
            try:
                size = int(header[48:58].decode("ascii").strip())
            except ValueError:
                raise SystemExit(f"flatten-archive: bad size in {path} member {index}")

            content = f.read(size)
            if size % 2 == 1:
                f.read(1)  # 1-byte alignment

            # Special: GNU extended-name table — capture and skip.
            if raw_name == "//":
                long_names = content
                continue

            # Resolve to canonical name first; the skip checks below need the
            # *resolved* name because BSD encodes __.SYMDEF as #1/<len> with
            # the real name living in the content payload.
            if raw_name.startswith("#1/"):
                namelen = int(raw_name[3:])
                name = content[:namelen].decode("ascii", errors="replace").rstrip("\0")
                content = content[namelen:]
            elif raw_name.startswith("/") and raw_name[1:].lstrip("0").isdigit():
                if long_names is None:
                    raise SystemExit(
                        f"flatten-archive: extended name reference without // table in {path}"
                    )
                offset = int(raw_name[1:])
                end = long_names.find(b"/\n", offset)
                if end < 0:
                    end = long_names.find(b"\n", offset)
                name = long_names[offset:end].decode("ascii", errors="replace")
            elif raw_name.endswith("/"):
                name = raw_name[:-1]
            else:
                name = raw_name

            # Skip both the GNU symbol index ("/" → empty after strip) and the
            # BSD-style __.SYMDEF / __.SYMDEF SORTED entries.
            if name == "" or name.startswith("__.SYMDEF"):
                continue

            yield index, name, content
            index += 1


def is_elf_dynamic(content: bytes) -> bool:
    """Return True if `content` is an ELF shared object (ET_DYN). Those come
    from `linkSystemLibrary(...)` calls and the consumer should resolve them
    via `-l` at link time, not pull them in as archive members."""
    if len(content) < 20 or content[:4] != b"\x7fELF":
        return False
    # ELF header layout: byte 5 is data encoding (1=LE, 2=BE), e_type is u16
    # at offset 16. Distinguish ET_REL (1) from ET_DYN (3).
    endian = "<" if content[5] == 1 else ">"
    (e_type,) = struct.unpack_from(f"{endian}H", content, 16)
    return e_type == 3  # ET_DYN


def is_macho_dylib(content: bytes) -> bool:
    """Return True if `content` is a Mach-O dynamic library (MH_DYLIB). Same
    reasoning as `is_elf_dynamic`: the consumer links those via `-l`/framework
    flags, and Apple's ld refuses an archive that carries one as a member."""
    if len(content) < 16:
        return False
    magic = content[:4]
    if magic == b"\xcf\xfa\xed\xfe" or magic == b"\xce\xfa\xed\xfe":  # LE (64/32)
        endian = "<"
    elif magic == b"\xfe\xed\xfa\xcf" or magic == b"\xfe\xed\xfa\xce":  # BE
        endian = ">"
    else:
        return False
    (filetype,) = struct.unpack_from(f"{endian}I", content, 12)
    return filetype == 0x6  # MH_DYLIB


def is_archive(content: bytes) -> bool:
    return content.startswith(ARCHIVE_MAGIC)


def safe_basename(name: str) -> str:
    base = Path(name).name
    return base or f"unnamed_{abs(hash(name)):x}.o"


def extract(archive_path: Path, outdir: Path, prefix: str) -> None:
    outdir.mkdir(parents=True, exist_ok=True)
    for index, name, content in parse_archive(archive_path):
        if is_elf_dynamic(content) or is_macho_dylib(content):
            continue
        if is_archive(content):
            nested = outdir / f"{prefix}_n{index}.a"
            nested.write_bytes(content)
            sub_prefix = f"{prefix}_{index}_{Path(name).stem or 'nested'}"
            extract(nested, outdir / f"sub_{index}", sub_prefix)
            nested.unlink()
            continue
        # Real .o (or unknown — treat as object and let ar/ld complain if not).
        target = outdir / f"{prefix}__{index}__{safe_basename(name)}"
        if not target.name.endswith(".o"):
            target = target.with_suffix(target.suffix + ".o")
        target.write_bytes(content)


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {sys.argv[0]} <archive.a> <outdir>", file=sys.stderr)
        return 64

    archive = Path(sys.argv[1]).resolve()
    outdir = Path(sys.argv[2]).resolve()
    if not archive.is_file():
        print(f"flatten-archive: not found: {archive}", file=sys.stderr)
        return 1

    extract(archive, outdir, "root")
    return 0


if __name__ == "__main__":
    sys.exit(main())
