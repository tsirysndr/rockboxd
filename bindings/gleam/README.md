# rockbox_ffi (Gleam)

[![Package Version](https://img.shields.io/hexpm/v/rockbox_ffi)](https://hex.pm/packages/rockbox_ffi)
[![Hex Docs](https://img.shields.io/badge/hex-docs-ffaff3)](https://hexdocs.pm/rockbox_ffi/)
![Gleam](https://img.shields.io/badge/Gleam-%E2%89%A51.0-FFAFF3?logo=gleam&logoColor=white)
![Erlang/OTP](https://img.shields.io/badge/Erlang%2FOTP-27%2B-A90533?logo=erlang&logoColor=white)
![NIF](https://img.shields.io/badge/native-erl__nif-5C4B8A)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

Gleam bindings for the Rockbox **DSP**, **metadata**, and **playback**
engine (Erlang target), via an `erl_nif` shim over the `librockbox_ffi`
C ABI.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

## Setup

```sh
gleam add rockbox_ffi
```

Requires OTP 27+ (JSON is decoded with the built-in `json` module plus
`gleam/dynamic/decode` — no `gleam_json` dependency).

### How the native code is delivered

Gleam has no compile step at `gleam add` time, so the package **ships prebuilt
NIF binaries** — one `.so` per platform under `priv/`, named
`rockbox_ffi_nif-<target>.so`. The Erlang loader (`src/rockbox_ffi_nif.erl`)
detects the host triple at load time and loads the matching one (falling back to
an unsuffixed `rockbox_ffi_nif.so` from a local `make` build).

Prebuilt targets:

| Target                    | Tier         |
| ------------------------- | ------------ |
| `aarch64-apple-darwin`    | supported    |
| `x86_64-apple-darwin`     | supported    |
| `x86_64-linux-gnu`        | supported    |
| `aarch64-linux-gnu`       | supported    |
| `x86_64-unknown-freebsd`  | best-effort  |
| `x86_64-unknown-netbsd`   | best-effort  |

On any other platform (musl/Alpine, Windows, or a glibc older than the CI
runner's) `load_nif` will fail — build from source instead.

### Building from source

A from-source build needs the **full monorepo checkout** (the Cargo workspace
and `include/` header are not in the Hex package):

```sh
cd bindings/gleam
make            # -> priv/rockbox_ffi_nif.so  (Gleam copies priv/ into its build)
gleam test
```

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

## Releasing (maintainers)

Publishing is automated by the `bindings-gleam-release.yml` GitHub Actions
workflow (needs a `HEXPM_API_KEY` repo secret). It builds the NIF `.so` on one
native runner per target, gathers every arch's `.so` into `priv/` on a single
machine, then runs `gleam publish` with the whole multi-arch `priv/` bundled.

1. Bump `version` in `gleam.toml` (Hex versions are immutable).
2. Trigger the workflow: push a `gleam-v<version>` tag, or run it from the
   Actions tab (`workflow_dispatch`, leave *publish* checked).

Nothing prebuilt is committed to git (`.gitignore` keeps `priv/*.so` out); the
binaries are produced fresh by CI for each release.
