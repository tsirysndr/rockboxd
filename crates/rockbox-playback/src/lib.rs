//! A small audio playback engine built on Rockbox's own building blocks:
//! [`rockbox-codecs`](https://crates.io/crates/rockbox-codecs) for
//! decoding, [`rockbox-dsp`](https://crates.io/crates/rockbox-dsp) for
//! ReplayGain + resampling (and optional EQ), and
//! [`cpal`](https://crates.io/crates/cpal) for output.
//!
//! It provides a queue, transport controls, native **ReplayGain**, and a
//! faithful port of Rockbox's **crossfade** (see [`crossfade`]).
//!
//! ```no_run
//! use rockbox_playback::{Player, CrossfadeSettings};
//!
//! let player = Player::new()?;
//! player.set_crossfade(CrossfadeSettings::always());   // 2 s crossfade
//! player.set_replaygain(rockbox_playback::ReplayGainMode::Track, 0.0, true);
//! player.set_queue(vec!["a.flac", "b.mp3", "c.opus"]);
//! player.play();
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Design
//!
//! One background *engine* thread owns the single decode session (Rockbox
//! codecs share one global `codec_api`) and the single DSP instance. It
//! decodes ahead into a ring buffer that the cpal callback drains, so
//! transport controls stay responsive and pause/resume is click-free
//! (the callback applies Rockbox's ~⅓ s volume fade). Because only one
//! decoder can be open at a time, crossfade is done by holding the
//! outgoing track's tail, decoding the incoming track's head, and mixing
//! the two — never by opening two decoders at once.

mod crossfade;
pub mod m3u;
mod resume;
pub mod source;

pub use crossfade::{CrossfadeMode, CrossfadeSettings, MixMode};
pub use m3u::M3uEntry;
pub use resume::ResumeState;
pub use rockbox_codecs::Decoder;
pub use rockbox_metadata::Metadata;
#[cfg(feature = "http")]
pub use source::HttpSource;
pub use source::{is_url, FileSource, MediaSource};

/// Read a resume snapshot written by a previous session without constructing
/// a [`Player`] — e.g. to decide whether to offer "resume playback" in a UI.
/// See [`Player::resume`] to actually restore it.
pub fn load_resume(path: impl AsRef<std::path::Path>) -> Option<ResumeState> {
    resume::load(path.as_ref())
}

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// ReplayGain mode — which tag to apply (mirrors `rockbox-dsp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReplayGainMode {
    /// Off — no gain adjustment.
    #[default]
    Off,
    /// Per-track gain.
    Track,
    /// Per-album gain (keeps relative loudness within an album).
    Album,
}

#[derive(Debug, Clone, Copy)]
struct ReplayGainConfig {
    mode: ReplayGainMode,
    preamp_db: f32,
    prevent_clipping: bool,
}

impl Default for ReplayGainConfig {
    fn default() -> Self {
        ReplayGainConfig {
            mode: ReplayGainMode::Off,
            preamp_db: 0.0,
            prevent_clipping: true,
        }
    }
}

/// Whether the player is stopped, playing or paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Where to insert tracks into the queue, mirroring Rockbox's
/// `playlist_insert_track` position constants (`apps/playlist.h`).
///
/// Positions are resolved relative to the currently-playing track: the
/// engine shifts its play index as needed so the current track keeps
/// playing when tracks are inserted before or at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPosition {
    /// Add at the very beginning of the queue (`PLAYLIST_PREPEND`).
    Prepend,
    /// Add after the last inserted track, or immediately after the current
    /// track if none were inserted since (`PLAYLIST_INSERT`). Successive
    /// `Insert`s therefore keep their relative order, growing a block right
    /// after the current track.
    Insert,
    /// Add immediately after the current track — "play next"
    /// (`PLAYLIST_INSERT_FIRST`).
    InsertNext,
    /// Add to the end of the queue — "play last" (`PLAYLIST_INSERT_LAST`).
    InsertLast,
    /// Add at a random point between the current track and the end of the
    /// queue (`PLAYLIST_INSERT_SHUFFLED`). When stopped (not started), the
    /// random point spans the whole queue.
    InsertShuffled,
    /// Add at a random point within the region appended by this call,
    /// leaving all earlier tracks in place (`PLAYLIST_INSERT_LAST_SHUFFLED`).
    /// With a multi-track insert this shuffles the new tracks among
    /// themselves at the tail of the queue.
    InsertLastShuffled,
    /// Erase the queue and cue the new tracks from the start
    /// (`PLAYLIST_REPLACE`). If playing, playback continues into the first
    /// new track.
    Replace,
    /// Insert at an explicit index (clamped to the queue length).
    Index(usize),
}

const ST_STOPPED: u8 = 0;
const ST_PLAYING: u8 = 1;
const ST_PAUSED: u8 = 2;

/// A snapshot of the player's status.
#[derive(Debug, Clone)]
pub struct Status {
    pub state: PlaybackState,
    /// Index of the current track in the queue, if any.
    pub index: Option<usize>,
    /// Playback position within the current track (accounts for the
    /// decode-ahead buffer, so it tracks what you actually hear).
    pub position: Duration,
    /// Length of the current track.
    pub duration: Duration,
    /// Metadata of the current track, if one is loaded.
    pub metadata: Option<Metadata>,
    /// Number of tracks in the queue.
    pub queue_len: usize,
}

/// Configuration for [`Player::with_config`].
pub struct PlayerConfig {
    /// Output sample rate. `None` uses the output device's default; every
    /// track is resampled to this rate by the DSP so mixed-rate queues
    /// work.
    pub sample_rate: Option<u32>,
    /// Seconds of audio to decode ahead into the ring buffer.
    pub buffer_seconds: f32,
    pub crossfade: CrossfadeSettings,
    pub replaygain_mode: ReplayGainMode,
    pub replaygain_preamp_db: f32,
    pub replaygain_prevent_clipping: bool,
    /// Initial volume, 0.0..=1.0.
    pub volume: f32,
    /// When set, the queue and the exact playback position are auto-saved to
    /// this file (an extended `.m3u8`) as playback proceeds, and
    /// [`Player::resume`] restores them. `None` disables persistence.
    pub resume_file: Option<PathBuf>,
    /// How often the resume file is refreshed while playing (in addition to
    /// immediate saves on pause / stop / track change / shutdown). Only used
    /// when `resume_file` is set.
    pub resume_save_interval: Duration,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        PlayerConfig {
            sample_rate: None,
            buffer_seconds: 4.0,
            crossfade: CrossfadeSettings::default(),
            replaygain_mode: ReplayGainMode::Off,
            replaygain_preamp_db: 0.0,
            replaygain_prevent_clipping: true,
            volume: 1.0,
            resume_file: None,
            resume_save_interval: Duration::from_secs(5),
        }
    }
}

