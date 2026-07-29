"""Queue-based player with native ReplayGain and Rockbox crossfade.

A :class:`Player` owns a live audio output device and a background engine
thread — construct it only where an output device exists.
"""

from __future__ import annotations

import json
from typing import List, Optional

from ._ffi import ffi, lib, take_string
from .enums import CrossfadeMode, InsertPosition, MixMode, RepeatMode, ReplayGainMode


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
        resume_file: str | None = None,
        resume_save_interval_ms: int = 0,
        output: str | None = None,
        _default: bool = False,
    ):
        """Create a player. With no args, uses the device default sample rate
        and Rockbox default settings. ``sample_rate=0`` means device default.

        When ``resume_file`` is set (an ``.m3u8`` path), the queue and exact
        position are auto-persisted to it; ``resume_save_interval_ms=0`` uses
        the 5 s default.

        ``output`` selects the audio output backend (``None``/``""`` => the
        system device via cpal, the default). Accepted specs: ``"cpal"``,
        ``"stdout"`` (or ``"-"``), ``"fifo:/path"``, ``"unix:/path"`` /
        ``"unix-connect:/path"``, ``"tcp:host:port"`` / ``"tcp-connect:host:port"``.
        Non-cpal backends emit raw S16LE stereo PCM; in ``stdout`` mode fd 1 IS
        that stream, so the host program must keep stdout clean (pipe it to a
        raw-PCM player, e.g. ``ffplay -f s16le -ar 44100 -ac 2 -``). A listening
        socket blocks until a client connects.
        """
        if _default:
            self._p = lib.rb_player_new()
        elif output:
            self._p = lib.rb_player_new_with_output(
                output.encode("utf-8"),
                int(sample_rate), float(buffer_seconds), float(volume),
                int(replaygain_mode), float(replaygain_preamp_db),
                bool(replaygain_prevent_clipping), int(crossfade_mode),
                int(fade_out_delay_ms), int(fade_out_duration_ms),
                int(fade_in_delay_ms), int(fade_in_duration_ms), int(mix_mode),
                resume_file.encode("utf-8") if resume_file else ffi.NULL,
                int(resume_save_interval_ms),
            )
        elif resume_file is not None:
            self._p = lib.rb_player_new_with_config_ex(
                int(sample_rate), float(buffer_seconds), float(volume),
                int(replaygain_mode), float(replaygain_preamp_db),
                bool(replaygain_prevent_clipping), int(crossfade_mode),
                int(fade_out_delay_ms), int(fade_out_duration_ms),
                int(fade_in_delay_ms), int(fade_in_duration_ms), int(mix_mode),
                resume_file.encode("utf-8"), int(resume_save_interval_ms),
            )
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
        """Replace the queue. Each entry may be a local file path, an
        ``http(s)://`` URL to a finite remote file, or a live-radio /
        streaming URL."""
        lib.rb_player_set_queue_json(self._p, json.dumps(list(paths)).encode("utf-8"))

    def enqueue(self, path: str) -> None:
        """Append one track to the queue. ``path`` may be a local file path,
        an ``http(s)://`` URL to a finite remote file, or a live-radio /
        streaming URL."""
        lib.rb_player_enqueue(self._p, path.encode("utf-8"))

    def insert(self, paths: List[str], position: int, index: int = 0) -> None:
        """Insert paths/URLs into the queue at ``position``.

        ``position``: :class:`rockbox_ffi.enums.InsertPosition`. ``index`` is
        only used when ``position`` is ``InsertPosition.INDEX`` (7).
        """
        lib.rb_player_insert_json(
            self._p, json.dumps(list(paths)).encode("utf-8"),
            int(position), int(index),
        )

    def queue(self) -> List[str]:
        """Current queue as a list of paths/URLs."""
        s = take_string(lib.rb_player_queue_json(self._p))
        if s is None:
            return []
        return json.loads(s)

    def remove(self, index: int) -> None:
        """Remove the track at ``index`` (0-based) from the queue.

        An out-of-range ``index`` is ignored. Removing a track before the
        current one keeps the current track playing; removing the
        currently-playing track hard-cuts to the track that slides into its
        place; removing the last remaining track stops playback.
        """
        lib.rb_player_remove(self._p, int(index))

    def clear_queue(self) -> None:
        """Empty the queue and stop playback (also clears saved resume state)."""
        lib.rb_player_clear_queue(self._p)

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

    def set_balance(self, balance: int) -> None:
        """Stereo balance, -100 (full left) ..= +100 (full right); 0 = centre."""
        lib.rb_player_set_balance(self._p, int(balance))

    def volume(self) -> float:
        return float(lib.rb_player_volume(self._p))

    def balance(self) -> int:
        """Stereo balance, -100 (full left) ..= +100 (full right)."""
        return int(lib.rb_player_balance(self._p))

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

    # -- shuffle / repeat -------------------------------------------------
    def set_shuffle(self, enabled: bool) -> None:
        lib.rb_player_set_shuffle(self._p, bool(enabled))

    def is_shuffle_enabled(self) -> bool:
        return bool(lib.rb_player_is_shuffle_enabled(self._p))

    def set_repeat(self, mode: int) -> None:
        """mode: :class:`rockbox_ffi.enums.RepeatMode` (OFF=0, ONE=1, ALL=2)."""
        lib.rb_player_set_repeat(self._p, int(mode))

    def repeat(self) -> int:
        """Current repeat mode as an int (0 off, 1 one, 2 all)."""
        return int(lib.rb_player_repeat(self._p))

    # -- dsp / eq ---------------------------------------------------------
    def set_eq_enabled(self, enabled: bool) -> None:
        lib.rb_player_set_eq_enabled(self._p, bool(enabled))

    def is_eq_enabled(self) -> bool:
        return bool(lib.rb_player_is_eq_enabled(self._p))

    def set_eq_band(
        self, band: int, cutoff_hz: int, q: float, gain_db: float
    ) -> None:
        lib.rb_player_set_eq_band(
            self._p, int(band), int(cutoff_hz), float(q), float(gain_db)
        )

    def set_eq_precut(self, db: float) -> None:
        lib.rb_player_set_eq_precut(self._p, float(db))

    def set_eq_preset(self, preset: int) -> None:
        """preset: :class:`rockbox_ffi.enums.EqPreset`."""
        lib.rb_player_set_eq_preset(self._p, int(preset))

    def set_tone(
        self,
        bass_db: int,
        treble_db: int,
        bass_cutoff_hz: int,
        treble_cutoff_hz: int,
    ) -> None:
        lib.rb_player_set_tone(
            self._p, int(bass_db), int(treble_db),
            int(bass_cutoff_hz), int(treble_cutoff_hz),
        )

    def set_bass(self, bass_db: int) -> None:
        lib.rb_player_set_bass(self._p, int(bass_db))

    def set_treble(self, treble_db: int) -> None:
        lib.rb_player_set_treble(self._p, int(treble_db))

    def set_bass_cutoff(self, hz: int) -> None:
        """Bass shelf cutoff in Hz; ``0`` restores the 200 Hz default."""
        lib.rb_player_set_bass_cutoff(self._p, int(hz))

    def set_treble_cutoff(self, hz: int) -> None:
        """Treble shelf cutoff in Hz; ``0`` restores the 3.5 kHz default."""
        lib.rb_player_set_treble_cutoff(self._p, int(hz))

    def set_crossfeed(
        self,
        mode: int,
        direct_gain: int,
        cross_gain: int,
        hf_gain: int,
        hf_cutoff: int,
    ) -> None:
        """Crossfeed (headphone stereo narrowing).

        ``mode``: :class:`rockbox_ffi.enums.CrossfeedMode` (OFF=0, MEIER=1,
        CUSTOM=2). ``direct_gain``/``cross_gain``/``hf_gain`` are in tenths of a
        dB (``<= 0``); ``hf_cutoff`` is in Hz.
        """
        lib.rb_player_set_crossfeed(
            self._p, int(mode), int(direct_gain), int(cross_gain),
            int(hf_gain), int(hf_cutoff),
        )

    def set_bass_enhancement(self, strength: int, precut: int) -> None:
        """Perceptual bass enhancement. ``strength`` is a percent (``0`` = off);
        ``precut`` is in tenths of a dB (``<= 0``)."""
        lib.rb_player_set_bass_enhancement(self._p, int(strength), int(precut))

    def set_fatigue_reduction(self, strength: int) -> None:
        """Auditory fatigue reduction. ``strength``: 0 off, 1 weak, 2 moderate,
        3 strong."""
        lib.rb_player_set_fatigue_reduction(self._p, int(strength))

    def set_surround(
        self,
        delay_ms: int,
        balance: int,
        cutoff_low_hz: int,
        cutoff_high_hz: int,
    ) -> None:
        lib.rb_player_set_surround(
            self._p, int(delay_ms), int(balance),
            int(cutoff_low_hz), int(cutoff_high_hz),
        )

    def set_channel_mode(self, mode: int) -> None:
        """mode: :class:`rockbox_ffi.enums.ChannelMode`."""
        lib.rb_player_set_channel_mode(self._p, int(mode))

    def set_stereo_width(self, percent: int) -> None:
        lib.rb_player_set_stereo_width(self._p, int(percent))

    def set_compressor(
        self,
        threshold_db: int,
        makeup_gain: int,
        ratio: int,
        knee: int,
        attack_ms: int,
        release_ms: int,
    ) -> None:
        lib.rb_player_set_compressor(
            self._p, int(threshold_db), int(makeup_gain), int(ratio),
            int(knee), int(attack_ms), int(release_ms),
        )

    def set_dither(self, enabled: bool) -> None:
        lib.rb_player_set_dither(self._p, bool(enabled))

    def set_pitch(self, ratio: int) -> None:
        lib.rb_player_set_pitch(self._p, int(ratio))

    def dsp_settings(self) -> dict:
        s = take_string(lib.rb_player_dsp_settings_json(self._p))
        if s is None:
            raise RuntimeError("rb_player_dsp_settings_json returned NULL")
        return json.loads(s)

    # -- status -----------------------------------------------------------
    def status(self) -> dict:
        s = take_string(lib.rb_player_status_json(self._p))
        if s is None:
            raise RuntimeError("rb_player_status_json returned NULL")
        return json.loads(s)

    # -- resume -----------------------------------------------------------
    def resume(self) -> Optional[dict]:
        """Restore the queue + exact position (does NOT auto-play).

        Returns a dict ``{tracks, index, elapsed_ms}`` or ``None`` if there is
        nothing to resume.
        """
        s = take_string(lib.rb_player_resume(self._p))
        if s is None:
            return None
        return json.loads(s)

    def save_resume(self) -> None:
        lib.rb_player_save_resume(self._p)

    def clear_resume(self) -> None:
        lib.rb_player_clear_resume(self._p)

    # -- m3u / m3u8 playlists ---------------------------------------------
    def import_m3u(
        self, path: str, position: int, index: int = 0
    ) -> Optional[List[str]]:
        """Import a playlist file into the queue at ``position``.

        Returns the imported paths, or ``None`` on error.
        """
        s = take_string(
            lib.rb_player_import_m3u(
                self._p, path.encode("utf-8"), int(position), int(index)
            )
        )
        if s is None:
            return None
        return json.loads(s)

    def load_m3u(self, path: str) -> Optional[List[str]]:
        """Replace the queue with a playlist file; returns loaded paths."""
        s = take_string(lib.rb_player_load_m3u(self._p, path.encode("utf-8")))
        if s is None:
            return None
        return json.loads(s)

    def export_m3u(self, path: str) -> bool:
        """Export the current queue to an ``.m3u8``; ``True`` on success."""
        return int(lib.rb_player_export_m3u(self._p, path.encode("utf-8"))) == 0
