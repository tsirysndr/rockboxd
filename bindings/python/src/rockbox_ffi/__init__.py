"""Python bindings for the Rockbox DSP / metadata / playback engine.

Thin ``cffi`` wrappers over the ``librockbox_ffi`` C ABI. See the submodules
:mod:`rockbox_ffi.metadata`, :mod:`rockbox_ffi.dsp`, :mod:`rockbox_ffi.player`.
"""

import json
from typing import List, Optional

from . import metadata
from ._ffi import lib, take_string
from .dsp import Dsp, sine_stereo
from .enums import (
    ChannelConfig,
    CrossfadeMode,
    DspReplayGainMode,
    InsertPosition,
    MixMode,
    ReplayGainMode,
)
from .player import Player

__all__ = [
    "metadata",
    "Dsp",
    "Player",
    "sine_stereo",
    "ChannelConfig",
    "CrossfadeMode",
    "DspReplayGainMode",
    "InsertPosition",
    "MixMode",
    "ReplayGainMode",
    "abi_version",
    "load_resume",
    "m3u_read",
    "m3u_write",
    "is_url",
]


def abi_version() -> int:
    """ABI major version of the loaded library (bumped on breaking changes)."""
    return int(lib.rb_ffi_abi_version())


def load_resume(path: str) -> Optional[dict]:
    """Peek at a resume file without a player.

    Returns a dict ``{tracks, index, elapsed_ms}`` or ``None`` if absent/invalid.
    """
    s = take_string(lib.rb_load_resume_json(path.encode("utf-8")))
    if s is None:
        return None
    return json.loads(s)


def m3u_read(path: str) -> Optional[list]:
    """Parse a playlist file into a list of ``{path, duration_ms, title}`` dicts.

    Returns ``None`` on error.
    """
    s = take_string(lib.rb_m3u_read_json(path.encode("utf-8")))
    if s is None:
        return None
    return json.loads(s)


def m3u_write(path: str, paths: List[str]) -> bool:
    """Write a list of paths as an ``.m3u8``; ``True`` on success."""
    payload = json.dumps(list(paths)).encode("utf-8")
    return int(lib.rb_m3u_write_json(path.encode("utf-8"), payload)) == 0


def is_url(s: str) -> bool:
    """Whether ``s`` looks like an ``http(s)://`` URL."""
    return bool(lib.rb_is_url(s.encode("utf-8")))