enum Command {
    SetQueue(Vec<PathBuf>),
    Enqueue(PathBuf),
    /// Insert one or more tracks at a Rockbox insertion position.
    Insert(Vec<PathBuf>, InsertPosition),
    /// Restore a persisted queue + exact position (does not auto-play).
    Resume(ResumeState),
    /// Force an immediate resume-file save.
    SaveResume,
    Play,
    Pause,
    Toggle,
    Stop,
    Next,
    Previous,
    SkipTo(usize),
    Seek(Duration),
    SetVolume(f32),
    SetCrossfade(CrossfadeSettings),
    SetReplayGain(ReplayGainConfig),
    Shutdown,
}

/// State shared between the public [`Player`] handle, the engine thread
/// and the cpal callback.
struct Shared {
    state: AtomicU8,
    /// Decode position of the current track in ms (ahead of playback by
    /// the ring buffer's fill).
    decode_pos_ms: AtomicU64,
    duration_ms: AtomicU64,
    index: AtomicUsize, // usize::MAX = none
    queue_len: AtomicUsize,
    /// Target output amplitude the callback ramps toward (f32 bits):
    /// `volume` when playing, 0 when paused/stopped.
    target_amp: AtomicU32,
    /// User volume 0.0..=1.0 (f32 bits).
    volume: AtomicU32,
    output_rate: AtomicU32,
    ring: Mutex<VecDeque<i16>>,
    meta: Mutex<Option<Metadata>>,
    /// Mirror of the engine's queue so the handle can read it (for
    /// `queue()` / `export_m3u`) without a round-trip to the engine thread.
    queue: Mutex<Vec<PathBuf>>,
}

impl Shared {
    fn set_volume_bits(&self, v: f32) {
        self.volume
            .store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
    fn volume_f32(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }
    fn ring_frames(&self) -> usize {
        self.ring.lock().unwrap().len() / 2
    }
}

/// Errors constructing a [`Player`].
#[derive(Debug)]
pub enum Error {
    /// No output audio device available.
    NoOutputDevice,
    /// cpal could not build or start the output stream.
    Stream(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoOutputDevice => write!(f, "no output audio device"),
            Error::Stream(e) => write!(f, "audio stream error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// The player handle. Cloneable-free but `Send` controls are issued
/// through it; the cpal stream lives here and keeps output alive for the
/// player's lifetime.
pub struct Player {
    tx: Sender<Command>,
    shared: Arc<Shared>,
    _stream: cpal::Stream,
    engine: Option<std::thread::JoinHandle<()>>,
    /// Resume file path (mirrors `PlayerConfig::resume_file`) so
    /// [`Player::resume`] can read it on the handle side.
    resume_file: Option<PathBuf>,
}

impl Player {
    /// Create a player on the default output device with default settings.
    pub fn new() -> Result<Self, Error> {
        Self::with_config(PlayerConfig::default())
    }

    /// Create a player with explicit configuration.
    pub fn with_config(config: PlayerConfig) -> Result<Self, Error> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(Error::NoOutputDevice)?;
        let default_cfg = device
            .default_output_config()
            .map_err(|e| Error::Stream(e.to_string()))?;

        let rate = config
            .sample_rate
            .unwrap_or_else(|| default_cfg.sample_rate().0);

        let shared = Arc::new(Shared {
            state: AtomicU8::new(ST_STOPPED),
            decode_pos_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            index: AtomicUsize::new(usize::MAX),
            queue_len: AtomicUsize::new(0),
            target_amp: AtomicU32::new(0f32.to_bits()),
            volume: AtomicU32::new(config.volume.clamp(0.0, 1.0).to_bits()),
            output_rate: AtomicU32::new(rate),
            ring: Mutex::new(VecDeque::new()),
            meta: Mutex::new(None),
            queue: Mutex::new(Vec::new()),
        });

        let stream = build_stream(&device, rate, Arc::clone(&shared))?;
        stream.play().map_err(|e| Error::Stream(e.to_string()))?;

        let (tx, rx) = std::sync::mpsc::channel();
        let engine_shared = Arc::clone(&shared);
        let resume_file = config.resume_file.clone();
        let engine_cfg = EngineConfig {
            output_rate: rate,
            buffer_frames: (config.buffer_seconds.max(0.5) * rate as f32) as usize,
            crossfade: config.crossfade,
            replaygain: ReplayGainConfig {
                mode: config.replaygain_mode,
                preamp_db: config.replaygain_preamp_db,
                prevent_clipping: config.replaygain_prevent_clipping,
            },
            resume_file: resume_file.clone(),
            resume_save_interval: config.resume_save_interval,
        };
        let engine = std::thread::Builder::new()
            .name("rbplayback".into())
            .spawn(move || Engine::new(engine_shared, rx, engine_cfg).run())
            .expect("spawn engine thread");

        Ok(Player {
            tx,
            shared,
            _stream: stream,
            engine: Some(engine),
            resume_file,
        })
    }

    /// The output sample rate everything is resampled to.
    pub fn sample_rate(&self) -> u32 {
        self.shared.output_rate.load(Ordering::Relaxed)
    }

    /// Replace the queue. Does not change playback state; call
    /// [`Player::play`] to start.
    pub fn set_queue<I, P>(&self, tracks: I)
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let v: Vec<PathBuf> = tracks.into_iter().map(Into::into).collect();
        let _ = self.tx.send(Command::SetQueue(v));
    }

    /// Append one track to the end of the queue.
    pub fn enqueue(&self, track: impl Into<PathBuf>) {
        let _ = self.tx.send(Command::Enqueue(track.into()));
    }

    /// Insert one track at a Rockbox insertion position (see
    /// [`InsertPosition`]). Does not change playback state.
    pub fn insert(&self, track: impl Into<PathBuf>, position: InsertPosition) {
        let _ = self.tx.send(Command::Insert(vec![track.into()], position));
    }

    /// Insert several tracks at a Rockbox insertion position, preserving
    /// their order (except for the shuffled positions). See
    /// [`InsertPosition`].
    pub fn insert_tracks<I, P>(&self, tracks: I, position: InsertPosition)
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        let v: Vec<PathBuf> = tracks.into_iter().map(Into::into).collect();
        let _ = self.tx.send(Command::Insert(v, position));
    }

    /// "Play next" — insert a track immediately after the current one
    /// ([`InsertPosition::InsertNext`]).
    pub fn insert_next(&self, track: impl Into<PathBuf>) {
        self.insert(track, InsertPosition::InsertNext);
    }

    /// "Play last" — append a track to the end of the queue
    /// ([`InsertPosition::InsertLast`]).
    pub fn insert_last(&self, track: impl Into<PathBuf>) {
        self.insert(track, InsertPosition::InsertLast);
    }

    /// Insert a track at a random point between the current track and the
    /// end ([`InsertPosition::InsertShuffled`]).
    pub fn insert_shuffled(&self, track: impl Into<PathBuf>) {
        self.insert(track, InsertPosition::InsertShuffled);
    }

    /// Insert a track at a random point in the tail region
    /// ([`InsertPosition::InsertLastShuffled`]).
    pub fn insert_last_shuffled(&self, track: impl Into<PathBuf>) {
        self.insert(track, InsertPosition::InsertLastShuffled);
    }

