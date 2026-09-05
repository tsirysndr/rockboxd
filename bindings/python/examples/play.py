"""Play an audio source through the real output device.

The queue entry can be a local file, a remote `http(s)://` file, an
internet-radio stream, or an **HLS** (`.m3u8`) / **MPEG-DASH** (`.mpd`)
manifest — the engine detects each kind automatically.

Run:
    uv run python examples/play.py [path-or-URL]
    uv run python examples/play.py hls    # public HLS test stream
    uv run python examples/play.py dash   # public MPEG-DASH test stream
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

# Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
# for more).
HLS_DEFAULT = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"
DASH_DEFAULT = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd"


def main() -> None:
    arg = sys.argv[1] if len(sys.argv) > 1 else str(FIXTURE)
    file = {"hls": HLS_DEFAULT, "dash": DASH_DEFAULT}.get(arg, arg)

    player = Player(volume=0.8)
    player.set_queue([file])
    # DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift.
    player.set_eq_preset(EqPreset.BASS_BOOST)
    player.set_bass(7)
    player.set_treble(4)
    player.play()
    print(f"▶ playing {file}")
    print("eq: BassBoost preset, bass +7 dB, treble +4 dB")

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
    # A live stream reports duration 0 and plays until Ctrl-C.
    while True:
        st = player.status()
        pos = st["position_ms"] / 1000
        dur = st["duration_ms"] / 1000
        clock = f"{pos:.1f}s / LIVE" if dur == 0 else f"{pos:.1f}s / {dur:.1f}s"
        codec = (st.get("metadata") or {}).get("codec", "")
        print(f"\r[{st['state']}] {codec} {clock}   ", end="", flush=True)
        if st["state"] == "stopped" and st["position_ms"] > 0:
            print("\n✔ done")
            break
        time.sleep(0.5)

    os._exit(0)


if __name__ == "__main__":
    main()
