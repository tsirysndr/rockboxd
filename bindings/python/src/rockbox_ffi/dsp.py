"""The DSP pipeline: EQ, tone, surround, compressor, ReplayGain, resampler."""

from __future__ import annotations

import array
import math
from typing import Iterable, Optional

from ._ffi import ffi, lib

_NAN = float("nan")


def _opt(v: Optional[float]) -> float:
    """None -> NaN, the ABI's 'tag absent' sentinel."""
    return _NAN if v is None else float(v)


class Dsp:
    """Interleaved-S16LE-stereo DSP instance.

    The underlying ``dsp_config`` is a process-wide singleton, so only one
    :class:`Dsp` may exist at a time and it must be used from one thread.
    Use as a context manager, or call :meth:`close` when done.
    """

    def __init__(self, sample_rate: int):
        self._p = lib.rb_dsp_new(int(sample_rate))
        if self._p == ffi.NULL:
            raise RuntimeError("rb_dsp_new returned NULL")

    # -- lifecycle --------------------------------------------------------
    def close(self) -> None:
        if getattr(self, "_p", ffi.NULL) != ffi.NULL:
            lib.rb_dsp_free(self._p)
            self._p = ffi.NULL

    def __del__(self):
        self.close()

    def __enter__(self) -> "Dsp":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    # -- configuration ----------------------------------------------------
    def set_input_frequency(self, hz: int) -> None:
        lib.rb_dsp_set_input_frequency(self._p, int(hz))

    def flush(self) -> None:
        lib.rb_dsp_flush(self._p)

    def eq_enable(self, enable: bool) -> None:
        lib.rb_dsp_eq_enable(self._p, bool(enable))

    def set_eq_band(self, band: int, cutoff_hz: int, q: float, gain_db: float) -> None:
        """Configure one EQ band (0..=9). Band 0 low shelf, 9 high shelf."""
        lib.rb_dsp_set_eq_band(self._p, int(band), int(cutoff_hz), float(q), float(gain_db))

    def set_eq_precut(self, db: float) -> None:
        lib.rb_dsp_set_eq_precut(self._p, float(db))

    def set_tone(self, bass_db: int, treble_db: int) -> None:
        lib.rb_dsp_set_tone(self._p, int(bass_db), int(treble_db))

    def set_tone_cutoffs(self, bass_hz: int, treble_hz: int) -> None:
        lib.rb_dsp_set_tone_cutoffs(self._p, int(bass_hz), int(treble_hz))

    def set_surround(self, delay_ms: int, balance: int, fx1: int, fx2: int) -> None:
        lib.rb_dsp_set_surround(self._p, int(delay_ms), int(balance), int(fx1), int(fx2))

    def set_channel_config(self, mode: int) -> None:
        lib.rb_dsp_set_channel_config(self._p, int(mode))

    def set_stereo_width(self, percent: int) -> None:
        lib.rb_dsp_set_stereo_width(self._p, int(percent))

    def set_compressor(
        self,
        threshold: int,
        makeup_gain: int,
        ratio: int,
        knee: int,
        release_time: int,
        attack_time: int,
    ) -> None:
        lib.rb_dsp_set_compressor(
            self._p, int(threshold), int(makeup_gain), int(ratio),
            int(knee), int(release_time), int(attack_time),
        )

    def set_replaygain(self, mode: int, noclip: bool, preamp_db: float) -> None:
        """mode: see :class:`rockbox_ffi.enums.DspReplayGainMode`
        (TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3)."""
        lib.rb_dsp_set_replaygain(self._p, int(mode), bool(noclip), float(preamp_db))

    def set_replaygain_gains(
        self,
        track_gain_db: Optional[float] = None,
        album_gain_db: Optional[float] = None,
        track_peak: Optional[float] = None,
        album_peak: Optional[float] = None,
    ) -> None:
        """Per-track gains in plain dB / peaks as linear amplitude
        (1.0 = full scale). None for any absent tag."""
        lib.rb_dsp_set_replaygain_gains(
            self._p, _opt(track_gain_db), _opt(album_gain_db),
            _opt(track_peak), _opt(album_peak),
        )

    def set_replaygain_gains_raw(
        self, track_gain: int, album_gain: int, track_peak: int, album_peak: int
    ) -> None:
        """Native Q7.24 linear factors (the ``raw_*`` fields from
        :func:`rockbox_ffi.metadata.read`), 0 = not tagged."""
        lib.rb_dsp_set_replaygain_gains_raw(
            self._p, int(track_gain), int(album_gain), int(track_peak), int(album_peak)
        )

    # -- processing -------------------------------------------------------
    def process(self, samples: Iterable[int]) -> array.array:
        """Run interleaved stereo S16 ``samples`` through the pipeline.

        Accepts any int16 sequence (list, ``array('h')``, bytes-like via
        ``array``). Returns a new ``array('h')`` of processed samples
        (length may differ from the input — the resampler buffers).
        """
        buf = samples if isinstance(samples, array.array) and samples.typecode == "h" \
            else array.array("h", samples)
        n = len(buf)
        if n % 2 != 0:
            raise ValueError("input must be interleaved stereo (even length)")

        cin = ffi.cast("const int16_t *", ffi.from_buffer(buf))
        out_len = ffi.new("size_t *")
        out_ptr = lib.rb_dsp_process(self._p, cin, n, out_len)

        produced = int(out_len[0])
        if out_ptr == ffi.NULL or produced == 0:
            return array.array("h")
        try:
            result = array.array("h")
            result.frombytes(ffi.buffer(out_ptr, produced * 2)[:])
            return result
        finally:
            lib.rb_buffer_free(out_ptr, produced)


def sine_stereo(freq_hz: float, seconds: float, rate: int, amplitude: int = 16000) -> array.array:
    """Helper: generate ``seconds`` of a sine as interleaved stereo int16."""
    n = int(seconds * rate)
    buf = array.array("h")
    for i in range(n):
        s = int(math.sin(i * 2.0 * math.pi * freq_hz / rate) * amplitude)
        s = max(-32768, min(32767, s))
        buf.append(s)
        buf.append(s)
    return buf
