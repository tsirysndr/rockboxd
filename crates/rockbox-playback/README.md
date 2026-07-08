# rockbox-playback

[![crates.io](https://img.shields.io/crates/v/rockbox-playback.svg)](https://crates.io/crates/rockbox-playback)
[![docs.rs](https://img.shields.io/docsrs/rockbox-playback)](https://docs.rs/rockbox-playback)
[![license](https://img.shields.io/badge/license-GPL--2.0--or--later-blue.svg)](https://www.gnu.org/licenses/old-licenses/gpl-2.0.en.html)

A small audio playback engine built on [Rockbox](https://www.rockbox.org)'s
own building blocks: [`rockbox-codecs`](https://crates.io/crates/rockbox-codecs)
for decoding, [`rockbox-dsp`](https://crates.io/crates/rockbox-dsp) for
ReplayGain + resampling (and optional EQ), and
[`cpal`](https://crates.io/crates/cpal) for output.

It gives you a queue, transport controls, native **ReplayGain**, and a
faithful port of Rockbox's **crossfade** — in a few lines:

```rust
use rockbox_playback::{Player, CrossfadeSettings, ReplayGainMode};

let player = Player::new()?;
player.set_crossfade(CrossfadeSettings::always());        // 2 s crossfade
player.set_replaygain(ReplayGainMode::Track, 0.0, true);  // track gain, no clip
player.set_queue(vec!["a.flac", "b.mp3", "c.opus"]);
player.play();
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Features

- **Queue + transport**: `play` / `pause` / `toggle` / `stop`, `next` /
  `previous` / `skip_to`, `seek`, `set_volume`, `enqueue`.
- **Gapless by default** — the buffered tail of each track plays out before
  the next begins, so there's no click or silence at a boundary.
- **Crossfade**: a port of `apps/pcmbuf.c` (see below). Fade-in/out delays
  and durations, and the crossfade-vs-mix outgoing mode, all mirror
  Rockbox's own settings, defaults and units.
- **ReplayGain**: track or album mode with a preamp and optional
  clip-prevention, applied natively by `rockbox-dsp` from the file's tags.
- **Mixed-rate queues**: every track is resampled by the DSP to one output
  rate, so a FLAC-at-96 kHz and an Opus-at-48 kHz queue play back to back.
- **Click-free pause/resume**: the output callback applies Rockbox's ~⅓ s
  volume fade; pausing freezes the buffered audio rather than dropping it.

## Design

One background *engine* thread owns the single decode session (Rockbox
codecs share one global `codec_api`) and the single DSP instance. It
decodes ahead into a ring buffer that the cpal callback drains, so
transport controls stay responsive and pause/resume is click-free.

Because only one decoder can be open at a time, **crossfade** is done
without ever opening two decoders at once: the engine holds the outgoing
track's tail, opens and decodes the incoming track's head, mixes the two
overlapping regions, then continues with the incoming track as current.

`Player::status()` returns a snapshot — state, queue index, position
(corrected for the decode-ahead buffer so it tracks what you actually
hear), duration, current-track metadata and queue length.

## Crossfade fidelity

`crossfade.rs` reproduces Rockbox's mixer exactly:

- the fade gain is a Q16 factor (`MIXFADE_UNITY = 1 << 16`);
- each sample is scaled with `(factor * s + MIXFADE_UNITY/2) >> 16` and
  the mixed result is saturated to 16 bits;
- fade-in and fade-out are independent linear ramps driven by the same
  Bresenham stepper as `mixfader_init` / `mixfader_step`.

Settings field names, defaults, ranges and units mirror
`apps/settings_list.c` (fade delays 0–7 s, durations 0–15 s).

## Examples

```sh
# gapless
cargo run --release --example play -- a.flac b.mp3 c.opus

# 2 s crossfade + track ReplayGain
cargo run --release --example play -- --crossfade 2 --replaygain track a.flac b.mp3
```

## Caveats

- **License**: this crate links the Rockbox firmware codecs and DSP
  (GPL-2.0-or-later), so a consuming binary is GPL.
- **One player at a time**: the codec decode gate and DSP are process-wide
  singletons, so only one `Player` may exist per process.
- Output is interleaved stereo `i16`; multichannel sources follow each
  codec's own downmix handling.
- Needs an output device — `Player::new()` returns
  `Error::NoOutputDevice` on a headless machine.
