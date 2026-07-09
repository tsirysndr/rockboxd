"""End-to-end smoke test for the Python bindings.

Run: `uv run python examples/smoke.py`
"""

from __future__ import annotations

import time
from pathlib import Path

import rockbox_ffi as rb
from rockbox_ffi import Dsp, Player, metadata
from rockbox_ffi.enums import DspReplayGainMode, ReplayGainMode

REPO = Path(__file__).resolve().parents[3]
FIXTURE = REPO / "crates" / "rocksky" / "fixtures" / "08 - Internet Money - Speak(Explicit).m4a"


def main() -> None:
    print(f"ABI version: {rb.abi_version()}")

    # -- metadata ---------------------------------------------------------
    meta = metadata.read(str(FIXTURE))
    print(f"codec={meta['codec']!r} title={meta['title']!r} "
          f"artist={meta['artist']!r} duration_ms={meta['duration_ms']} "
          f"sample_rate={meta['sample_rate']}")
    assert meta["codec"], "codec label should not be empty"

    probe = metadata.probe("song.flac")
    print(f"probe('song.flac') = {probe!r}")
    assert probe, "flac probe should return a label"

    # -- DSP: -6.0206 dB track gain should HALVE amplitude ----------------
    rate = 44100
    with Dsp(rate) as dsp:
        dsp.set_replaygain(DspReplayGainMode.TRACK, noclip=False, preamp_db=0.0)
        dsp.set_replaygain_gains(track_gain_db=-6.0206)  # x0.5

        sine = rb.sine_stereo(1000.0, 1.0, rate, amplitude=16000)
        out = dsp.process(sine)
        peak = max(abs(s) for s in out)
        print(f"DSP peak after -6.02 dB track gain: {peak} (expected ~8000)")
        assert 7600 <= peak <= 8400, f"peak {peak} out of range"

    # -- Player (construct only, no audible playback) ---------------------
    with Player(volume=0.0) as player:
        rate = player.sample_rate()
        print(f"Player sample_rate={rate}")
        assert rate > 0
        player.set_volume(0.0)
        player.set_queue([str(FIXTURE)])
        time.sleep(0.1)  # the queue command is applied asynchronously by the engine thread
        st = player.status()
        print(f"Player status: state={st['state']!r} queue_len={st['queue_len']}")
        assert st["state"] == "stopped"
        assert st["queue_len"] == 1

    print("\nALL CHECKS PASSED ✔")


if __name__ == "__main__":
    main()
