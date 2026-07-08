# rockbox-metadata

[![crates.io](https://img.shields.io/crates/v/rockbox-metadata.svg)](https://crates.io/crates/rockbox-metadata)
[![docs.rs](https://img.shields.io/docsrs/rockbox-metadata)](https://docs.rs/rockbox-metadata)
[![license](https://img.shields.io/badge/license-GPL--2.0--or--later-blue.svg)](https://www.gnu.org/licenses/old-licenses/gpl-2.0.en.html)

Audio file metadata / tag parsing extracted from
[Rockbox](https://www.rockbox.org) as a reusable Rust library.

One call parses tags, duration, bitrate, sample rate, ReplayGain and
embedded album-art / cuesheet positions for 40+ audio formats, using the
battle-tested Rockbox C parsers (`lib/rbcodec/metadata/`) compiled into
the crate — no external dependencies, no subprocess, no tag-library
zoo.

```rust
let meta = rockbox_metadata::read("song.flac")?;

println!(
    "{} — {} [{}] {:?} @ {} Hz",
    meta.artist,
    meta.title,
    meta.codec,
    meta.duration,
    meta.sample_rate,
);

if let Some(db) = meta.replaygain.track_gain_db {
    println!("ReplayGain: {db:+.2} dB");
}
if let Some(art) = meta.album_art {
    // read `art.size` bytes at `art.offset` to extract the cover image
    println!("Cover: {:?} at {}+{}", art.kind, art.offset, art.size);
}
```

## Supported formats

| Family              | Formats                                                                       |
| ------------------- | ----------------------------------------------------------------------------- |
| Lossy               | MP1 / MP2 / MP3, Ogg Vorbis, Opus, AAC (M4A + ADTS), WMA, Musepack, Speex,     |
|                     | ATRAC3 / OMA, RealAudio (Cook), AC3 / A52                                      |
| Lossless            | FLAC, ALAC (M4A), WavPack, Monkey's Audio (APE), Shorten, TTA (True Audio)     |
| PCM containers      | WAV, AIFF, AU / SND, VOX, SMAF                                                 |
| Chiptune / tracker  | SPC, NSF / NSFE, SID, VGM, KSS, AY, GBS, HES, SGC, VTX, MOD, SAP + Atari       |
|                     | (CMC, CM3, CMR, CMS, DMC, DLT, MPT, MPD, RMT, TMC, TM8), ADX                   |

Tag containers: ID3v1, ID3v2.2/2.3/2.4 (including unsynchronization),
Vorbis comments, APE tags, MP4 atoms, ASF objects — whatever each format
uses, handled by its parser.

## What you get

| Field                                        | Notes                                                        |
| -------------------------------------------- | ------------------------------------------------------------ |
| `codec`, `codec_id`                          | Format label ("MP3", "FLAC", …) and stable Rockbox AFMT index |
| `title`, `artist`, `album`, `albumartist`    | Plain `String`s — empty when the tag is absent                |
| `composer`, `grouping`, `comment`, `genre`   |                                                              |
| `track_number`, `disc_number`, `year`        | Parsed numbers, plus the raw tag strings                      |
| `mb_track_id`                                | MusicBrainz track ID                                          |
| `duration`, `bitrate`, `sample_rate`, `vbr`  | Stream properties                                             |
| `filesize`, `samples`, `first_frame_offset`  | Layout info (leading-tag skip offset for gapless streaming)   |
| `replaygain`                                 | Track/album gain + peak, in dB and raw fixed point            |
| `album_art`                                  | Kind (JPEG/PNG/BMP) + byte offset/size of the embedded image  |
| `cuesheet`                                   | Byte offset/size/encoding of an embedded cuesheet             |

## ReplayGain and rockbox-dsp

The `ReplayGain` struct exposes both user-friendly values
(`track_gain_db: Option<f32>`, `track_peak: Option<f32>`) and Rockbox's
native fixed-point encoding:

| Raw field                          | Encoding                                        |
| ---------------------------------- | ------------------------------------------------ |
| `raw_track_gain`, `raw_album_gain` | Linear scale factor, Q7.24 (`1 << 24` = ×1.0)    |
| `raw_track_peak`, `raw_album_peak` | Linear peak amplitude, Q7.24 (0 = tag absent)    |

The raw values feed directly into the
[`rockbox-dsp`](https://crates.io/crates/rockbox-dsp) crate:

```rust
let meta = rockbox_metadata::read("song.flac")?;
dsp.set_replaygain_gains_raw(
    meta.replaygain.raw_track_gain,
    meta.replaygain.raw_album_gain,
    meta.replaygain.raw_track_peak,
    meta.replaygain.raw_album_peak,
);
```

## Examples

```sh
# Print full parsed metadata for any files
cargo run --example tags -- ~/Music/album/*.flac

# Minimal player: rockbox-metadata tags + symphonia decode + cpal output,
# with the parsed ReplayGain track gain applied
cargo run --example play -- ~/Music/album/track.m4a
```

## How it's built

- `build.rs` compiles the metadata parsers with the [`cc`] crate — from
  the live `lib/rbcodec/metadata/` tree when built inside the rockbox
  repo, or from the `vendor/` snapshot in the published package
  (re-sync with `./sync-vendor.sh` before publishing).
- `shim/` supplies stub headers that shadow the firmware includes
  (`config.h`, `debug.h`, `logf.h`, `rbunicode.h`, …) so no firmware
  tree is needed.
- `shim/rbmeta_shim.c` implements the handful of firmware symbols the
  parsers use — `filesize()`, BSD string helpers, and the rbunicode
  UTF-8/UTF-16/ISO-8859-1 conversion functions — plus the flat
  `rbmeta_read()` bridge the Rust wrapper calls.
- `get_metadata()` itself is exactly the code Rockbox runs on every
  track change on hundreds of DAP models.

## Caveats

- **License**: the compiled C sources are Rockbox firmware code,
  GPL-2.0-or-later — linking this crate makes the consuming binary GPL.
- **Thread safety**: some Rockbox parsers keep static scratch state, so
  the crate serializes all `read()` calls behind an internal mutex.
  Calls are cheap (one `open` + a few reads); contention is unlikely to
  matter.
- **Legacy codepages**: ID3v1 / ID3v2.3 text tagged in a legacy
  multi-byte codepage (SJIS, GB2312, …) decodes as ISO-8859-1 — the
  firmware behavior before a codepage table is loaded. UTF-8 and UTF-16
  tags (the overwhelming majority) decode exactly.
- `samples` is only filled by parsers whose container declares it
  (Vorbis, MP4, …); FLAC reports duration but not the raw sample count.

[`cc`]: https://crates.io/crates/cc
