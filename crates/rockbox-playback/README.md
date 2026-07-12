# rockbox-playback

[![crates.io](https://img.shields.io/crates/v/rockbox-playback.svg)](https://crates.io/crates/rockbox-playback)
[![docs.rs](https://img.shields.io/docsrs/rockbox-playback)](https://docs.rs/rockbox-playback)
[![license](https://img.shields.io/badge/license-GPL--2.0--or--later-blue.svg)](https://www.gnu.org/licenses/old-licenses/gpl-2.0.en.html)

A small audio playback engine built on [Rockbox](https://www.rockbox.org)'s
own building blocks: [`rockbox-codecs`](https://crates.io/crates/rockbox-codecs)
for decoding, [`rockbox-dsp`](https://crates.io/crates/rockbox-dsp) for the
full DSP chain (ReplayGain, resampling, 10-band EQ, tone controls, surround,
channel mixing, compressor, dither and pitch), and
[`cpal`](https://crates.io/crates/cpal) for output.

It gives you a queue, transport controls, native **ReplayGain**, the complete
Rockbox **DSP** feature set, and a faithful port of Rockbox's **crossfade** —
in a few lines:

```rust
use rockbox_playback::{Player, CrossfadeSettings, ReplayGainMode, EqPreset};

let player = Player::new()?;
player.set_crossfade(CrossfadeSettings::always());        // 2 s crossfade
player.set_replaygain(ReplayGainMode::Track, 0.0, true);  // track gain, no clip
player.set_eq_preset(EqPreset::Rock);                     // ready-made EQ curve
player.set_queue(vec!["a.flac", "b.mp3", "c.opus"]);
player.play();
# Ok::<(), Box<dyn std::error::Error>>(())
```

Prefer to set everything up front? Use the fluent **config builder**:

```rust
use rockbox_playback::{PlayerConfig, RepeatMode, ReplayGainMode};

let player = PlayerConfig::builder()
    .volume(0.8)
    .replaygain(ReplayGainMode::Track, 0.0, true)
    .shuffle(true)
    .repeat(RepeatMode::All)
    .open()?;                       // build the config + construct the player
# Ok::<(), rockbox_playback::Error>(())
```

## Features

- **Queue + transport**: `play` / `pause` / `toggle` / `stop`, `next` /
  `previous` / `skip_to`, `seek`, `set_volume`, `enqueue`.
- **Shuffle + repeat**: `set_shuffle(bool)` plays the queue in a shuffled
  order (current track stays put, the rest are shuffled); `set_repeat` takes
  `RepeatMode::Off` / `One` (loop the current track) / `All` (loop the queue).
  Both are readable back via `shuffle()` / `repeat()` and the `Status`
  snapshot (see [Shuffle & repeat](#shuffle--repeat)).
- **Rockbox queue insertion**: the full `playlist_insert_track` position
  set — insert next, insert last, insert (grow a block after the current
  track), shuffled, last-shuffled, prepend, replace and insert-at-index —
  with single- and multi-track variants (see [Queue insertion](#queue-insertion)).
- **Resume, exactly like Rockbox**: the queue and the *exact* playback
  position auto-persist as you listen and restore on the next launch (see
  [Resume](#resume)).
- **`.m3u` / `.m3u8` playlists**: first-class import, export, insert and
  update in the same UTF-8 extended-M3U format Rockbox uses (see
  [M3U playlists](#m3u--m3u8-playlists)).
- **HTTP(S) remote media** (feature `http`, on by default): queue
  `http(s)://` URLs alongside local files. Seekable files are fetched with
  HTTP **range requests** into a local cache; **unbounded live streams**
  (internet radio) are decoded on the fly, never downloaded in full. The
  format is detected from the `Content-Type` header (see
  [Remote media](#remote-media-http)).
- **Gapless by default** — the buffered tail of each track plays out before
  the next begins, so there's no click or silence at a boundary.
- **Crossfade**: a port of `apps/pcmbuf.c` (see below). Fade-in/out delays
  and durations, and the crossfade-vs-mix outgoing mode, all mirror
  Rockbox's own settings, defaults and units.
- **ReplayGain**: track or album mode with a preamp and optional
  clip-prevention, applied natively by `rockbox-dsp` from the file's tags.
- **Full DSP chain**: the complete Rockbox pipeline past ReplayGain — a
  10-band parametric **equalizer** (with 21 ready-to-use presets), bass/treble
  **tone controls**, Haas **surround**, **channel mixing** (mono / karaoke /
  swap / custom stereo width), a dynamic-range **compressor**, output
  **dither** and **pitch**/speed. Configure it up front via
  `PlayerConfig.dsp` or live through `Player::set_*`, and read the current
  state back with `dsp_settings()` (see [DSP chain](#dsp-chain)).
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

## Queue insertion

Beyond `set_queue` / `enqueue`, the player exposes Rockbox's full
`playlist_insert_track` position model
([`apps/playlist.h`](../../apps/playlist.h)). All of these keep the
currently-playing track playing — the play cursor is shifted as needed
when tracks land before or at it — and none of them change playback
state (call `play()` to start):

```rust
use rockbox_playback::{Player, InsertPosition};

let player = Player::new()?;
player.set_queue(vec!["a.flac", "b.mp3"]);
player.play();

player.insert_next("cue.flac");        // right after the current track
player.insert_last("later.flac");      // end of the queue
player.insert_shuffled("random.flac"); // random point after the current track
player.insert_last_shuffled("x.flac"); // random point in the tail region

// Multi-track variants keep their order (except the shuffled ones):
player.insert_tracks_next(vec!["1.flac", "2.flac"]);
player.insert_tracks_last(vec!["3.flac", "4.flac"]);
player.insert_tracks_shuffled(vec!["5.flac", "6.flac"]);
player.insert_tracks_last_shuffled(vec!["7.flac", "8.flac"]);

// Or address a position explicitly:
player.insert("p.flac", InsertPosition::Prepend);
player.insert("i.flac", InsertPosition::Index(2));
player.insert_tracks(vec!["new.flac"], InsertPosition::Replace);
# Ok::<(), Box<dyn std::error::Error>>(())
```

| `InsertPosition`     | Rockbox constant                | Where the track lands                                              |
| -------------------- | ------------------------------- | ----------------------------------------------------------------- |
| `Prepend`            | `PLAYLIST_PREPEND`              | Very beginning of the queue                                       |
| `Insert`             | `PLAYLIST_INSERT`               | After the last inserted track, else after the current one         |
| `InsertNext`         | `PLAYLIST_INSERT_FIRST`         | Immediately after the current track ("play next")                 |
| `InsertLast`         | `PLAYLIST_INSERT_LAST`          | End of the queue ("play last")                                    |
| `InsertShuffled`     | `PLAYLIST_INSERT_SHUFFLED`      | Random point between the current track and the end                |
| `InsertLastShuffled` | `PLAYLIST_INSERT_LAST_SHUFFLED` | Random point in the region appended by the call (batch-shuffled)  |
| `Replace`            | `PLAYLIST_REPLACE`              | Erases the queue and cues the new tracks from the top             |
| `Index(i)`           | explicit position               | At index `i` (clamped to the queue length)                        |

`Insert` is stateful the way Rockbox's is: successive `Insert` calls grow
one contiguous block right after the current track, in call order. A
multi-track `insert_tracks(.., InsertLastShuffled)` shuffles the new
tracks among themselves at the tail while leaving every earlier track in
place.

## Shuffle & repeat

Shuffle and repeat are independent playback modes, each a live setter on the
`Player` (and an initial value in `PlayerConfig`, defaulting to off / `Off`):

```rust
use rockbox_playback::{Player, RepeatMode};

let player = Player::new()?;
player.set_queue(vec!["a.flac", "b.mp3", "c.opus", "d.ogg"]);

player.set_shuffle(true);          // play the queue in a shuffled order
player.set_repeat(RepeatMode::All); // loop the whole queue

player.play();

// Read the current modes back (also on the Status snapshot).
assert!(player.shuffle());
assert_eq!(player.repeat(), RepeatMode::All);
let st = player.status();
let _ = (st.shuffle, st.repeat);
# Ok::<(), Box<dyn std::error::Error>>(())
```

- **Shuffle** keeps the *current* track playing and shuffles the play order of
  the remaining tracks (Fisher-Yates); turning it off restores natural queue
  order from where you are. `next` / `previous` follow the shuffled order.
- **Repeat** — `RepeatMode::Off` stops at the end of the queue; `One` replays
  the current track on automatic advance (a manual `next` still moves on);
  `All` wraps from the last track back to the first (and from the first back
  to the last when stepping `previous`).

The two compose: shuffle + `RepeatMode::All` reshuffles-and-loops the queue.

## Resume

Like Rockbox, the player can pick up **exactly** where it left off across
runs. Set `resume_file` and the engine auto-saves the queue plus the
current track index and the exact playback position — on every track
change, pause, stop and shutdown, and periodically while playing (so a
crash loses at most `resume_save_interval`). When the queue plays to its
natural end the file is removed, so a finished playlist doesn't resume.

```rust
use rockbox_playback::{Player, PlayerConfig};
use std::time::Duration;

let cfg = PlayerConfig {
    resume_file: Some("~/.config/rockbox.org/resume.m3u8".into()),
    resume_save_interval: Duration::from_secs(5),
    ..Default::default()
};
let player = Player::with_config(cfg)?;

// On startup: restore the queue + exact position, then start playing.
if player.resume().is_some() {
    player.play();          // resumes mid-track at the saved millisecond
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

`resume()` restores state but does **not** auto-play (call `play()` to
resume, mirroring Rockbox's resume-on-startup). `load_resume(path)` peeks
at a saved snapshot without building a `Player` — e.g. to show a "Resume
playback" prompt. The resume file is a valid `.m3u8`: the track list is
plain, and the index/position ride along in `#RESUME-INDEX` /
`#RESUME-ELAPSED` header comments any other player ignores.

Position resolution mirrors `apps/playlist.c:playlist_update_resume_info`
(`resume_index` + `resume_elapsed`); the saved elapsed is the true
*playback* position, corrected for the decode-ahead buffer.

## DSP chain

Beyond ReplayGain, the player exposes Rockbox's entire DSP pipeline. Every
stage is a live setter on `Player` and also has an initial value in
`PlayerConfig.dsp` (a `DspSettings`, which defaults to a transparent
pipeline). Stages are process-wide DSP state that persists across track
changes — you set them once and they stay until changed.

```rust
use rockbox_playback::{
    Player, EqPreset, EqBand, ToneControls, Surround, ChannelMode, Compressor,
};

let player = Player::new()?;

// Equalizer — a ready-made preset, or drive the 10 bands yourself.
player.set_eq_preset(EqPreset::Jazz);          // enables the EQ + all bands
player.set_eq_enabled(true);                   // or toggle it directly
player.set_eq_band(5, EqBand { cutoff_hz: 1000, q: 1.0, gain_db: -3.0 });
player.set_eq_precut(6.0);                      // headroom against clipping

// Tone controls — set both, or one axis at a time.
player.set_tone(ToneControls { bass_db: 4, treble_db: 2, ..Default::default() });
player.set_bass(6);                            // leaves treble & cutoffs intact
player.set_treble(-2);

// Spatial / channel.
player.set_surround(Surround { delay_ms: 12, balance: 0, ..Default::default() });
player.set_channel_mode(ChannelMode::Karaoke);
player.set_stereo_width(120);                  // only with ChannelMode::Custom

// Dynamics, dither, pitch.
player.set_compressor(Compressor { threshold_db: -20, makeup_gain: 1, ratio: 2, ..Default::default() });
player.set_dither(true);
player.set_pitch(10500);                        // +5 % (PITCH_NORMAL = 10000)

// Read the whole current configuration back.
let dsp = player.dsp_settings();
println!("EQ on: {}", dsp.equalizer.enabled);
assert_eq!(player.is_eq_enabled(), dsp.equalizer.enabled);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`DspSettings` is the snapshot of the whole chain — pass it in via
`PlayerConfig.dsp`, or read it out with `dsp_settings()`:

| Field          | Setter(s)                                          | Notes                                              |
| -------------- | -------------------------------------------------- | -------------------------------------------------- |
| `equalizer`    | `set_eq_preset` / `set_eq_band` / `set_eq_enabled` | 10 bands; `is_eq_enabled()` reads the on/off state |
| `tone`         | `set_tone` / `set_bass` / `set_treble`             | bass/treble in dB, optional shelf cutoffs in Hz    |
| `surround`     | `set_surround`                                     | Haas delay (0 = off), balance, band-split cutoffs  |
| `channel_mode` | `set_channel_mode`                                 | stereo / mono / custom / mono-L/R / karaoke / swap |
| `stereo_width` | `set_stereo_width`                                 | percent; audible with `ChannelMode::Custom`        |
| `compressor`   | `set_compressor`                                   | `threshold_db = 0` disables the stage              |
| `dither`       | `set_dither`                                       | output dithering + noise shaping                   |
| `pitch`        | `set_pitch`                                        | `PITCH_NORMAL` (10000) = normal; pitch + tempo     |

### Equalizer presets

`EqPreset` bundles 21 ready-to-use curves over the standard octave-spaced
band frequencies (`EQ_BAND_FREQUENCIES`, 32 Hz … 16 kHz). `set_eq_preset`
enables the EQ and configures all ten bands in one call; `EqPreset::ALL`
lists them for a UI picker, and `EqPreset::equalizer()` gives you the
`Equalizer` to tweak before applying.

```rust
use rockbox_playback::EqPreset;

for p in EqPreset::ALL {
    println!("{}", p.name());   // Flat, Acoustic, Bass Boost, … Vocal Boost
}
let mut eq = EqPreset::Rock.equalizer();
eq.precut_db = 8.0;             // customise, then set_equalizer(eq)
```

Presets: Flat, Acoustic, Bass Boost, Bass Reducer, Classical, Dance, Deep,
Electronic, Hip-Hop, Jazz, Latin, Loudness, Lounge, Piano, Pop, R&B, Rock,
Small Speakers, Treble Boost, Treble Reducer, Vocal Boost.

## M3U / M3U8 playlists

Import, export, insert and update playlists in Rockbox's native
extended-M3U format (UTF-8 `.m3u8`). Relative paths resolve against the
playlist's directory and `#EXTINF` duration/title hints are parsed; writes
are atomic (temp + rename).

```rust
use rockbox_playback::{Player, InsertPosition, m3u};

let player = Player::new()?;

// Import — replace the queue, or insert at any position.
player.load_m3u("/music/Favourites.m3u8")?;                       // replace
player.import_m3u("/music/More.m3u8", InsertPosition::InsertLast)?; // append
player.import_m3u("/music/Next.m3u8", InsertPosition::InsertNext)?; // play next

// Export / update — write the live queue back out (same path = update).
player.export_m3u("/music/Now Playing.m3u8")?;

// The queue is also readable directly:
let paths = player.queue();

// Standalone parsing/writing without a Player:
let entries = m3u::read("/music/Favourites.m3u8")?;   // Vec<M3uEntry> w/ EXTINF
m3u::write_paths("/tmp/out.m3u8", &paths)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Remote media (HTTP)

With the default `http` feature, a queue entry can be an `http(s)://` URL —
everything else (transport, insertion, resume, m3u) treats it like any
other track:

```rust
use rockbox_playback::{Player, HttpSource, MediaSource};

let player = Player::new()?;
player.set_queue(vec![
    "/music/local.flac".to_string(),
    "https://example.com/song".to_string(),          // finite remote file
    "https://ec7.yesstreaming.net:1360/stream".to_string(), // live radio
]);
player.play();

// The source abstraction is also usable directly (range requests, seek):
use std::io::{Read, Seek, SeekFrom};
let mut src = HttpSource::new("https://example.com/song.flac")?;
let total = src.size();
src.seek(SeekFrom::Start(total / 2))?;
let mut buf = [0u8; 4096];
src.read_exact(&mut buf)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

A single probe classifies each URL:

- **Seekable finite file** (the server reports a length): playback starts
  as soon as the **header** is buffered — only ~512 KiB is fetched up front
  (via a range request) to read the format/rate/duration, then the codec
  reads and seeks through the file **on demand**, fetching only the ranges
  it needs. A big file is never downloaded in full just to start, and
  already-fetched ranges (plus read-ahead) are served from a local cache,
  so backward seeks and mid-file scrubbing are cheap.
- **Unbounded live stream** (no length — chunked / ICY internet radio): the
  response body is decoded **forward-only, on the fly**, never downloaded
  in full. A self-describing codec (MP3/Ogg/AAC/…) derives its format from
  the bitstream; there is no seeking, and playback continues until you skip
  or stop it.

In both cases the audio **format is detected from the `Content-Type`
response header** (e.g. `audio/flac` → FLAC), falling back to the URL's
extension — so an extension-less `/stream?id=42` still resolves a codec.

The [`MediaSource`] trait (`Read + Seek + size`) is the seam for finite
sources: [`FileSource`] for local files, [`HttpSource`] for remote ones;
live streams use [`HttpStream`] (`Read`-only). Disable the `http` feature
for a local-file-only build with no `reqwest` dependency.

### Live radio metadata (ICY)

For SHOUTcast/Icecast streams the player requests in-band metadata
(`Icy-MetaData: 1`) and de-interleaves the `StreamTitle` blocks out of the
audio, so `status().metadata` reflects the **current song as it changes** —
plus the station info from the `icy-name` / `icy-genre` / `icy-br` headers:

```rust
let st = player.status();
if let Some(m) = st.metadata {
    // "Artist - Title" from StreamTitle is split into artist/title.
    println!("{} — {}", m.artist, m.title);
    println!("station: {}  {} kbps  {} Hz", m.album, m.bitrate, m.sample_rate);
}
```

The mapping into [`Metadata`]: `StreamTitle` → `artist` / `title`
(split on `" - "`), `icy-name` → `album`, `icy-br` → `bitrate`, and the
decoded rate → `sample_rate`. The `play` example renders these live:

```sh
# live radio — prints the now-playing song, station, bitrate & sample rate
cargo run --release --example play -- https://ec7.yesstreaming.net:1360/stream
```

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
