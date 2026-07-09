"""Queue-based player with native ReplayGain and Rockbox crossfade.

A :class:`Player` owns a live audio output device and a background engine
thread — construct it only where an output device exists.
"""

from __future__ import annotations

import json
from typing import List, Optional

from ._ffi import ffi, lib, take_string
from .enums import CrossfadeMode, MixMode, ReplayGainMode


class Player:
    def __init__(
        self,
        *,
        sample_rate: int = 0,
        buffer_seconds: float = 4.0,
        volume: float = 1.0,
        replaygain_mode: int = ReplayGainMode.OFF,
        replaygain_preamp_db: float = 0.0,
        replaygain_prevent_clipping: bool = True,
        crossfade_mode: int = CrossfadeMode.OFF,
        fade_out_delay_ms: int = 0,
        fade_out_duration_ms: int = 2000,
        fade_in_delay_ms: int = 0,
        fade_in_duration_ms: int = 2000,
        mix_mode: int = MixMode.CROSSFADE,
        _default: bool = False,
    ):
        """Create a player. With no args, uses the device default sample rate
        and Rockbox default settings. ``sample_rate=0`` means device default."""
        if _default:
            self._p = lib.rb_player_new()
        else:
            self._p = lib.rb_player_new_with_config(
                int(sample_rate), float(buffer_seconds), float(volume),
                int(replaygain_mode), float(replaygain_preamp_db),
                bool(replaygain_prevent_clipping), int(crossfade_mode),
                int(fade_out_delay_ms), int(fade_out_duration_ms),
                int(fade_in_delay_ms), int(fade_in_duration_ms), int(mix_mode),
            )
        if self._p == ffi.NULL:
            raise RuntimeError("failed to create Player (no output device?)")

    @classmethod
    def default(cls) -> "Player":
        """Player on the default device with default settings."""
        return cls(_default=True)

    # -- lifecycle --------------------------------------------------------
    def close(self) -> None:
        if getattr(self, "_p", ffi.NULL) != ffi.NULL:
            lib.rb_player_free(self._p)
            self._p = ffi.NULL

    def __del__(self):
        self.close()

    def __enter__(self) -> "Player":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    # -- queue ------------------------------------------------------------
    def set_queue(self, paths: List[str]) -> None:
        lib.rb_player_set_queue_json(self._p, json.dumps(list(paths)).encode("utf-8"))

    def enqueue(self, path: str) -> None:
        lib.rb_player_enqueue(self._p, path.encode("utf-8"))

    # -- transport --------------------------------------------------------
    def play(self) -> None:
        lib.rb_player_play(self._p)

    def pause(self) -> None:
        lib.rb_player_pause(self._p)

    def toggle(self) -> None:
        lib.rb_player_toggle(self._p)

    def stop(self) -> None:
        lib.rb_player_stop(self._p)

    def next(self) -> None:
        lib.rb_player_next(self._p)

    def previous(self) -> None:
        lib.rb_player_previous(self._p)

    def skip_to(self, index: int) -> None:
        lib.rb_player_skip_to(self._p, int(index))

    def seek_ms(self, ms: int) -> None:
        lib.rb_player_seek_ms(self._p, int(ms))

    # -- settings ---------------------------------------------------------
    def set_volume(self, vol: float) -> None:
        lib.rb_player_set_volume(self._p, float(vol))

    def volume(self) -> float:
        return float(lib.rb_player_volume(self._p))

    def sample_rate(self) -> int:
        return int(lib.rb_player_sample_rate(self._p))

    def set_crossfade(
        self,
        mode: int,
        fade_out_delay_ms: int = 0,
        fade_out_duration_ms: int = 2000,
        fade_in_delay_ms: int = 0,
        fade_in_duration_ms: int = 2000,
        mix_mode: int = MixMode.CROSSFADE,
    ) -> None:
        lib.rb_player_set_crossfade(
            self._p, int(mode), int(fade_out_delay_ms), int(fade_out_duration_ms),
            int(fade_in_delay_ms), int(fade_in_duration_ms), int(mix_mode),
        )

    def set_replaygain(self, mode: int, preamp_db: float, prevent_clipping: bool) -> None:
        """mode: :class:`rockbox_ffi.enums.ReplayGainMode` (OFF=0, TRACK=1, ALBUM=2)."""
        lib.rb_player_set_replaygain(self._p, int(mode), float(preamp_db), bool(prevent_clipping))

    # -- status -----------------------------------------------------------
    def status(self) -> dict:
        s = take_string(lib.rb_player_status_json(self._p))
        if s is None:
            raise RuntimeError("rb_player_status_json returned NULL")
        return json.loads(s)
