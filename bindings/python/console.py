#!/usr/bin/env python
"""Interactive IPython console with the Python binding preloaded.

Run: `uv run python console.py`  (install IPython first: `uv pip install ipython`;
falls back to the plain REPL when IPython is absent).

Loads rockbox_ffi and points FIXTURE at a bundled sample track so you can poke
at metadata / Dsp / Player straight away.
"""

from __future__ import annotations

from pathlib import Path

import rockbox_ffi as rb
from rockbox_ffi import Dsp, Player, metadata
from rockbox_ffi.enums import (
    ChannelConfig,
    CrossfadeMode,
    DspReplayGainMode,
    MixMode,
    ReplayGainMode,
)

REPO = Path(__file__).resolve().parents[2]  # bindings/python/console.py -> repo root
FIXTURE = REPO / "crates" / "rocksky" / "fixtures" / "08 - Internet Money - Speak(Explicit).m4a"

BANNER = f"""rockbox_ffi — ABI {rb.abi_version()}
Loaded: rb, metadata, Dsp, Player + enums. FIXTURE = a sample track.
Try:
  metadata.read(str(FIXTURE))
  metadata.probe("song.flac")
  p = Player(volume=0.6); p.set_queue([str(FIXTURE)]); p.play()
"""


def main() -> None:
    ns = {
        "rb": rb, "metadata": metadata, "Dsp": Dsp, "Player": Player,
        "FIXTURE": FIXTURE, "DspReplayGainMode": DspReplayGainMode,
        "ReplayGainMode": ReplayGainMode, "CrossfadeMode": CrossfadeMode,
        "MixMode": MixMode, "ChannelConfig": ChannelConfig,
    }
    try:
        from IPython import start_ipython
        start_ipython(argv=[], user_ns=ns, display_banner=False)
        print()  # IPython prints its own; keep our banner above it
    except ImportError:
        import code
        code.interact(banner=BANNER, local=ns)


if __name__ == "__main__":
    print(BANNER)
    main()
