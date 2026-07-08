# rockbox-codecs

[![crates.io](https://img.shields.io/crates/v/rockbox-codecs.svg)](https://crates.io/crates/rockbox-codecs)
[![docs.rs](https://img.shields.io/docsrs/rockbox-codecs)](https://docs.rs/rockbox-codecs)
[![license](https://img.shields.io/badge/license-GPL--2.0--or--later-blue.svg)](https://www.gnu.org/licenses/old-licenses/gpl-2.0.en.html)

Audio decoding with [Rockbox](https://www.rockbox.org)'s codecs as a
reusable Rust library.

The decoders are the Rockbox firmware codecs — the exact code that has
played music on hundreds of DAP models for two decades — compiled into
the crate by `build.rs` and driven through Rockbox's `codec_api` by a
small C shim modeled on the upstream `warble` standalone test player.
No external decoder libraries, no subprocess.

```rust
let mut dec = rockbox_codecs::Decoder::open("song.flac")?;
println!(
    "{} — {}",
    dec.metadata().artist,
    dec.metadata().title,
);
while let Some(chunk) = dec.next_chunk() {
    // chunk.pcm: interleaved stereo i16 at chunk.sample_rate Hz
    sink.write(&chunk.pcm);
}
```

## Codecs (feature-gated)

All 29 codecs of Rockbox's static-link set (the same set the Android /
WASM builds of rockboxd link) are available and enabled by default;
disable `default-features` and pick what you need to trim compile time
and binary size.

| Feature      | Formats                                | Decoder library       | Verified by test      |
| ------------ | -------------------------------------- | --------------------- | --------------------- |
| `flac`       | FLAC                                   | libffmpegFLAC         | bit-exact roundtrip   |
| `shorten`    | Shorten (SHN)                          | libffmpegFLAC         | compiles + table      |
| `alac`       | Apple Lossless (M4A)                   | libalac + libm4a      | bit-exact roundtrip   |
| `wavpack`    | WavPack                                | libwavpack            | bit-exact roundtrip   |
| `ape`        | Monkey's Audio (APE)                   | libdemac              | compiles + table      |
| `tta`        | True Audio                             | libtta                | bit-exact roundtrip   |
| `wav`        | WAV (PCM + ADPCM family)               | libpcm                | bit-exact roundtrip   |
| `aiff`       | AIFF                                   | libpcm                | bit-exact roundtrip   |
| `au`         | Sun AU / SND                           | libpcm                | bit-exact roundtrip   |
| `wav64`      | Sony Wave64                            | libpcm                | bit-exact roundtrip   |
| `smaf`       | SMAF (Yamaha mobile)                   | libpcm                | compiles + table      |
| `vox`        | Dialogic VOX ADPCM                     | libpcm                | compiles + table      |
| `mpa`        | MP1 / MP2 / MP3 (incl. MP3-in-ASF)     | libmad + libasf       | decode + energy check |
| `vorbis`     | Ogg Vorbis                             | libtremor             | decode + energy check |
| `aac`        | AAC / HE-AAC (M4A)                     | libfaad + libm4a      | decode + energy check |
| `aac_bsf`    | raw AAC / ADTS streams                 | libfaad               | decode + energy check |
| `opus`       | Opus                                   | libopus (fixed point) | decode + energy check |
| `mpc`        | Musepack SV7 / SV8                     | libmusepack           | compiles + table      |
| `speex`      | Ogg Speex                              | libspeex              | compiles + table      |
| `wma`        | WMA v1/v2 in ASF                       | libwma + libasf       | decode + energy check |
| `wmapro`     | WMA Professional                       | libwmapro + libasf    | compiles + table      |
| `a52`        | AC3 / A-52                             | liba52                | decode + energy check |
| `a52_rm`     | AC3 in RealMedia                       | liba52 + librm        | compiles + table      |
| `cook`       | RealAudio Cook                         | libcook + librm       | compiles + table      |
| `raac`       | AAC in RealMedia                       | libfaad + librm       | compiles + table      |
| `atrac3_rm`  | ATRAC3 in RealMedia                    | libatrac + librm      | compiles + table      |
| `atrac3_oma` | ATRAC3 in Sony OMA                     | libatrac              | compiles + table      |
| `adx`        | CRI ADX (game audio)                   | built-in              | decode + energy check |
| `mod`        | Amiga MOD                              | built-in              | compiles + table      |

Not included: the libgme chiptune family (NSF, GBS, HES, KSS, AY, SGC,
VGM, VTX), SPC, SID and SAP — their emulator cores share symbols across
codecs and only work in Rockbox's dlopen-one-codec-at-a-time model, so
the upstream static builds exclude them too.

## API model

- [`Decoder::open`] parses metadata (via the sibling
  [`rockbox-metadata`](https://crates.io/crates/rockbox-metadata) crate,
  re-exported as `Metadata`), selects the codec by format, and starts a
  dedicated decode thread.
- [`Decoder::next_chunk`] pulls interleaved stereo `i16` PCM through a
  bounded channel (backpressure keeps memory flat). Mono sources are
  duplicated to stereo; high-depth codecs are converted from their
  native fraction-bit format.
- [`Decoder::seek`] / [`Decoder::elapsed`] map to the codec's native
  seek handling (sample-accurate where the codec supports it).
- `chunk.sample_rate` is the codec's output rate — Opus always decodes
  at 48 kHz, everything else at the file's native rate. Pair with
  [`rockbox-dsp`](https://crates.io/crates/rockbox-dsp) for resampling,
  EQ and ReplayGain.

**One session at a time**: Rockbox codecs share a single global
`codec_api` instance, so `Decoder::open` blocks until any other live
`Decoder` in the process is dropped.

## How it's built

- Each codec (wrapper + `codec_crt0.c` + its decoder library) compiles
  in its own `cc` build with the four entry symbols renamed per codec
  (`-D__header=__header_<name>`, …) — the same compile-time scheme the
  Rockbox WASM build uses, so no objcopy is needed on any platform.
- A static codec table maps formats to the renamed `__header_<name>`
  entry points (the pattern of Rockbox's Android static-codec loader).
- `codeclib` (shared helpers), `libm4a` (shared by alac + aac), `tlsf`
  (allocator for the ogg codecs) and `libasf` (MP3-in-ASF) compile once.
- libopus and libtremor both bundle libogg's `framing.c` with identical
  symbol names but different internals; the opus copy is prefix-renamed
  (`opus_ogg_*`) so both codecs can coexist in one binary.
- In-repo builds compile from the live `lib/rbcodec` tree; the published
  package builds from the `vendor/` snapshot (`./sync-vendor.sh`).

## Examples

```sh
# Player CLI: Rockbox decode → cpal output
cargo run --example play -- ~/Music/track.m4a
```

## Caveats

- **License**: the compiled C sources are Rockbox firmware code (and
  GPL-compatible decoder libraries), GPL-2.0-or-later overall — linking
  this crate makes the consuming binary GPL.
- Output is always interleaved stereo `i16` in this version; multichannel
  sources are downmixed/dropped per the codec's own handling.
- Gapless trims (encoder delay/padding) follow each codec's built-in
  handling; MP3 LAME-tag gapless works, but the very last Opus packet
  (flagged e-o-s) is skipped by the upstream wrapper.
- The upstream ASF/WMA code targets WMP-muxed files; ffmpeg-muxed WMA
  decodes only partially (and its header carries a preroll-inflated
  duration). WMP-produced files are the codec's tested corpus.
