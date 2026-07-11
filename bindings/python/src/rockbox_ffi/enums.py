"""Enum constants for the Rockbox FFI.

Note the two *different* ReplayGain encodings in the C ABI:

* The **DSP-level** call ``rb_dsp_set_replaygain`` (exposed via
  :meth:`rockbox_ffi.dsp.Dsp.set_replaygain`) uses the Rockbox-native values:
  ``TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3`` — see :class:`DspReplayGainMode`.
* The **player-level** call ``rb_player_set_replaygain`` uses a simpler
  encoding: ``OFF=0, TRACK=1, ALBUM=2`` — see :class:`ReplayGainMode`.
"""

from enum import IntEnum


class DspReplayGainMode(IntEnum):
    """Values for :meth:`rockbox_ffi.dsp.Dsp.set_replaygain` (native Rockbox)."""

    TRACK = 0
    ALBUM = 1
    SHUFFLE = 2
    OFF = 3


class ReplayGainMode(IntEnum):
    """Values for :meth:`rockbox_ffi.player.Player.set_replaygain`."""

    OFF = 0
    TRACK = 1
    ALBUM = 2


class CrossfadeMode(IntEnum):
    OFF = 0
    AUTO_SKIP = 1
    MANUAL_SKIP = 2
    SHUFFLE = 3
    SHUFFLE_OR_MANUAL = 4
    ALWAYS = 5


class MixMode(IntEnum):
    CROSSFADE = 0
    MIX = 1


class InsertPosition(IntEnum):
    """Values for :meth:`rockbox_ffi.player.Player.insert` and
    :meth:`~rockbox_ffi.player.Player.import_m3u`.

    ``INDEX`` uses the explicit ``index`` argument; all others ignore it.
    """

    PREPEND = 0
    INSERT = 1
    INSERT_NEXT = 2
    INSERT_LAST = 3
    INSERT_SHUFFLED = 4
    INSERT_LAST_SHUFFLED = 5
    REPLACE = 6
    INDEX = 7


class ChannelConfig(IntEnum):
    STEREO = 0
    MONO = 1
    CUSTOM = 2
    MONO_LEFT = 3
    MONO_RIGHT = 4
    KARAOKE = 5
    SWAP = 6
