# rockbox_ex_ffi (Elixir)

[![Hex.pm](https://img.shields.io/hexpm/v/rockbox_ex_ffi.svg?logo=elixir)](https://hex.pm/packages/rockbox_ex_ffi)
[![Hex Docs](https://img.shields.io/badge/hex-docs-lightgreen.svg)](https://hexdocs.pm/rockbox_ex_ffi/)
![Elixir](https://img.shields.io/badge/Elixir-1.15%2B-4B275F?logo=elixir&logoColor=white)
![Erlang/OTP](https://img.shields.io/badge/Erlang%2FOTP-27%2B-A90533?logo=erlang&logoColor=white)
![NIF](https://img.shields.io/badge/native-erl__nif-5C4B8A)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

Elixir bindings for the Rockbox **DSP**, **metadata**, and **playback**
engine, via an `erl_nif` shim over the `librockbox_ffi` C ABI.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

## Setup

The NIF links the Rust static archive (`target/release/librockbox_ffi.a`);
`mix compile` builds it automatically through `elixir_make` (the `Makefile`
runs `cargo build --release -p rockbox-ffi` first if the archive is missing).

```sh
cd bindings/elixir
mix deps.get
mix compile
mix test
```

Requires OTP 27+ (uses the built-in `:json` module — no `jason` dependency).

Generate the API docs locally with [ExDoc](https://hexdocs.pm/ex_doc/):

```sh
mix docs        # -> doc/index.html
```

Published docs live at <https://hexdocs.pm/rockbox_ex_ffi/>.

## Usage

```elixir
# --- metadata ---
{:ok, meta} = Rockbox.Metadata.read("song.flac")
meta.artist       # "…"
meta.duration_ms  # 122324
Rockbox.Metadata.probe("track.opus")   # "Opus"

# --- DSP (interleaved stereo int16 binary) ---
d = Rockbox.Dsp.new(44_100)
Rockbox.Dsp.eq_enable(d, true)
Rockbox.Dsp.set_eq_band(d, 0, 60, 0.7, 3.0)
Rockbox.Dsp.set_replaygain(d, 0, true, 0.0)          # 0 = track (DSP-native)
Rockbox.Dsp.set_replaygain_gains(d, -6.02, nil, nil, nil)
out = Rockbox.Dsp.process(d, pcm_binary)             # int16 LE in/out

# --- playback (needs an output device) ---
p = Rockbox.Player.new(volume: 0.8, crossfade_mode: 5)   # 5 = always
Rockbox.Player.set_replaygain(p, 1, 0.0, true)           # 1 = track (player)
Rockbox.Player.set_queue(p, ["a.flac", "b.mp3"])
Rockbox.Player.play(p)
Rockbox.Player.status(p)   # %{state: "playing", index: 0, ...}
```

Handles (`Rockbox.Dsp` / `Rockbox.Player`) are NIF resources freed by the
BEAM garbage collector — no explicit close.

## Two ReplayGain encodings

The DSP and player use *different* mode integers (a quirk of the C ABI):

- `Rockbox.Dsp.set_replaygain/4` → `0` track, `1` album, `2` shuffle, `3` off
- `Rockbox.Player.set_replaygain/4` → `0` off, `1` track, `2` album

## Shared NIF

`c_src/rockbox_ffi_nif.c` and `src/rockbox_ffi_nif.erl` are shared verbatim
with the Gleam binding (`bindings/gleam/`).
