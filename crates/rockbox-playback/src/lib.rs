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

pub use crossfade::{CrossfadeMode, CrossfadeSettings, MixMode};
pub use rockbox_codecs::Decoder;
pub use rockbox_metadata::Metadata;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
        }
    }
}

enum Command {
    SetQueue(Vec<PathBuf>),
    Enqueue(PathBuf),
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
        });

        let stream = build_stream(&device, rate, Arc::clone(&shared))?;
        stream.play().map_err(|e| Error::Stream(e.to_string()))?;

        let (tx, rx) = std::sync::mpsc::channel();
        let engine_shared = Arc::clone(&shared);
        let engine_cfg = EngineConfig {
            output_rate: rate,
            buffer_frames: (config.buffer_seconds.max(0.5) * rate as f32) as usize,
            crossfade: config.crossfade,
            replaygain: ReplayGainConfig {
                mode: config.replaygain_mode,
                preamp_db: config.replaygain_preamp_db,
                prevent_clipping: config.replaygain_prevent_clipping,
            },
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
        }
    }

    fn run(mut self) {
        loop {
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
            Command::Shutdown => return false,
            Command::SetQueue(q) => {
                self.queue = q;
                self.shared
                    .queue_len
                    .store(self.queue.len(), Ordering::Relaxed);
                self.index = 0;
                self.finishing = false;
                self.paused = false;
                self.reset_current();
            }
            Command::Enqueue(p) => {
                self.queue.push(p);
                self.shared
                    .queue_len
                    .store(self.queue.len(), Ordering::Relaxed);
            }
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
        }
    }

    // ---- queue / decoder helpers ----------------------------------------

    fn open_current(&mut self) -> bool {
        let Some(path) = self.queue.get(self.index).cloned() else {
            return false;
        };
        match Decoder::open(&path) {
            Ok(dec) => {
                self.input_rate = 0; // force resampler reconfigure on first chunk
                let meta = dec.metadata().clone();
                self.shared
                    .duration_ms
                    .store(meta.duration.as_millis() as u64, Ordering::Relaxed);
                self.shared.decode_pos_ms.store(0, Ordering::Relaxed);
                self.shared.index.store(self.index, Ordering::Relaxed);
                apply_replaygain_track(&mut self.dsp, &meta);
                *self.shared.meta.lock().unwrap() = Some(meta);
                self.decoder = Some(dec);
                true
            }
            Err(_) => false,
        }
    }

    fn reset_current(&mut self) {
        self.decoder = None;
        self.dsp.flush();
        self.shared.ring.lock().unwrap().clear();
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
