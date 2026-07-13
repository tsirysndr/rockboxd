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

Add it to your `mix.exs` and fetch:

```elixir
def deps do
  [{:rockbox_ex_ffi, "~> 0.4"}]
end
```

```sh
mix deps.get
mix compile   # downloads a precompiled NIF — no Rust toolchain needed
```

Requires OTP 27+ (uses the built-in `:json` module — no `jason` dependency).

### How the native code is delivered

The package ships **no** Rust source and **no** static archive. On `mix compile`,
[`elixir_make`](https://hex.pm/packages/elixir_make) +
[`cc_precompiler`](https://hex.pm/packages/cc_precompiler) download a prebuilt
`priv/rockbox_ffi_nif.so` matching your OS/arch from the project's GitHub
releases and verify it against the shipped `checksum-rockbox_ex_ffi.exs`.

Prebuilt targets:

| Target                    | Runner        | Tier         |
| ------------------------- | ------------- | ------------ |
| `aarch64-apple-darwin`    | macOS (Apple) | supported    |
| `x86_64-apple-darwin`     | macOS (Intel) | supported    |
| `x86_64-linux-gnu`        | Linux x86-64  | supported    |
| `aarch64-linux-gnu`       | Linux arm64   | supported    |
| `x86_64-unknown-freebsd`  | FreeBSD amd64 | best-effort  |
| `x86_64-unknown-netbsd`   | NetBSD amd64  | best-effort  |

The \*BSD artifacts are built in a VM and may lag or be absent for a given
release. Because cc_precompiler auto-detects an OS-version-suffixed triple on
\*BSD (e.g. `amd64-portbld-freebsd14.0`), a BSD consumer must pin the canonical
triple to fetch the prebuilt NIF:

```sh
TARGET_ARCH=x86_64 TARGET_OS=unknown TARGET_ABI=freebsd mix deps.get
TARGET_ARCH=x86_64 TARGET_OS=unknown TARGET_ABI=freebsd mix compile
```

Any other platform (musl/Alpine, Windows, or a glibc older than the CI
runner's) has no prebuilt NIF — build from source instead (see below).

### Building from source

A from-source build needs the **full monorepo checkout** — the Cargo workspace
and `include/` header are not in the Hex package. Set `ROCKBOX_FFI_BUILD=1` (or
use a `-dev` version) to force `make` to run `cargo build --release -p rockbox-ffi`
and link the NIF locally:

```sh
cd bindings/elixir
ROCKBOX_FFI_BUILD=1 mix deps.get
ROCKBOX_FFI_BUILD=1 mix compile
ROCKBOX_FFI_BUILD=1 mix test
```

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

## Releasing (maintainers)

Publishing is automated by the `bindings-elixir-release.yml` GitHub Actions
workflow (needs a `HEX_API_KEY` repo secret). It builds the precompiled NIF on
one native runner per target, uploads the tarballs to a single **rolling**
GitHub release (`rockbox-ffi-nif`), then generates the checksum file and runs
`mix hex.publish`.

1. Bump `@version` in `mix.exs` (Hex versions are immutable — you cannot
   republish over an existing one).
2. Trigger the workflow: push a `elixir-v<version>` tag, or run it from the
   Actions tab (`workflow_dispatch`, leave *publish* checked).

The URL template in `mix.exs` intentionally omits the version (elixir_make only
substitutes `@{artefact_filename}`, which already embeds the version), so every
release's artifacts pile up under the same `rockbox-ffi-nif` tag — no per-bump
URL edit needed.

To reproduce a target's artifact locally:

```sh
cd bindings/elixir
MIX_ENV=prod CC_PRECOMPILER_PRECOMPILE_ONLY_LOCAL=true mix elixir_make.precompile
# -> ~/.cache/elixir_make (Linux) or ~/Library/Caches/elixir_make (macOS)
```
