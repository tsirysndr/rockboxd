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

This package contains **only the Gleam wrappers** — no C shim and no `.so`. The
native code lives in a separate, shared package,
[`rockbox_ffi_nif`](../erlang) (a `rebar3` Hex package that the Elixir binding
depends on too), which `gleam add rockbox_ffi` pulls in automatically.

On the **first load** of the NIF, `rockbox_ffi_nif`'s Erlang loader
(`rockbox_ffi_nif.erl`) detects the host triple, downloads the matching
prebuilt `rockbox_ffi_nif-<target>.so` from its GitHub release into your user
cache, and verifies it against a shipped sha256 manifest. (The `.so` statically
links the Rust engine and is far too large to bundle in a Hex tarball.)

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
runner's) the download has no matching artifact and `load_nif` will fail —
build from source instead.

### Building from source

A from-source build needs the **full monorepo checkout** (the Cargo workspace
and `include/` header are not in any Hex package). The native code builds in the
shared `rockbox_ffi_nif` package, which this binding uses via a sibling path
dependency (`../erlang`):

```sh
# Build the shared NIF once; the loader prefers this local .so over a download.
cargo build --release -p rockbox-ffi
cd bindings/erlang && make && cd ../gleam
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

## Shared native package

The C shim and Erlang NIF loader are **not** in this package — they live in the
shared [`rockbox_ffi_nif`](../erlang) package, which the Elixir binding
(`bindings/elixir/`) depends on as well. This package carries only the Gleam
`rockbox/*` wrappers and declares `rockbox_ffi_nif` as a dependency.

## Releasing (maintainers)

Because the native code is shared, publish `rockbox_ffi_nif` **first**, then
this package.

1. **Native NIFs** — bump `{vsn, ...}` in `bindings/erlang/src/rockbox_ffi_nif.app.src`
   (and the mirrored `version` in `bindings/erlang/gleam.toml`), then run the
   `bindings-erlang-release.yml` workflow (push an `erlang-v<version>` tag or
   dispatch it) to build + upload the per-target `.so` files, and publish the
   package to Hex locally:
   ```sh
   bindings/scripts/publish-erlang.sh
   ```
2. **This package** — bump `version` in `gleam.toml` (Hex versions are
   immutable), then publish locally (`gleam publish` needs an interactive Hex
   login that can't run in CI):
   ```sh
   gleam hex authenticate              # or: export HEXPM_API_KEY=...
   bindings/scripts/publish-gleam.sh   # swaps the path dep for the Hex version, then gleam publish
   ```
   The script temporarily rewrites the `rockbox_ffi_nif = { path = "../erlang" }`
   line in `gleam.toml` to the released Hex version requirement for publishing,
   then restores it. Pass `--tag` / `--repo` to override, or `--dry-run` to
   preview.
