# rockbox-ffi (Python)

[![PyPI](https://img.shields.io/pypi/v/rockbox-ffi?logo=pypi&logoColor=white)](https://pypi.org/project/rockbox-ffi/)
![Python](https://img.shields.io/badge/python-3.9%2B-3776AB?logo=python&logoColor=white)
![FFI](https://img.shields.io/badge/FFI-cffi-306998)
![uv](https://img.shields.io/badge/packaging-uv-DE5FE9?logo=uv&logoColor=white)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

Python bindings for the Rockbox **DSP**, **metadata**, and **playback**
engine, via `cffi` (ABI mode) over the prebuilt `librockbox_ffi` shared
library.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

## Setup

Build the shared library once (from the repo root):

```sh
cargo build --release -p rockbox-ffi
```

Then install the Python package with [uv](https://docs.astral.sh/uv/):

```sh
cd bindings/python
uv venv
uv pip install -e .
uv run python examples/smoke.py
```

The library is located automatically by walking up to
`target/release/librockbox_ffi.{dylib,so}`. Override with the
`ROCKBOX_FFI_LIB` environment variable.

## Interactive console

```sh
uv pip install -e '.[dev]'      # installs IPython
uv run python console.py
```

Drops into IPython with `rb`, `metadata`, `Dsp`, `Player`, the enums, and a
`FIXTURE` sample track preloaded (falls back to the plain REPL without
IPython):

```python
metadata.read(str(FIXTURE))["title"]        # 'Speak'
p = Player(volume=0.6)
p.set_queue([str(FIXTURE)]); p.play()
p.status()["state"]                          # 'playing'
```

## Usage

```python
import rockbox_ffi as rb
from rockbox_ffi import Dsp, Player, metadata
from rockbox_ffi.enums import DspReplayGainMode, ReplayGainMode, CrossfadeMode

# --- metadata ---------------------------------------------------------
meta = metadata.read("song.flac")
print(meta["artist"], "—", meta["title"], meta["duration_ms"], "ms")
print(metadata.probe("track.opus"))          # -> "Opus"

# --- DSP (interleaved stereo int16) -----------------------------------
with Dsp(44100) as dsp:
    dsp.eq_enable(True)
    dsp.set_eq_band(0, cutoff_hz=60, q=0.7, gain_db=3.0)
    dsp.set_replaygain(DspReplayGainMode.TRACK, noclip=True, preamp_db=0.0)
    dsp.set_replaygain_gains(track_gain_db=-6.02)   # halves amplitude
    processed = dsp.process(samples)                # array('h')

# --- playback (needs an output device) --------------------------------
with Player(volume=0.8) as player:
    player.set_replaygain(ReplayGainMode.TRACK, preamp_db=0.0, prevent_clipping=True)
    player.set_crossfade(CrossfadeMode.ALWAYS)
    player.set_queue(["a.flac", "b.mp3", "c.opus"])
    player.play()
    print(player.status())     # {'state': 'playing', 'index': 0, ...}
```

## API

| Module                 | Contents                                                              |
| ---------------------- | --------------------------------------------------------------------- |
| `rockbox_ffi.metadata` | `read(path) -> dict`, `probe(filename) -> str \| None`                |
| `rockbox_ffi.Dsp`      | EQ / tone / surround / compressor / ReplayGain, `process(samples)`    |
| `rockbox_ffi.Player`   | queue + transport + crossfade + ReplayGain, `status() -> dict`        |
| `rockbox_ffi.enums`    | `DspReplayGainMode`, `ReplayGainMode`, `CrossfadeMode`, `MixMode`, …  |

### Two ReplayGain encodings

The DSP and player use *different* mode integers (a quirk of the C ABI):

- `Dsp.set_replaygain` → `DspReplayGainMode` (`TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3`)
- `Player.set_replaygain` → `ReplayGainMode` (`OFF=0, TRACK=1, ALBUM=2`)

Use the named enums and you won't have to remember which is which.

## Memory

All heap allocations crossing the FFI boundary (JSON strings, sample
buffers) are freed inside the wrappers — you never call a `*_free` yourself.
