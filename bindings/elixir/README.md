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
mix deps.get   # pulls in rockbox_ffi_nif (the shared native package)
mix compile    # no Rust toolchain needed
```

Requires OTP 27+ (uses the built-in `:json` module — no `jason` dependency).

### How the native code is delivered

This package contains **only the Elixir wrappers** — no Rust source, no C shim,
no static archive. The native code lives in a separate, shared Hex package,
[`rockbox_ffi_nif`](https://hex.pm/packages/rockbox_ffi_nif) (source in
[`bindings/erlang`](../erlang)), which the Gleam binding depends on too.

You never build it: on the **first load** of the NIF, `rockbox_ffi_nif`'s Erlang
loader downloads a prebuilt `rockbox_ffi_nif-<target>.so` matching your OS/arch
from its GitHub release into your user cache and verifies it against a shipped
sha256 manifest. (The `.so` statically links the Rust engine and is far too
large to bundle in a Hex tarball, so it can't ship in the package itself.)

Prebuilt targets:

| Target                    | Tier         |
| ------------------------- | ------------ |
| `aarch64-apple-darwin`    | supported    |
| `x86_64-apple-darwin`     | supported    |
| `x86_64-linux-gnu`        | supported    |
| `aarch64-linux-gnu`       | supported    |
| `x86_64-unknown-freebsd`  | best-effort  |
| `x86_64-unknown-netbsd`   | best-effort  |

The \*BSD artifacts are built in a VM and may lag or be absent for a given
release. Any other platform (musl/Alpine, Windows, or a glibc older than the CI
runner's) has no prebuilt NIF — build from source instead (see below).

### Building from source

A from-source build needs the **full monorepo checkout** — the Cargo workspace
and `include/` header are not in any Hex package. The native code builds in the
shared `rockbox_ffi_nif` package, and this binding picks it up via a sibling
path dependency (`../erlang`) automatically:

```sh
# 1. Build the shared NIF once (compiles the Rust archive + links the .so).
cargo build --release -p rockbox-ffi
cd bindings/erlang && make && cd ../elixir

# 2. The path dep uses that local build — no download, no version override.
mix deps.get
mix compile
mix test
```

The loader prefers a local `priv/rockbox_ffi_nif.so` over any cached download,
so the freshly built `.so` is used as-is.

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

## Shared native package

The C shim and Erlang NIF loader are **not** in this package — they live in the
shared [`rockbox_ffi_nif`](../erlang) package, which the Gleam binding
(`bindings/gleam/`) depends on as well. This package carries only the
`Rockbox.*` Elixir wrappers.

## Releasing (maintainers)

Because the native code is shared, publish `rockbox_ffi_nif` **first**, then
this package.

1. **Native NIFs** — bump `{vsn, ...}` in `bindings/erlang/src/rockbox_ffi_nif.app.src`,
   then run the `bindings-erlang-release.yml` GitHub Actions workflow (push an
   `erlang-v<version>` tag or dispatch it). It builds one `.so` per target on a
   native runner and uploads them to the `erlang-v<version>` release. Publish the
   package to Hex locally (interactive Hex auth can't run in CI):
   ```sh
   bindings/scripts/publish-erlang.sh   # writes the checksum manifest, then rebar3 hex publish
   ```
2. **This package** — bump `@version` in `mix.exs` (Hex versions are immutable),
   then publish locally:
   ```sh
   bindings/scripts/publish-elixir.sh   # sets the rockbox_ffi_nif Hex dep, mix hex.publish
   ```
   The script exports `ROCKBOX_NIF_HEX=<version>` so the published tarball
   depends on the released `rockbox_ffi_nif` Hex version rather than the local
   `../erlang` path dep used for monorepo development.
