"""Audio file metadata parsing."""

from __future__ import annotations

import json
from typing import Optional

from ._ffi import ffi, lib, take_string


def read(path: str) -> dict:
    """Parse the metadata of the audio file at ``path``.

    Returns a dict with tag strings, ``duration_ms``, ``bitrate``,
    ``sample_rate``, a nested ``replaygain`` dict (including the raw Q7.24
    ``raw_*`` fields), and optional ``album_art`` / ``cuesheet`` locations.

    Raises ``FileNotFoundError`` / ``ValueError`` on failure.
    """
    s = take_string(lib.rb_meta_read_json(path.encode("utf-8")))
    if s is None:
        raise ValueError(f"could not read metadata from {path!r}")
    return json.loads(s)


def probe(filename: str) -> Optional[str]:
    """Guess the codec label (e.g. ``"FLAC"``) from a filename's extension
    without opening the file. Returns ``None`` for an unknown extension."""
    return take_string(lib.rb_meta_probe(filename.encode("utf-8")))