    /// Insert several tracks immediately after the current one, in order
    /// ([`InsertPosition::InsertNext`]).
    pub fn insert_tracks_next<I, P>(&self, tracks: I)
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.insert_tracks(tracks, InsertPosition::InsertNext);
    }

    /// Append several tracks to the end of the queue, in order
    /// ([`InsertPosition::InsertLast`]).
    pub fn insert_tracks_last<I, P>(&self, tracks: I)
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.insert_tracks(tracks, InsertPosition::InsertLast);
    }

    /// Insert several tracks at random points between the current track and
    /// the end ([`InsertPosition::InsertShuffled`]).
    pub fn insert_tracks_shuffled<I, P>(&self, tracks: I)
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.insert_tracks(tracks, InsertPosition::InsertShuffled);
    }

    /// Insert several tracks shuffled among themselves at the tail of the
    /// queue ([`InsertPosition::InsertLastShuffled`]).
    pub fn insert_tracks_last_shuffled<I, P>(&self, tracks: I)
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.insert_tracks(tracks, InsertPosition::InsertLastShuffled);
    }

    // ---- resume (auto-persist / restore) --------------------------------

    /// A snapshot of the current queue, in order.
    pub fn queue(&self) -> Vec<PathBuf> {
        self.shared.queue.lock().unwrap().clone()
    }

    /// Restore the queue and the exact position saved by a previous session
    /// (from `PlayerConfig::resume_file`). Returns the restored state, or
    /// `None` if resume is disabled or there is nothing to resume.
    ///
    /// Playback is **not** started — call [`Player::play`] to resume from the
    /// stored position, mirroring Rockbox's resume-on-startup.
    pub fn resume(&self) -> Option<ResumeState> {
        let path = self.resume_file.as_ref()?;
        let state = resume::load(path)?;
        let _ = self.tx.send(Command::Resume(state.clone()));
        Some(state)
    }

    /// Force an immediate write of the resume file (the engine also saves on
    /// pause / stop / track change / shutdown and periodically while
    /// playing). No-op when resume is disabled.
    pub fn save_resume(&self) {
        let _ = self.tx.send(Command::SaveResume);
    }

    /// Delete the resume file so the next launch starts fresh.
    pub fn clear_resume(&self) {
        if let Some(p) = &self.resume_file {
            resume::clear(p);
        }
    }

    // ---- m3u / m3u8 playlist files --------------------------------------

    /// Import an `.m3u` / `.m3u8` file into the queue at `position`
    /// (relative paths resolve against the file's directory). Returns the
    /// tracks that were read.
    pub fn import_m3u(
        &self,
        path: impl AsRef<std::path::Path>,
        position: InsertPosition,
    ) -> std::io::Result<Vec<PathBuf>> {
        let tracks = m3u::read_paths(path.as_ref())?;
        if !tracks.is_empty() || position == InsertPosition::Replace {
            let _ = self.tx.send(Command::Insert(tracks.clone(), position));
        }
        Ok(tracks)
    }

    /// Replace the queue with the contents of an `.m3u` / `.m3u8` file.
    /// Does not change playback state; call [`Player::play`] to start.
    pub fn load_m3u(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<Vec<PathBuf>> {
        let tracks = m3u::read_paths(path.as_ref())?;
        self.set_queue(tracks.clone());
        Ok(tracks)
    }

    /// Export the current queue to an `.m3u8` file (UTF-8, `#EXTM3U`
    /// header, one path per line), written atomically. Use the same path to
    /// **update** an existing playlist.
    pub fn export_m3u(&self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        m3u::write_paths(path.as_ref(), &self.queue())
    }

    pub fn play(&self) {
        let _ = self.tx.send(Command::Play);
    }
    pub fn pause(&self) {
        let _ = self.tx.send(Command::Pause);
    }
    /// Toggle play/pause.
    pub fn toggle(&self) {
        let _ = self.tx.send(Command::Toggle);
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Command::Stop);
    }
    /// Skip to the next track (honours the crossfade manual-skip mode).
    pub fn next(&self) {
        let _ = self.tx.send(Command::Next);
    }
    /// Skip to the previous track.
    pub fn previous(&self) {
        let _ = self.tx.send(Command::Previous);
    }
    /// Jump to a specific queue index.
    pub fn skip_to(&self, index: usize) {
        let _ = self.tx.send(Command::SkipTo(index));
    }
    /// Seek within the current track.
    pub fn seek(&self, pos: Duration) {
        let _ = self.tx.send(Command::Seek(pos));
    }
    /// Set output volume, 0.0..=1.0.
    pub fn set_volume(&self, vol: f32) {
        let _ = self.tx.send(Command::SetVolume(vol));
    }
    pub fn set_crossfade(&self, settings: CrossfadeSettings) {
        let _ = self.tx.send(Command::SetCrossfade(settings));
    }
    /// Configure ReplayGain: mode, preamp in dB, and whether to scale
    /// down to prevent clipping (using the track/album peak tag).
    pub fn set_replaygain(&self, mode: ReplayGainMode, preamp_db: f32, prevent_clipping: bool) {
        let _ = self.tx.send(Command::SetReplayGain(ReplayGainConfig {
            mode,
            preamp_db,
            prevent_clipping,
        }));
    }

    /// Current volume, 0.0..=1.0.
    pub fn volume(&self) -> f32 {
        self.shared.volume_f32()
    }

    /// A snapshot of the player's status.
    pub fn status(&self) -> Status {
        let state = match self.shared.state.load(Ordering::Relaxed) {
            ST_PLAYING => PlaybackState::Playing,
            ST_PAUSED => PlaybackState::Paused,
            _ => PlaybackState::Stopped,
        };
        let idx = self.shared.index.load(Ordering::Relaxed);
        let rate = self.shared.output_rate.load(Ordering::Relaxed).max(1);
        let decode_ms = self.shared.decode_pos_ms.load(Ordering::Relaxed);
        // Playback lags decoding by the ring buffer's fill.
        let ring_lag_ms = (self.shared.ring_frames() as u64 * 1000) / rate as u64;
        let position = Duration::from_millis(decode_ms.saturating_sub(ring_lag_ms));
        Status {
            state,
            index: (idx != usize::MAX).then_some(idx),
            position,
            duration: Duration::from_millis(self.shared.duration_ms.load(Ordering::Relaxed)),
            metadata: self.shared.meta.lock().unwrap().clone(),
            queue_len: self.shared.queue_len.load(Ordering::Relaxed),
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(e) = self.engine.take() {
            let _ = e.join();
        }
    }
}

