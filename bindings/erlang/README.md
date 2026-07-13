# rockbox_ffi_nif

[![Hex.pm](https://img.shields.io/hexpm/v/rockbox_ffi_nif.svg)](https://hex.pm/packages/rockbox_ffi_nif)
[![HexDocs](https://img.shields.io/badge/hex-docs-lightgreen.svg)](https://hexdocs.pm/rockbox_ffi_nif/)

Shared Erlang NIF over the [`librockbox_ffi`](https://github.com/tsirysndr/rockboxd/tree/master/crates/rockbox-ffi) C ABI —
the Rockbox DSP / metadata / codecs / playback engine exposed to the BEAM.

This is the **common native layer** for the language bindings that run on the
BEAM. The [Elixir](https://github.com/tsirysndr/rockboxd/tree/master/bindings/elixir) (`rockbox_ex_ffi`) and [Gleam](https://github.com/tsirysndr/rockboxd/tree/master/bindings/gleam)
(`rockbox_ffi`) packages both depend on this one instead of each vendoring
their own copy of the C shim and Erlang loader; the ergonomic wrappers live in
those packages.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

## What's here

| Path                          | Role                                                            |
| ----------------------------- | --------------------------------------------------------------- |
| `c_src/rockbox_ffi_nif.c`     | erl_nif shim — a 1:1 surface of `include/rockbox_ffi.h`          |
| `src/rockbox_ffi_nif.erl`     | NIF loader + stub module; downloads the `.so` on first load     |
| `priv/rockbox_ffi_nif.manifest` | per-triple sha256 checksums for the first-load download       |
| `Makefile`                    | local-dev build of `priv/rockbox_ffi_nif.so`                    |

## Usage

Every function is a raw NIF: strings/paths are **UTF-8 binaries**, `*_json`
functions return raw JSON binaries (decode with OTP 27's `json` module), and
handles (`RbDecoder`, `RbPlayer`, `RbDsp`) are opaque `reference()`s freed by
the GC — there is no explicit close. For application code prefer the ergonomic
[Elixir](https://github.com/tsirysndr/rockboxd/tree/master/bindings/elixir) or
[Gleam](https://github.com/tsirysndr/rockboxd/tree/master/bindings/gleam)
wrappers; the examples below use the raw surface directly.

### Read metadata

```erlang
%% Guess the codec from the extension (no I/O)…
<<"FLAC">> = rockbox_ffi_nif:meta_probe(<<"song.flac">>),

%% …or parse the full tag/stream properties.
Json = rockbox_ffi_nif:meta_read_json(<<"/music/song.flac">>),
#{<<"title">>       := Title,
  <<"artist">>      := Artist,
  <<"duration_ms">> := DurMs,
  <<"sample_rate">> := Rate} = json:decode(Json).
```

### Decode a file to PCM

`decoder_next_chunk/1` hands back interleaved-stereo little-endian `int16`
PCM until it returns `nil` at end of track.

```erlang
D = rockbox_ffi_nif:decoder_open(<<"/music/song.mp3">>),

%% Metadata is available from the open decoder too.
Meta = json:decode(rockbox_ffi_nif:decoder_metadata_json(D)),

%% Pull every chunk into one PCM binary.
Drain = fun Drain(Acc) ->
    case rockbox_ffi_nif:decoder_next_chunk(D) of
        nil          -> lists:reverse(Acc);
        {Pcm, _Rate} -> Drain([Pcm | Acc])
    end
end,
Pcm = iolist_to_binary(Drain([])),

%% Confirm the track ended cleanly (0 = clean, negative = codec error).
{true, 0} = rockbox_ffi_nif:decoder_finished(D).
```

Seek before decoding more:

```erlang
rockbox_ffi_nif:decoder_seek_ms(D, 30000),   %% jump to 30s
Ms = rockbox_ffi_nif:decoder_elapsed_ms(D).
```

### Run PCM through the DSP pipeline

```erlang
Dsp = rockbox_ffi_nif:dsp_new(44100),

%% Turn on the equalizer and lift a 1 kHz band by 6 dB.
rockbox_ffi_nif:dsp_eq_enable(Dsp, true),
rockbox_ffi_nif:dsp_set_eq_band(Dsp, 4, 1000, 1.0, 6.0),

Out = rockbox_ffi_nif:dsp_process(Dsp, Pcm).
```

### Play a queue

A player owns a live audio device and a background engine thread; dropping the
last reference to the handle stops playback.

```erlang
P = rockbox_ffi_nif:player_new(),

%% Queue local files and/or http(s)/stream URLs.
Queue = json:encode([<<"/music/a.mp3">>, <<"/music/b.flac">>]),
rockbox_ffi_nif:player_set_queue_json(P, Queue),
rockbox_ffi_nif:player_enqueue(P, <<"http://host/stream.mp3">>),

rockbox_ffi_nif:player_set_volume(P, 0.8),
rockbox_ffi_nif:player_play(P),

%% Transport controls.
rockbox_ffi_nif:player_next(P),
rockbox_ffi_nif:player_seek_ms(P, 15000),

%% Poll playback state.
Status = json:decode(rockbox_ffi_nif:player_status_json(P)),

rockbox_ffi_nif:player_pause(P).
```

The player exposes the same DSP chain as the standalone pipeline —
`player_set_eq_band/5`, `player_set_tone/5`, `player_set_crossfade/7`,
`player_set_replaygain/4`, and more (see the
[module docs](https://hexdocs.pm/rockbox_ffi_nif/)).

## NIF delivery

The compiled NIF is a multi-megabyte shared object (it statically links the
Rust `librockbox_ffi.a`), too large to bundle in a Hex tarball. So the Hex
package ships only the checksum manifest, and `rockbox_ffi_nif.erl`'s `-on_load`
hook downloads the matching `rockbox_ffi_nif-<triple>.so` from the GitHub
release named in the manifest into the user cache on first use, verifying it
against the manifest sha256. This is why both the Elixir and Gleam bindings get
their native code the same way.

## Local development

Inside a full monorepo checkout (needs the Cargo workspace + `include/` header):

```sh
# 1. Build the Rust static archive.
cargo build --release -p rockbox-ffi

# 2. Build the NIF into priv/rockbox_ffi_nif.so.
cd bindings/erlang && make

# 3. Compile the Erlang app.
rebar3 compile
```

The Elixir/Gleam bindings pick up this locally-built `.so` via a path dependency
on `../erlang` — the loader prefers a local `priv/*.so` over any cached download.

## Supported prebuilt targets

`aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-linux-gnu`,
`x86_64-linux-gnu`, `x86_64-unknown-freebsd`, `x86_64-unknown-netbsd`.
Requires OTP 27+ (the bindings decode JSON with the built-in `json` module).
Other platforms build from source (steps above).
