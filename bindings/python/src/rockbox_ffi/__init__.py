"""Python bindings for the Rockbox DSP / metadata / playback engine.

Thin ``cffi`` wrappers over the ``librockbox_ffi`` C ABI. See the submodules
:mod:`rockbox_ffi.metadata`, :mod:`rockbox_ffi.dsp`, :mod:`rockbox_ffi.player`.
"""

from . import metadata
from ._ffi import lib
from .dsp import Dsp, sine_stereo
from .enums import (
    ChannelConfig,
    CrossfadeMode,
    DspReplayGainMode,
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
    "MixMode",
    "ReplayGainMode",
    "abi_version",
]


def abi_version() -> int:
    """ABI major version of the loaded library (bumped on breaking changes)."""
    return int(lib.rb_ffi_abi_version())