/// Build the cpal output stream: drains the ring buffer, converts i16 →
/// f32, and applies a per-sample amplitude ramp toward `target_amp`
/// (~⅓ s full-range, matching Rockbox's pause/stop fade) for click-free
/// transitions.
fn build_stream(
    device: &cpal::Device,
    rate: u32,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, Error> {
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(rate),
        buffer_size: cpal::BufferSize::Default,
    };
    // ~1/3 second to fade the full 0..1 range, matching pcmbuf_fade_tick.
    let step = 3.0 / rate as f32;
    let mut cur_amp = 0.0f32;

    let err_fn = |e| eprintln!("rockbox-playback: output stream error: {e}");
    let stream = device
        .build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                let target = f32::from_bits(shared.target_amp.load(Ordering::Relaxed));
                // Fully muted for pause/stop: output silence WITHOUT
                // consuming the ring, so the buffered audio is frozen and
                // resume is click-free from where it left off.
                if target == 0.0 && cur_amp == 0.0 {
                    data.fill(0.0);
                    return;
                }
                let mut ring = shared.ring.lock().unwrap();
                for frame in data.chunks_mut(2) {
                    if cur_amp < target {
                        cur_amp = (cur_amp + step).min(target);
                    } else if cur_amp > target {
                        cur_amp = (cur_amp - step).max(target);
                    }
                    // Reached full mute mid-callback (fade-out complete):
                    // stop draining and freeze the rest.
                    if cur_amp == 0.0 && target == 0.0 {
                        frame.fill(0.0);
                        continue;
                    }
                    let l = ring.pop_front().unwrap_or(0);
                    let r = ring.pop_front().unwrap_or(0);
                    frame[0] = (l as f32 / 32768.0) * cur_amp;
                    if frame.len() > 1 {
                        frame[1] = (r as f32 / 32768.0) * cur_amp;
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| Error::Stream(e.to_string()))?;
    Ok(stream)
}

struct EngineConfig {
    output_rate: u32,
    buffer_frames: usize,
    crossfade: CrossfadeSettings,
    replaygain: ReplayGainConfig,
    resume_file: Option<PathBuf>,
    resume_save_interval: Duration,
}

struct Engine {
    shared: Arc<Shared>,
    rx: Receiver<Command>,
    cfg: EngineConfig,
    dsp: rockbox_dsp::Dsp,
    queue: Vec<PathBuf>,
    index: usize,
    /// Open decoder for the current track, if playing.
    decoder: Option<Decoder>,
    playing: bool,
    paused: bool,
    /// The queue is fully decoded; the engine is draining the ring buffer
    /// before going Stopped.
    finishing: bool,
    /// Native rate of the current decoder's output (0 = unknown); the DSP
    /// resampler is only reconfigured when this changes.
    input_rate: u32,
    /// Set for the duration of a manual-skip crossfade so [`Engine::next_index`]
    /// resolves to the skip target instead of `index + 1`.
    pending_manual_target: Option<usize>,
    /// Position of the last inserted track, so successive
    /// [`InsertPosition::Insert`]s append to the same block (mirrors
    /// Rockbox's `last_insert_pos`). `None` once invalidated.
    last_insert_pos: Option<usize>,
    /// xorshift64 state for the shuffled insertion positions.
    rng_state: u64,
    /// Set once a `Shutdown` command is seen so every loop — including the
    /// inner `push_frames` back-off, which may consume the command before the
    /// main loop does — unwinds instead of decoding forever.
    shutdown: bool,
    /// A `(index, position)` seek to apply the next time `index`'s decoder is
    /// opened — set by a resume restore so the track starts at the exact
    /// saved position. Discarded if the user navigates elsewhere first.
    pending_seek: Option<(usize, Duration)>,
    /// Last time the resume file was written (for interval throttling).
    last_save: Instant,
    /// Keeps the current track's HTTP cache (a temp file) alive for as long as
    /// its decoder is open. Dropped — and the temp file deleted — when the
    /// track is reset. Boxed as `Any` so the field needn't be `cfg`-gated.
    current_source: Option<Box<dyn std::any::Any + Send>>,
}

impl Engine {
    fn new(shared: Arc<Shared>, rx: Receiver<Command>, cfg: EngineConfig) -> Self {
        let mut dsp = rockbox_dsp::Dsp::new(cfg.output_rate);
        apply_replaygain_mode(&mut dsp, &cfg.replaygain);
        Engine {
            shared,
            rx,
            cfg,
            dsp,
            queue: Vec::new(),
            index: 0,
            decoder: None,
            playing: false,
            paused: false,
            finishing: false,
            input_rate: 0,
            pending_manual_target: None,
            last_insert_pos: None,
            rng_state: seed_rng(),
            shutdown: false,
            pending_seek: None,
            last_save: Instant::now(),
            current_source: None,
        }
    }

    fn run(mut self) {
        loop {
            // Break even if a Shutdown was consumed by an inner loop
            // (e.g. push_frames) rather than the pump below.
            if self.shutdown {
                break;
            }
            // Drain pending commands; returns false on Shutdown.
            if !self.pump_commands(false) {
                break;
            }

            // Paused: keep the decoder and buffered audio; idle until a
            // command changes things.
            if self.paused {
                self.set_state(ST_PAUSED);
                match self.rx.recv() {
                    Ok(cmd) => {
                        if !self.handle(cmd) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
                continue;
            }

            // Finishing: the queue is decoded; stay Playing until the ring
            // drains, then Stopped. (`position` keeps advancing because the
            // ring lag shrinks.)
            if self.finishing {
                if self.shared.ring_frames() > 0 {
                    self.set_state(ST_PLAYING);
                    std::thread::sleep(Duration::from_millis(25));
                } else {
                    self.finishing = false;
                    self.playing = false;
                    // Queue played to the end — don't resume a finished
                    // playlist next launch. Clear before signalling Stopped so
                    // an observer never sees Stopped with a stale file.
                    self.clear_resume();
                    self.set_state(ST_STOPPED);
                    self.shared.decode_pos_ms.store(0, Ordering::Relaxed);
                    self.shared.index.store(usize::MAX, Ordering::Relaxed);
                    self.shared
                        .target_amp
                        .store(0f32.to_bits(), Ordering::Relaxed);
                }
                continue;
            }

            if !self.playing || self.queue.is_empty() {
                self.set_state(ST_STOPPED);
                // Idle: block until a command arrives.
                match self.rx.recv() {
                    Ok(cmd) => {
                        if !self.handle(cmd) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
                continue;
            }

            // Ensure a decoder is open for the current track.
            if self.decoder.is_none() && !self.open_current() {
                // Failed to open — skip forward, or stop at end.
                if !self.advance_index(true) {
                    self.finish_queue();
                }
                continue;
            }

            self.set_state(ST_PLAYING);
            self.decode_step();

            // Refresh the resume file periodically so a crash loses at most
            // `resume_save_interval` of position.
            if self.cfg.resume_file.is_some()
                && self.last_save.elapsed() >= self.cfg.resume_save_interval
            {
                self.save_resume();
            }
        }
    }

    /// Process one decode step: pull a chunk, run DSP, and either push it
    /// or trigger a crossfade near the track's end.
    fn decode_step(&mut self) {
        // Near-end crossfade detection (auto skip).
        if self.should_start_crossfade() {
            self.crossfade_to_next(true);
            return;
        }

        let chunk = match self.next_output_chunk() {
            Some(c) => c,
            None => {
                // End of track — advance (auto). Ring keeps this track's
                // buffered tail so playback is gapless.
                self.dsp.flush();
                if !self.advance_index(true) {
                    self.finish_queue();
                }
                self.decoder = None;
                return;
            }
        };
        self.push_frames(&chunk);
    }

    /// Pull one decoded chunk and run it through the DSP (ReplayGain +
    /// resample to the output rate). Returns interleaved stereo i16 at
    /// `output_rate`, or `None` at end of track.
    fn next_output_chunk(&mut self) -> Option<Vec<i16>> {
        let dec = self.decoder.as_mut()?;
        let chunk = dec.next_chunk()?;
        // Engage the resampler only when the native rate actually changes.
        if chunk.sample_rate != self.input_rate {
            self.dsp.set_input_frequency(chunk.sample_rate);
            self.input_rate = chunk.sample_rate;
        }
        self.shared
            .decode_pos_ms
            .store(dec.elapsed().as_millis() as u64, Ordering::Relaxed);
        let mut out = Vec::new();
        self.dsp.process(&chunk.pcm, &mut out);
        Some(out)
    }

    fn should_start_crossfade(&self) -> bool {
        if !self.cfg.crossfade.mode.applies(true) {
            return false;
        }
        if self.next_index(true).is_none() {
            return false; // last track
        }
        let dur = self.shared.duration_ms.load(Ordering::Relaxed);
        if dur == 0 {
            return false; // unknown length — can't predict the tail
        }
        let region_ms = (self.cfg.crossfade.region_frames(self.cfg.output_rate) as u64 * 1000)
            / self.cfg.output_rate as u64;
        let pos = self.shared.decode_pos_ms.load(Ordering::Relaxed);
        pos + region_ms >= dur
    }

    /// Perform a crossfade into the next track: gather the outgoing tail,
    /// decode the incoming head, mix, and continue with the incoming
    /// track as current.
    fn crossfade_to_next(&mut self, auto: bool) {
        let region = self.cfg.crossfade.region_frames(self.cfg.output_rate);

        // 1. Outgoing tail (post-DSP). For auto skip we're near the end,
        //    so drain what's left; for a manual skip decode `region`
        //    frames from the current position.
        let mut tail: Vec<i16> = Vec::with_capacity(region * 2);
        while tail.len() < region * 2 {
            match self.next_output_chunk() {
                Some(c) => tail.extend_from_slice(&c),
                None => break, // track ended
            }
        }
        // If the outgoing track had more than the overlap (manual skip
        // mid-track), keep only its final `region` frames as the tail;
        // anything earlier plays normally first.
        if tail.len() > region * 2 {
            let split = tail.len() - region * 2;
            let head_part: Vec<i16> = tail.drain(..split).collect();
            self.push_frames(&head_part);
        }

        // 2. Switch to the incoming track.
        let next = match self.next_index(auto) {
            Some(i) => i,
            None => {
                self.push_frames(&tail);
                self.dsp.flush();
                self.decoder = None;
                self.finish_queue();
                return;
            }
        };
        self.dsp.flush();
        self.index = next;
        self.decoder = None;
        if !self.open_current() {
            self.push_frames(&tail);
            self.advance_index(auto);
            return;
        }

        // 3. Incoming head (post-DSP), `region` frames.
        let mut head: Vec<i16> = Vec::with_capacity(region * 2);
        while head.len() < region * 2 {
            match self.next_output_chunk() {
                Some(c) => head.extend_from_slice(&c),
                None => break,
            }
        }

        // 4. Mix the overlap and emit; the incoming track continues from
        //    where its head left off.
        let mixed = crossfade::mix(&tail, &head, &self.cfg.crossfade, self.cfg.output_rate);
        self.push_frames(&mixed);
    }

    /// Push frames to the ring, sleeping while it is full but staying
    /// responsive to commands.
    fn push_frames(&mut self, pcm: &[i16]) {
        let cap = self.cfg.buffer_frames * 2;
        let mut pos = 0;
        while pos < pcm.len() {
            // Back off if the ring is full, but keep handling commands. If
            // a command changes what we're doing, abandon the rest.
            loop {
                if !self.pump_commands(true) {
                    return; // shutdown
                }
                if !self.playing || self.decoder.is_none() {
                    return; // stopped / seeked / skipped — drop stale audio
                }
                let len = self.shared.ring.lock().unwrap().len();
                if len < cap {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            let mut ring = self.shared.ring.lock().unwrap();
            let space = cap.saturating_sub(ring.len());
            let end = (pos + space).min(pcm.len());
            ring.extend(&pcm[pos..end]);
            pos = end;
        }
    }

    // ---- command handling ------------------------------------------------

    /// Handle all queued commands. `nonblocking` = drain without waiting.
    /// Returns false if a Shutdown was received.
    fn pump_commands(&mut self, _nonblocking: bool) -> bool {
        while let Ok(cmd) = self.rx.try_recv() {
            if !self.handle(cmd) {
                return false;
            }
        }
        true
    }

    /// Returns false on Shutdown.
    fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Shutdown => {
                // Persist the exact position on exit, like Rockbox saves on
                // power-off — but only mid-session (a finished queue already
                // cleared its resume file).
                if self.playing || self.paused {
                    self.save_resume();
                }
                self.shutdown = true;
                return false;
            }
            Command::SetQueue(q) => {
                self.queue = q;
                self.index = 0;
                self.finishing = false;
                self.paused = false;
                self.last_insert_pos = None;
                self.pending_seek = None;
                self.reset_current();
                self.sync_queue();
            }
            Command::Enqueue(p) => {
                self.queue.push(p);
                self.sync_queue();
            }
            Command::Insert(tracks, position) => self.insert_tracks(tracks, position),
            Command::Resume(state) => {
                self.queue = state.tracks;
                self.index = state.index.min(self.queue.len().saturating_sub(1));
                self.pending_seek = Some((self.index, state.elapsed));
                self.finishing = false;
                self.paused = false;
                self.last_insert_pos = None;
                self.reset_current();
                self.sync_queue();
                // Reflect the restored index in status before play().
                self.shared.index.store(self.index, Ordering::Relaxed);
            }
            Command::SaveResume => self.save_resume(),
            Command::Play => {
                if self.queue.is_empty() {
                    return true;
                }
                // Resume from pause, or (re)start if stopped/finished.
                if self.paused {
                    self.set_paused(false);
                } else {
                    if self.finishing {
                        self.finishing = false;
                        self.index = 0;
                        self.reset_current();
                    }
                    self.playing = true;
                    self.shared
                        .target_amp
                        .store(self.shared.volume_f32().to_bits(), Ordering::Relaxed);
                }
            }
            Command::Pause => self.set_paused(true),
            Command::Toggle => self.set_paused(!self.paused),
            Command::Stop => self.stop_playback(),
            Command::Next => self.manual_skip(self.next_index(false)),
            Command::Previous => {
                let prev = if self.index == 0 {
                    None
                } else {
                    Some(self.index - 1)
                };
                self.manual_skip(prev.or(Some(0)));
            }
            Command::SkipTo(i) => {
                if i < self.queue.len() {
                    self.manual_skip(Some(i));
                }
            }
            Command::Seek(pos) => self.seek(pos),
            Command::SetVolume(v) => {
                self.shared.set_volume_bits(v);
                if self.playing && self.shared.state.load(Ordering::Relaxed) == ST_PLAYING {
                    self.shared
                        .target_amp
                        .store(self.shared.volume_f32().to_bits(), Ordering::Relaxed);
                }
            }
            Command::SetCrossfade(s) => self.cfg.crossfade = s,
            Command::SetReplayGain(rg) => {
                self.cfg.replaygain = rg;
                apply_replaygain_mode(&mut self.dsp, &rg);
                // Re-apply the current track's per-track gains under the
                // new mode.
                if let Some(meta) = self.shared.meta.lock().unwrap().as_ref() {
                    apply_replaygain_track(&mut self.dsp, meta);
                }
            }
        }
        true
    }

    fn set_paused(&mut self, paused: bool) {
        if paused {
            // Pause is valid while actively playing OR while draining the
            // ring after the queue finished (audio is still audible).
            if self.playing || self.finishing {
                self.paused = true;
                self.set_state(ST_PAUSED);
                self.shared
                    .target_amp
                    .store(0f32.to_bits(), Ordering::Relaxed);
                self.save_resume();
            }
        } else if self.paused || (!self.playing && !self.queue.is_empty()) {
            self.paused = false;
            if !self.finishing {
                self.playing = true;
            }
            self.set_state(ST_PLAYING);
            self.shared
                .target_amp
                .store(self.shared.volume_f32().to_bits(), Ordering::Relaxed);
        }
    }

    fn stop_playback(&mut self) {
        // Save the position before tearing down so a manual stop can still be
        // resumed (Rockbox keeps its last resume info on stop).
        if self.playing || self.paused || self.finishing {
            self.save_resume();
        }
        self.playing = false;
        self.paused = false;
        self.finishing = false;
        self.reset_current();
        self.set_state(ST_STOPPED);
        self.shared
            .target_amp
            .store(0f32.to_bits(), Ordering::Relaxed);
        self.shared.decode_pos_ms.store(0, Ordering::Relaxed);
        self.shared.index.store(usize::MAX, Ordering::Relaxed);
    }

    /// The whole queue has been decoded; keep the target amplitude up so
    /// the buffered tail plays out, and let [`Engine::run`] drain the ring
    /// before going Stopped.
    fn finish_queue(&mut self) {
        self.decoder = None;
        self.finishing = true;
        self.playing = true;
        self.shared
            .target_amp
            .store(self.shared.volume_f32().to_bits(), Ordering::Relaxed);
    }

    /// Manual (user-initiated) skip to `target`.
    fn manual_skip(&mut self, target: Option<usize>) {
        let Some(target) = target else {
            return;
        };
        if !self.playing {
            self.index = target;
            self.reset_current();
            return;
        }
        if self.cfg.crossfade.mode.applies(false) && self.decoder.is_some() {
            // Crossfade from the current position into `target`.
            // next_index(false) is `target`; align index bookkeeping.
            self.pending_manual_target = Some(target);
            self.crossfade_to_next(false);
            self.pending_manual_target = None;
        } else {
            // Immediate: drop the current decoder and clear buffered audio.
            self.index = target;
            self.reset_current();
            self.shared.ring.lock().unwrap().clear();
        }
    }

    fn seek(&mut self, pos: Duration) {
        if let Some(dec) = self.decoder.as_mut() {
            dec.seek(pos);
            self.dsp.flush();
            self.shared.ring.lock().unwrap().clear();
            self.shared
                .decode_pos_ms
                .store(pos.as_millis() as u64, Ordering::Relaxed);
            self.save_resume();
        }
    }

    // ---- queue / decoder helpers ----------------------------------------

    fn open_current(&mut self) -> bool {
        let Some(path) = self.queue.get(self.index).cloned() else {
            return false;
        };
        // Remote URLs are resolved to a seekable local cache first.
        let local = match self.resolve_source(&path) {
            Some(p) => p,
            None => return false,
        };
        match Decoder::open(&local) {
            Ok(mut dec) => {
                self.input_rate = 0; // force resampler reconfigure on first chunk
                let meta = dec.metadata().clone();
                self.shared
                    .duration_ms
                    .store(meta.duration.as_millis() as u64, Ordering::Relaxed);
                // A resume restore seeks this track to its exact saved
                // position; any other track (or a stale target) starts at 0.
                let mut start_ms = 0u64;
                if let Some((idx, pos)) = self.pending_seek.take() {
                    if idx == self.index && !pos.is_zero() {
                        dec.seek(pos);
                        start_ms = pos.as_millis() as u64;
                    }
                }
                self.shared.decode_pos_ms.store(start_ms, Ordering::Relaxed);
                self.shared.index.store(self.index, Ordering::Relaxed);
                apply_replaygain_track(&mut self.dsp, &meta);
                *self.shared.meta.lock().unwrap() = Some(meta);
                self.decoder = Some(dec);
                // Track change (or resume) — refresh the resume index.
                self.save_resume();
                true
            }
            Err(_) => false,
        }
    }

    fn reset_current(&mut self) {
        self.decoder = None;
        self.current_source = None; // drop any HTTP temp cache
        self.dsp.flush();
        self.shared.ring.lock().unwrap().clear();
    }

    /// Resolve a queue entry to a local path the codec can open. Local paths
    /// pass through; `http(s)://` URLs are fetched into a seekable temp cache
    /// (kept alive in `current_source`). Returns `None` if the URL can't be
    /// fetched or the `http` feature is disabled.
    fn resolve_source(&mut self, path: &Path) -> Option<PathBuf> {
        let s = path.to_string_lossy();
        if !source::is_url(&s) {
            self.current_source = None;
            return Some(path.to_path_buf());
        }
        #[cfg(feature = "http")]
        {
            match source::HttpSource::new(&s).and_then(|mut src| {
                src.ensure_complete()?;
                Ok(src)
            }) {
                Ok(src) => {
                    let local = src.cache_path().to_path_buf();
                    // Keep the temp file alive while its decoder is open.
                    self.current_source = Some(Box::new(src));
                    Some(local)
                }
                Err(_) => None,
            }
        }
        #[cfg(not(feature = "http"))]
        {
            None
        }
    }

    // ---- insertion (Rockbox playlist_insert_track semantics) ------------

    /// Insert `tracks` at the given [`InsertPosition`]. Delegates to the
    /// pure [`perform_insert`] model, then reconciles engine-side state
    /// (decoder/ring for `Replace`, the shared queue length).
    fn insert_tracks(&mut self, tracks: Vec<PathBuf>, position: InsertPosition) {
        let is_replace = position == InsertPosition::Replace;
        perform_insert(
            &mut self.queue,
            &mut self.index,
            &mut self.last_insert_pos,
            &mut self.rng_state,
            self.playing,
            tracks,
            position,
        );
        if is_replace {
            // Cue the new queue from the top; the run loop reopens index 0.
            self.finishing = false;
            self.pending_seek = None;
            self.reset_current();
        }
        self.sync_queue();
    }

    // ---- resume persistence ---------------------------------------------

    /// Mirror the queue (len + contents) into `Shared` for the handle.
    fn sync_queue(&self) {
        self.shared
            .queue_len
            .store(self.queue.len(), Ordering::Relaxed);
        *self.shared.queue.lock().unwrap() = self.queue.clone();
    }

    /// The playback position within the current track (decode position minus
    /// the ring-buffer lag) — what the listener has actually heard.
    fn playback_pos_ms(&self) -> u64 {
        let decode = self.shared.decode_pos_ms.load(Ordering::Relaxed);
        let rate = self.cfg.output_rate.max(1) as u64;
        let lag = (self.shared.ring_frames() as u64 * 1000) / rate;
        decode.saturating_sub(lag)
    }

    /// Write the resume file (queue + current index + exact position). No-op
    /// when resume is disabled or the queue is empty.
    fn save_resume(&mut self) {
        let Some(path) = self.cfg.resume_file.clone() else {
            return;
        };
        self.last_save = Instant::now();
        if self.queue.is_empty() {
            return;
        }
        let state = ResumeState {
            tracks: self.queue.clone(),
            index: self.index.min(self.queue.len() - 1),
            elapsed: Duration::from_millis(self.playback_pos_ms()),
        };
        let _ = resume::save(&path, &state);
    }

    /// Delete the resume file (queue finished — nothing to resume).
    fn clear_resume(&self) {
        if let Some(path) = &self.cfg.resume_file {
            resume::clear(path);
        }
    }

    /// The index the next auto/manual transition moves to, if any.
    fn next_index(&self, _auto: bool) -> Option<usize> {
        if let Some(t) = self.pending_manual_target {
            return Some(t);
        }
        let n = self.index + 1;
        (n < self.queue.len()).then_some(n)
    }

    /// Move to the next track for playback. Returns false at end of queue.
    fn advance_index(&mut self, auto: bool) -> bool {
        match self.next_index(auto) {
            Some(n) => {
                self.index = n;
                self.decoder = None;
                true
            }
            None => false,
        }
    }

    fn set_state(&self, s: u8) {
        self.shared.state.store(s, Ordering::Relaxed);
    }
}

/// Pure Rockbox `playlist_insert_track` model: mutate `queue` / `index` /
/// `last_insert_pos` in place for `position`. `started` means playback has
/// begun (so the current track under `index` must be kept playing).
///
/// Ordered positions ([`InsertPosition::Prepend`], `Insert`, `InsertNext`,
/// `InsertLast`, `Index`) place the tracks as a contiguous, order-preserving
/// block. The shuffled positions place each track at its own random point;
/// `Replace` erases the queue and cues the new tracks from the top.
///
/// Kept free of engine/audio state so it is unit-testable in isolation.
fn perform_insert(
    queue: &mut Vec<PathBuf>,
    index: &mut usize,
    last_insert_pos: &mut Option<usize>,
    rng: &mut u64,
    started: bool,
    tracks: Vec<PathBuf>,
    position: InsertPosition,
) {
    if tracks.is_empty() && position != InsertPosition::Replace {
        return;
    }

    match position {
        InsertPosition::Replace => {
            *queue = tracks;
            *index = 0;
            *last_insert_pos = None;
        }
        InsertPosition::InsertShuffled => {
            for t in tracks {
                let amount = queue.len();
                let at = if started && amount > 0 {
                    // Random point between the current track and the end.
                    let offset = rand_below(rng, amount - *index);
                    (*index + offset + 1).min(amount)
                } else {
                    rand_below(rng, amount + 1)
                };
                insert_one(queue, index, last_insert_pos, started, at, t, false);
            }
        }
        InsertPosition::InsertLastShuffled => {
            // Freeze the region start, then drop each new track at a random
            // point within the growing tail so the batch ends up shuffled
            // among itself while earlier tracks stay put.
            let start = queue.len();
            for t in tracks {
                let span = queue.len() - start + 1;
                let at = start + rand_below(rng, span);
                insert_one(queue, index, last_insert_pos, started, at, t, false);
            }
        }
        _ => {
            let (mut at, sets_last) = block_start(queue, *index, *last_insert_pos, position);
            for t in tracks {
                insert_one(queue, index, last_insert_pos, started, at, t, sets_last);
                at += 1;
            }
        }
    }
}

/// Insert one track at raw position `at`, keeping the currently-playing
/// track under `index` and shifting `last_insert_pos` if it moved.
fn insert_one(
    queue: &mut Vec<PathBuf>,
    index: &mut usize,
    last_insert_pos: &mut Option<usize>,
    started: bool,
    at: usize,
    path: PathBuf,
    sets_last: bool,
) {
    let at = at.min(queue.len());
    let pre_amount = queue.len();
    queue.insert(at, path);

    if started && pre_amount > 0 && at <= *index {
        *index += 1;
    }
    if let Some(lp) = *last_insert_pos {
        if at <= lp {
            *last_insert_pos = Some(lp + 1);
        }
    }
    if sets_last {
        *last_insert_pos = Some(at);
    }
}

/// Resolve the start index for an ordered-block insertion and whether it
/// updates `last_insert_pos`.
fn block_start(
    queue: &[PathBuf],
    index: usize,
    last_insert_pos: Option<usize>,
    position: InsertPosition,
) -> (usize, bool) {
    let amount = queue.len();
    match position {
        InsertPosition::Prepend => (0, false),
        InsertPosition::InsertNext => (if amount > 0 { index + 1 } else { 0 }, true),
        InsertPosition::InsertLast => (amount, true),
        InsertPosition::Insert => {
            let p = match last_insert_pos {
                Some(lp) if lp < amount => lp + 1,
                _ if amount > 0 => index + 1,
                _ => 0,
            };
            (p, true)
        }
        InsertPosition::Index(i) => (i.min(amount), false),
        // Shuffled / Replace are handled by perform_insert directly.
        _ => (amount, false),
    }
}

/// Uniform random in `0..n` via xorshift64 (`0` when `n == 0`).
fn rand_below(rng: &mut u64, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *rng = x;
    (x % n as u64) as usize
}

/// Seed the shuffled-insertion PRNG. xorshift64 must not start at 0, so a
/// non-zero fallback constant is used if the clock is unavailable.
fn seed_rng() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos | 0x9E37_79B9_7F4A_7C15
}

fn apply_replaygain_mode(dsp: &mut rockbox_dsp::Dsp, rg: &ReplayGainConfig) {
    let mode = match rg.mode {
        ReplayGainMode::Off => rockbox_dsp::REPLAYGAIN_OFF,
        ReplayGainMode::Track => rockbox_dsp::REPLAYGAIN_TRACK,
        ReplayGainMode::Album => rockbox_dsp::REPLAYGAIN_ALBUM,
    };
    dsp.set_replaygain(mode, rg.prevent_clipping, rg.preamp_db);
}

fn apply_replaygain_track(dsp: &mut rockbox_dsp::Dsp, meta: &Metadata) {
    let rg = &meta.replaygain;
    dsp.set_replaygain_gains_raw(
        rg.raw_track_gain,
        rg.raw_album_gain,
        rg.raw_track_peak,
        rg.raw_album_peak,
    );
}

#[cfg(test)]
mod insertion_tests {
    //! Unit tests for the pure Rockbox insertion model ([`perform_insert`]).
    //! These need no audio device — they exercise the queue/index bookkeeping
    //! directly.

    use super::*;

    /// Build a queue of single-letter paths.
    fn q(names: &str) -> Vec<PathBuf> {
        names
            .chars()
            .map(|c| PathBuf::from(c.to_string()))
            .collect()
    }

    /// Render a queue back to a compact string for assertions.
    fn s(queue: &[PathBuf]) -> String {
        queue
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// Run one insertion against a fresh state and return
    /// `(queue_string, index, last_insert_pos)`.
    fn run(
        start: &str,
        index: usize,
        last: Option<usize>,
        started: bool,
        tracks: &str,
        pos: InsertPosition,
    ) -> (String, usize, Option<usize>) {
        let mut queue = q(start);
        let mut idx = index;
        let mut lip = last;
        let mut rng = 0x1234_5678_9abc_def0u64;
        perform_insert(
            &mut queue,
            &mut idx,
            &mut lip,
            &mut rng,
            started,
            q(tracks),
            pos,
        );
        (s(&queue), idx, lip)
    }

    #[test]
    fn insert_next_after_current_shifts_nothing_before() {
        // Playing "B" (index 1) in A B C; InsertNext X → A B X C, index unchanged.
        let (queue, idx, lip) = run("ABC", 1, None, true, "X", InsertPosition::InsertNext);
        assert_eq!(queue, "ABXC");
        assert_eq!(idx, 1);
        assert_eq!(lip, Some(2));
    }

    #[test]
    fn insert_next_block_preserves_order() {
        let (queue, idx, _) = run("ABC", 0, None, true, "XY", InsertPosition::InsertNext);
        assert_eq!(queue, "AXYBC");
        assert_eq!(idx, 0);
    }

    #[test]
    fn insert_last_appends_in_order() {
        let (queue, idx, lip) = run("ABC", 1, None, true, "XY", InsertPosition::InsertLast);
        assert_eq!(queue, "ABCXY");
        assert_eq!(idx, 1);
        assert_eq!(lip, Some(4)); // last inserted position
    }

    #[test]
    fn prepend_before_current_shifts_index() {
        // Playing index 1; prepend at 0 pushes the current track to index 2.
        let (queue, idx, _) = run("ABC", 1, None, true, "X", InsertPosition::Prepend);
        assert_eq!(queue, "XABC");
        assert_eq!(idx, 2);
    }

    #[test]
    fn prepend_when_stopped_keeps_index() {
        // Not started: index does not chase the insertion.
        let (queue, idx, _) = run("ABC", 0, None, false, "X", InsertPosition::Prepend);
        assert_eq!(queue, "XABC");
        assert_eq!(idx, 0);
    }

    #[test]
    fn insert_chaining_grows_one_block_after_current() {
        // Two separate Insert calls should build a contiguous block right
        // after the current track, in call order.
        let mut queue = q("ABC");
        let mut idx = 0usize;
        let mut lip = None;
        let mut rng = 1u64;
        perform_insert(
            &mut queue,
            &mut idx,
            &mut lip,
            &mut rng,
            true,
            q("X"),
            InsertPosition::Insert,
        );
        perform_insert(
            &mut queue,
            &mut idx,
            &mut lip,
            &mut rng,
            true,
            q("Y"),
            InsertPosition::Insert,
        );
        assert_eq!(s(&queue), "AXYBC");
        assert_eq!(idx, 0);
    }

    #[test]
    fn explicit_index_inserts_and_clamps() {
        let (queue, _, _) = run("ABC", 0, None, true, "X", InsertPosition::Index(2));
        assert_eq!(queue, "ABXC");
        let (clamped, _, _) = run("ABC", 0, None, true, "X", InsertPosition::Index(99));
        assert_eq!(clamped, "ABCX");
    }

    #[test]
    fn replace_erases_and_cues_from_top() {
        let (queue, idx, lip) = run("ABC", 2, Some(1), true, "XYZ", InsertPosition::Replace);
        assert_eq!(queue, "XYZ");
        assert_eq!(idx, 0);
        assert_eq!(lip, None);
    }

    #[test]
    fn insert_shuffled_stays_after_current_and_keeps_it() {
        // Every random placement must land strictly after the current track
        // and leave the played prefix intact.
        for rng_seed in 1..200u64 {
            let mut queue = q("ABCDE");
            let mut idx = 2usize; // playing "C"
            let mut lip = None;
            let mut rng = rng_seed;
            perform_insert(
                &mut queue,
                &mut idx,
                &mut lip,
                &mut rng,
                true,
                q("X"),
                InsertPosition::InsertShuffled,
            );
            assert_eq!(queue.len(), 6);
            let xpos = queue.iter().position(|p| p == &PathBuf::from("X")).unwrap();
            assert!(xpos > idx, "X at {xpos} must be after current index {idx}");
            // Prefix up to and including the current track is untouched.
            assert_eq!(s(&queue[..=idx]), "ABC");
        }
    }

    #[test]
    fn insert_last_shuffled_keeps_prefix_and_adds_batch() {
        for rng_seed in 1..200u64 {
            let mut queue = q("ABC");
            let mut idx = 1usize;
            let mut lip = None;
            let mut rng = rng_seed;
            perform_insert(
                &mut queue,
                &mut idx,
                &mut lip,
                &mut rng,
                true,
                q("XYZ"),
                InsertPosition::InsertLastShuffled,
            );
            // Original tracks stay in their original order and positions.
            assert_eq!(s(&queue[..3]), "ABC");
            assert_eq!(queue.len(), 6);
            // The batch is entirely in the tail region.
            let tail = s(&queue[3..]);
            for c in ['X', 'Y', 'Z'] {
                assert!(
                    tail.contains(c),
                    "batch member {c} missing from tail {tail}"
                );
            }
        }
    }

    #[test]
    fn empty_insert_is_a_noop_except_replace() {
        let (queue, idx, _) = run("ABC", 1, None, true, "", InsertPosition::InsertLast);
        assert_eq!(queue, "ABC");
        assert_eq!(idx, 1);
        // Replace with no tracks clears the queue.
        let (empty, ridx, _) = run("ABC", 1, None, true, "", InsertPosition::Replace);
        assert_eq!(empty, "");
        assert_eq!(ridx, 0);
    }
}
