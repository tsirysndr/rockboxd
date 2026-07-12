"""Play an audio file through the real output device.

Run: `uv run python examples/play.py [path-to-audio]`
"""

from __future__ import annotations

import os
import signal
import sys
import time
from pathlib import Path

from rockbox_ffi import EqPreset, Player

REPO = Path(__file__).resolve().parents[3]
FIXTURE = REPO / "crates" / "rocksky" / "fixtures" / "08 - Internet Money - Speak(Explicit).m4a"


def main() -> None:
    file = sys.argv[1] if len(sys.argv) > 1 else str(FIXTURE)

    player = Player(volume=0.8)
    player.set_queue([file])
    # DSP: Bass Boost preset + a +3 dB bass/treble lift.
    player.set_eq_preset(EqPreset.BASS_BOOST)
    player.set_bass(3)
    player.set_treble(3)
    player.play()
    print(f"▶ playing {file}")
    print("eq: BassBoost preset, bass +3 dB, treble +3 dB")

    # Reinstall a SIGINT handler AFTER the player boots: the native audio
    # engine installs its own signal handler while starting the output
    # device, which otherwise swallows Ctrl-C. We os._exit() straight away
    # instead of calling player.stop()/close() — those are blocking native
    # calls that can deadlock against the engine thread. The OS reclaims the
    # output device on exit.
    def on_sigint(_sig, _frame):
        print("\nstopped")
        os._exit(130)

    signal.signal(signal.SIGINT, on_sigint)

    # Poll status until playback finishes (state returns to "stopped").
    while True:
        st = player.status()
        pos = st["position_ms"] / 1000
        dur = st["duration_ms"] / 1000
        print(f"\r[{st['state']}] {pos:.1f}s / {dur:.1f}s   ", end="", flush=True)
        if st["state"] == "stopped" and st["position_ms"] > 0:
            print("\n✔ done")
            break
        time.sleep(0.5)

    os._exit(0)


if __name__ == "__main__":
    main()
