# rockbox_ffi (Gleam)

Gleam bindings for the Rockbox **DSP**, **metadata**, and **playback**
engine (Erlang target), via an `erl_nif` shim over the `librockbox_ffi`
C ABI.

## Setup

The NIF links the Rust static archive (`target/release/librockbox_ffi.a`).
Build it with `make` (which runs `cargo build --release -p rockbox-ffi`
first if the archive is missing), then run the Gleam build:

```sh
cd bindings/gleam
make            # -> priv/rockbox_ffi_nif.so  (Gleam copies priv/ into its build)
gleam test
```

Requires OTP 27+ (JSON is decoded with the built-in `json` module plus
`gleam/dynamic/decode` — no `gleam_json` dependency).

## Usage

```gleam
import rockbox/metadata
import rockbox/dsp
import rockbox/player
import gleam/option.{None, Some}

// --- metadata ---
let assert Ok(meta) = metadata.read("song.flac")
meta.artist        // "…"
meta.duration_ms   // 122324
metadata.probe("track.opus")   // Some("Opus")

// --- DSP (interleaved stereo int16 BitArray) ---
let d = dsp.new(44_100)
dsp.eq_enable(d, True)
dsp.set_eq_band(d, 0, 60, 0.7, 3.0)
dsp.set_replaygain(d, 0, True, 0.0)                 // 0 = track (DSP-native)
dsp.set_replaygain_gains(d, Some(-6.02), None, None, None)
let out = dsp.process(d, pcm)                        // BitArray in/out

// --- playback (needs an output device) ---
let p = player.with_config(player.Config(..player.default_config(), volume: 0.8))
player.set_replaygain(p, 1, 0.0, True)              // 1 = track (player)
player.set_queue(p, ["a.flac", "b.mp3"])
player.play(p)
player.status(p)   // Status(state: "playing", index: Some(0), ...)
```

`Dsp` and `Player` are opaque NIF resources, freed by the BEAM garbage
collector — no explicit close.

## Two ReplayGain encodings

The DSP and player use *different* mode integers (a quirk of the C ABI):

- `dsp.set_replaygain` → `0` track, `1` album, `2` shuffle, `3` off
- `player.set_replaygain` → `0` off, `1` track, `2` album

## Shared NIF

`c_src/rockbox_ffi_nif.c` and `src/rockbox_ffi_nif.erl` are shared verbatim
with the Elixir binding (`bindings/elixir/`).
