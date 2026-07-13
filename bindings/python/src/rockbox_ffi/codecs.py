"""Decode an audio file to interleaved-stereo S16 PCM, one chunk at a time.

Wraps the Rockbox codec engine. The codec state is process-wide, so only one
:class:`Decoder` may decode at a time — constructing a second one blocks until
the first is closed. Use as a context manager or call :meth:`close`.
"""

from __future__ import annotations

import array
import json
from typing import Optional, Tuple

from ._ffi import ffi, lib, take_string


class Decoder:
    """A streaming decoder for one audio file.

    Iterate :meth:`chunks` (or call :meth:`next_chunk`) to pull decoded PCM
    until end of track.
    """

    def __init__(self, path: str):
        self._d = lib.rb_decoder_open(path.encode("utf-8"))
        if self._d == ffi.NULL:
            raise ValueError(f"could not open decoder for {path!r}")

    # -- lifecycle --------------------------------------------------------
    def close(self) -> None:
        if getattr(self, "_d", ffi.NULL) != ffi.NULL:
            lib.rb_decoder_free(self._d)
            self._d = ffi.NULL

    def __del__(self):
        self.close()

    def __enter__(self) -> "Decoder":
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    # -- metadata ---------------------------------------------------------
    def metadata(self) -> dict:
        """Tags and stream properties (same shape as ``metadata.read``)."""
        s = take_string(lib.rb_decoder_metadata_json(self._d))
        if s is None:
            raise RuntimeError("rb_decoder_metadata_json returned NULL")
        return json.loads(s)

    # -- decoding ---------------------------------------------------------
    def next_chunk(self) -> Optional[Tuple[array.array, int]]:
        """Next buffer of decoded PCM as ``(samples, sample_rate)``.

        ``samples`` is an ``array('h')`` of interleaved stereo int16. Returns
        ``None`` at end of track (or after a codec error — see
        :meth:`finished`).
        """
        out_len = ffi.new("size_t *")
        out_rate = ffi.new("uint32_t *")
        ptr = lib.rb_decoder_next_chunk(self._d, out_len, out_rate)
        n = int(out_len[0])
        if ptr == ffi.NULL or n == 0:
            return None
        try:
            samples = array.array("h")
            samples.frombytes(ffi.buffer(ptr, n * 2)[:])
            return samples, int(out_rate[0])
        finally:
            lib.rb_buffer_free(ptr, n)

    def chunks(self):
        """Yield ``(samples, sample_rate)`` chunks until end of track."""
        while True:
            chunk = self.next_chunk()
            if chunk is None:
                return
            yield chunk

    def seek_ms(self, ms: int) -> None:
        """Request a seek to ``ms`` milliseconds."""
        lib.rb_decoder_seek_ms(self._d, int(ms))

    def elapsed_ms(self) -> int:
        """Playback position last reported by the codec, in milliseconds."""
        return int(lib.rb_decoder_elapsed_ms(self._d))

    def finished(self) -> Tuple[bool, int]:
        """``(done, code)``: ``done`` True once the track has ended; ``code``
        is the exit status (0 = clean end, negative = codec error), valid
        only when ``done``."""
        out_code = ffi.new("int32_t *")
        done = bool(lib.rb_decoder_finished(self._d, out_code))
        return done, int(out_code[0])
