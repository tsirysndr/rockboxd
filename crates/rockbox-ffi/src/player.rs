//! Flat C ABI over [`rockbox_playback::Player`].
//!
//! A player owns a background engine thread and a live cpal output stream,
//! so it only works where a real output device exists. The handle is an
//! opaque pointer; call `rb_player_free` to stop playback and join the
//! engine. Status is returned as a JSON C string.

use crate::meta::MetadataJson;
use crate::util::{cstr, into_cstring};
use rockbox_playback::{
    m3u, BassEnhancement, ChannelMode, Compressor, CrossfadeMode, CrossfadeSettings, Crossfeed,
    CrossfeedMode, DspSettings, EqBand, EqPreset, InsertPosition, MixMode, PlaybackState, Player,
    PlayerConfig, RepeatMode, ReplayGainMode, ResumeState, Status, Surround, ToneControls,
};
use serde::Serialize;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::time::Duration;

fn rg_mode(v: i32) -> ReplayGainMode {
    match v {
        1 => ReplayGainMode::Track,
        2 => ReplayGainMode::Album,
        _ => ReplayGainMode::Off,
    }
}

/// Decode an insertion position: 0 prepend, 1 insert, 2 insert-next,
/// 3 insert-last, 4 insert-shuffled, 5 insert-last-shuffled, 6 replace,
/// 7 explicit index (uses `index`). Anything else → insert-last.
fn insert_position(v: i32, index: usize) -> InsertPosition {
    match v {
        0 => InsertPosition::Prepend,
        1 => InsertPosition::Insert,
        2 => InsertPosition::InsertNext,
        3 => InsertPosition::InsertLast,
        4 => InsertPosition::InsertShuffled,
        5 => InsertPosition::InsertLastShuffled,
        6 => InsertPosition::Replace,
        7 => InsertPosition::Index(index),
        _ => InsertPosition::InsertLast,
    }
}

fn xfade_mode(v: i32) -> CrossfadeMode {
    match v {
        1 => CrossfadeMode::AutoSkip,
        2 => CrossfadeMode::ManualSkip,
        3 => CrossfadeMode::Shuffle,
        4 => CrossfadeMode::ShuffleOrManualSkip,
        5 => CrossfadeMode::Always,
        _ => CrossfadeMode::Off,
    }
}

fn mix_mode(v: i32) -> MixMode {
    match v {
        1 => MixMode::Mix,
        _ => MixMode::Crossfade,
    }
}

fn crossfade(
    mode: i32,
    fo_delay_ms: u32,
    fo_dur_ms: u32,
    fi_delay_ms: u32,
    fi_dur_ms: u32,
    mix: i32,
) -> CrossfadeSettings {
    CrossfadeSettings {
        mode: xfade_mode(mode),
        fade_out_delay: Duration::from_millis(fo_delay_ms as u64),
        fade_out_duration: Duration::from_millis(fo_dur_ms as u64),
        fade_in_delay: Duration::from_millis(fi_delay_ms as u64),
        fade_in_duration: Duration::from_millis(fi_dur_ms as u64),
        mix_mode: mix_mode(mix),
    }
}

/// Create a player on the default output device with default settings.
/// Returns null if no output device is available or the stream fails.
#[no_mangle]
pub extern "C" fn rb_player_new() -> *mut Player {
    match Player::new() {
        Ok(p) => Box::into_raw(Box::new(p)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Create a player with explicit configuration. `sample_rate` of 0 means
/// "use the device default". Enum arguments follow the same encoding as
/// the individual setters. Returns null on failure.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn rb_player_new_with_config(
    sample_rate: u32,
    buffer_seconds: f32,
    volume: f32,
    rg_mode_v: i32,
    rg_preamp_db: f32,
    rg_prevent_clipping: bool,
    xfade_mode_v: i32,
    fo_delay_ms: u32,
    fo_dur_ms: u32,
    fi_delay_ms: u32,
    fi_dur_ms: u32,
    mix_mode_v: i32,
) -> *mut Player {
    let cfg = PlayerConfig {
        sample_rate: (sample_rate != 0).then_some(sample_rate),
        buffer_seconds,
        crossfade: crossfade(
            xfade_mode_v,
            fo_delay_ms,
            fo_dur_ms,
            fi_delay_ms,
            fi_dur_ms,
            mix_mode_v,
        ),
        replaygain_mode: rg_mode(rg_mode_v),
        replaygain_preamp_db: rg_preamp_db,
        replaygain_prevent_clipping: rg_prevent_clipping,
        dsp: DspSettings::default(),
        shuffle: false,
        repeat: RepeatMode::Off,
        volume,
        resume_file: None,
        resume_save_interval: Duration::from_secs(5),
    };
    match Player::with_config(cfg) {
        Ok(p) => Box::into_raw(Box::new(p)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Like [`rb_player_new_with_config`] but also enables **resume**: the queue
/// and exact playback position are auto-persisted to `resume_file` (an
/// extended `.m3u8`) and restored via [`rb_player_resume`]. A null or empty
/// `resume_file` disables persistence; `resume_save_interval_ms` of 0 uses
/// the 5 s default. Returns null on failure.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn rb_player_new_with_config_ex(
    sample_rate: u32,
    buffer_seconds: f32,
    volume: f32,
    rg_mode_v: i32,
    rg_preamp_db: f32,
    rg_prevent_clipping: bool,
    xfade_mode_v: i32,
    fo_delay_ms: u32,
    fo_dur_ms: u32,
    fi_delay_ms: u32,
    fi_dur_ms: u32,
    mix_mode_v: i32,
    resume_file: *const c_char,
    resume_save_interval_ms: u32,
) -> *mut Player {
    let resume_file = cstr(resume_file)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let interval = if resume_save_interval_ms == 0 {
        Duration::from_secs(5)
    } else {
        Duration::from_millis(resume_save_interval_ms as u64)
    };
    let cfg = PlayerConfig {
        sample_rate: (sample_rate != 0).then_some(sample_rate),
        buffer_seconds,
        crossfade: crossfade(
            xfade_mode_v,
            fo_delay_ms,
            fo_dur_ms,
            fi_delay_ms,
            fi_dur_ms,
            mix_mode_v,
        ),
        replaygain_mode: rg_mode(rg_mode_v),
        replaygain_preamp_db: rg_preamp_db,
        replaygain_prevent_clipping: rg_prevent_clipping,
        dsp: DspSettings::default(),
        shuffle: false,
        repeat: RepeatMode::Off,
        volume,
        resume_file,
        resume_save_interval: interval,
    };
    match Player::with_config(cfg) {
        Ok(p) => Box::into_raw(Box::new(p)),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Stop playback, join the engine thread and destroy the handle. Null is
/// ignored.
#[no_mangle]
pub extern "C" fn rb_player_free(p: *mut Player) {
    if !p.is_null() {
        unsafe { drop(Box::from_raw(p)) };
    }
}

macro_rules! player {
    ($p:ident) => {{
        if $p.is_null() {
            return;
        }
        unsafe { &*$p }
    }};
}

/// Like [`player!`] but for functions that return a value: yields `$ret` on a
/// null handle.
macro_rules! player_or {
    ($p:ident, $ret:expr) => {{
        if $p.is_null() {
            return $ret;
        }
        unsafe { &*$p }
    }};
}

/// Replace the queue from a JSON array of strings, e.g.
/// `["a.flac","https://host/song.mp3"]`. Each entry may be a local file
/// path, an `http(s)://` URL to a finite remote file, or a live-radio /
/// streaming URL. Invalid JSON is ignored.
#[no_mangle]
pub extern "C" fn rb_player_set_queue_json(p: *mut Player, json: *const c_char) {
    let player = player!(p);
    let Some(json) = cstr(json) else { return };
    if let Ok(paths) = serde_json::from_str::<Vec<String>>(json) {
        player.set_queue(paths.into_iter().map(PathBuf::from).collect::<Vec<_>>());
    }
}

/// Append one track to the end of the queue. `path` may be a local file
/// path, an `http(s)://` URL to a finite remote file, or a live-radio /
/// streaming URL.
#[no_mangle]
pub extern "C" fn rb_player_enqueue(p: *mut Player, path: *const c_char) {
    let player = player!(p);
    if let Some(path) = cstr(path) {
        player.enqueue(PathBuf::from(path));
    }
}

/// Insert one or more tracks (JSON array of path/URL strings) into the queue
/// at a Rockbox insertion `position` (see [`insert_position`]); `index` is
/// only used when `position == 7` (explicit index). Local paths and
/// `http(s)://` URLs may be mixed. Invalid JSON is ignored.
#[no_mangle]
pub extern "C" fn rb_player_insert_json(
    p: *mut Player,
    json: *const c_char,
    position: i32,
    index: usize,
) {
    let player = player!(p);
    let Some(json) = cstr(json) else { return };
    if let Ok(paths) = serde_json::from_str::<Vec<String>>(json) {
        player.insert_tracks(
            paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
            insert_position(position, index),
        );
    }
}

/// The current queue as a JSON array of path/URL strings. Free with
/// `rb_string_free`; null on a null handle or a serialization error.
#[no_mangle]
pub extern "C" fn rb_player_queue_json(p: *mut Player) -> *mut c_char {
    let player = player_or!(p, std::ptr::null_mut());
    let paths: Vec<String> = player
        .queue()
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    match serde_json::to_string(&paths) {
        Ok(s) => into_cstring(s),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn rb_player_play(p: *mut Player) {
    player!(p).play();
}
#[no_mangle]
pub extern "C" fn rb_player_pause(p: *mut Player) {
    player!(p).pause();
}
#[no_mangle]
pub extern "C" fn rb_player_toggle(p: *mut Player) {
    player!(p).toggle();
}
#[no_mangle]
pub extern "C" fn rb_player_stop(p: *mut Player) {
    player!(p).stop();
}
#[no_mangle]
pub extern "C" fn rb_player_next(p: *mut Player) {
    player!(p).next();
}
#[no_mangle]
pub extern "C" fn rb_player_previous(p: *mut Player) {
    player!(p).previous();
}
#[no_mangle]
pub extern "C" fn rb_player_skip_to(p: *mut Player, index: usize) {
    player!(p).skip_to(index);
}
/// Seek within the current track to `ms` milliseconds.
#[no_mangle]
pub extern "C" fn rb_player_seek_ms(p: *mut Player, ms: u64) {
    player!(p).seek(Duration::from_millis(ms));
}
/// Set output volume, 0.0..=1.0.
#[no_mangle]
pub extern "C" fn rb_player_set_volume(p: *mut Player, vol: f32) {
    player!(p).set_volume(vol);
}
/// Set stereo balance, -100 (full left) ..= +100 (full right); 0 = centre.
#[no_mangle]
pub extern "C" fn rb_player_set_balance(p: *mut Player, balance: i32) {
    player!(p).set_balance(balance);
}
#[no_mangle]
pub extern "C" fn rb_player_set_crossfade(
    p: *mut Player,
    mode: i32,
    fo_delay_ms: u32,
    fo_dur_ms: u32,
    fi_delay_ms: u32,
    fi_dur_ms: u32,
    mix: i32,
) {
    player!(p).set_crossfade(crossfade(
        mode,
        fo_delay_ms,
        fo_dur_ms,
        fi_delay_ms,
        fi_dur_ms,
        mix,
    ));
}
/// Configure ReplayGain: mode (0 off, 1 track, 2 album), preamp in dB, and
/// whether to scale down to prevent clipping.
#[no_mangle]
pub extern "C" fn rb_player_set_replaygain(
    p: *mut Player,
    mode: i32,
    preamp_db: f32,
    prevent_clipping: bool,
) {
    player!(p).set_replaygain(rg_mode(mode), preamp_db, prevent_clipping);
}

fn repeat_mode(v: i32) -> RepeatMode {
    match v {
        1 => RepeatMode::One,
        2 => RepeatMode::All,
        _ => RepeatMode::Off,
    }
}

/// Enable or disable shuffle playback.
#[no_mangle]
pub extern "C" fn rb_player_set_shuffle(p: *mut Player, enabled: bool) {
    player!(p).set_shuffle(enabled);
}

/// Whether shuffle playback is currently enabled (false on a null handle).
#[no_mangle]
pub extern "C" fn rb_player_is_shuffle_enabled(p: *mut Player) -> bool {
    player_or!(p, false).shuffle()
}

/// Set the repeat mode: 0 off, 1 one (repeat current track), 2 all
/// (repeat the whole queue).
#[no_mangle]
pub extern "C" fn rb_player_set_repeat(p: *mut Player, mode: i32) {
    player!(p).set_repeat(repeat_mode(mode));
}

/// The current repeat mode: 0 off, 1 one, 2 all (0 on a null handle).
#[no_mangle]
pub extern "C" fn rb_player_repeat(p: *mut Player) -> i32 {
    match player_or!(p, 0).repeat() {
        RepeatMode::Off => 0,
        RepeatMode::One => 1,
        RepeatMode::All => 2,
    }
}

fn channel_mode(v: i32) -> ChannelMode {
    match v {
        1 => ChannelMode::Mono,
        2 => ChannelMode::Custom,
        3 => ChannelMode::MonoLeft,
        4 => ChannelMode::MonoRight,
        5 => ChannelMode::Karaoke,
        6 => ChannelMode::Swap,
        _ => ChannelMode::Stereo,
    }
}

/// Map a preset index (matching `EqPreset::ALL` order) to a preset;
/// out-of-range values fall back to `Flat`.
fn eq_preset(v: i32) -> EqPreset {
    EqPreset::ALL
        .get(v as usize)
        .copied()
        .unwrap_or(EqPreset::Flat)
}

/// Enable or disable the 10-band parametric equalizer.
#[no_mangle]
pub extern "C" fn rb_player_set_eq_enabled(p: *mut Player, enabled: bool) {
    player!(p).set_eq_enabled(enabled);
}

/// Whether the 10-band parametric equalizer is currently enabled
/// (false on a null handle).
#[no_mangle]
pub extern "C" fn rb_player_is_eq_enabled(p: *mut Player) -> bool {
    player_or!(p, false).is_eq_enabled()
}

/// Configure one EQ band: `band` in `0..EQ_BANDS` (10), `cutoff_hz` in Hz,
/// `q` a Q factor, `gain_db` in dB. Out-of-range bands are ignored.
#[no_mangle]
pub extern "C" fn rb_player_set_eq_band(
    p: *mut Player,
    band: usize,
    cutoff_hz: i32,
    q: f32,
    gain_db: f32,
) {
    player!(p).set_eq_band(
        band,
        EqBand {
            cutoff_hz,
            q,
            gain_db,
        },
    );
}

/// EQ pre-gain (headroom) in dB.
#[no_mangle]
pub extern "C" fn rb_player_set_eq_precut(p: *mut Player, db: f32) {
    player!(p).set_eq_precut(db);
}

/// Apply a built-in EQ preset by index (see `EqPreset::ALL`: 0 = Flat,
/// 1 = Acoustic, 2 = Bass Boost, …). Enables the EQ and sets all ten bands.
#[no_mangle]
pub extern "C" fn rb_player_set_eq_preset(p: *mut Player, preset: i32) {
    player!(p).set_eq_preset(eq_preset(preset));
}

/// Bass/treble tone controls (dB); cutoffs in Hz (0 = Rockbox defaults).
#[no_mangle]
pub extern "C" fn rb_player_set_tone(
    p: *mut Player,
    bass_db: i32,
    treble_db: i32,
    bass_cutoff_hz: i32,
    treble_cutoff_hz: i32,
) {
    player!(p).set_tone(ToneControls {
        bass_db,
        treble_db,
        bass_cutoff_hz,
        treble_cutoff_hz,
    });
}

/// Set the bass shelf gain in dB (treble and cutoffs unchanged; 0 = flat).
#[no_mangle]
pub extern "C" fn rb_player_set_bass(p: *mut Player, bass_db: i32) {
    player!(p).set_bass(bass_db);
}

/// Set the treble shelf gain in dB (bass and cutoffs unchanged; 0 = flat).
#[no_mangle]
pub extern "C" fn rb_player_set_treble(p: *mut Player, treble_db: i32) {
    player!(p).set_treble(treble_db);
}

/// Override the bass shelf cutoff in Hz (0 = default 200 Hz); gains and the
/// treble cutoff unchanged.
#[no_mangle]
pub extern "C" fn rb_player_set_bass_cutoff(p: *mut Player, hz: i32) {
    player!(p).set_bass_cutoff(hz);
}

/// Override the treble shelf cutoff in Hz (0 = default 3.5 kHz); gains and the
/// bass cutoff unchanged.
#[no_mangle]
pub extern "C" fn rb_player_set_treble_cutoff(p: *mut Player, hz: i32) {
    player!(p).set_treble_cutoff(hz);
}

fn crossfeed_mode(v: i32) -> CrossfeedMode {
    match v {
        1 => CrossfeedMode::Meier,
        2 => CrossfeedMode::Custom,
        _ => CrossfeedMode::Off,
    }
}

/// Headphone crossfeed. `mode`: 0 off, 1 Meier, 2 custom. `direct_gain`,
/// `cross_gain`, `hf_gain` are in tenths of a dB (≤ 0); `hf_cutoff` in Hz.
/// The `cross_*`/`hf_*` params only apply in custom mode.
#[no_mangle]
pub extern "C" fn rb_player_set_crossfeed(
    p: *mut Player,
    mode: i32,
    direct_gain: i32,
    cross_gain: i32,
    hf_gain: i32,
    hf_cutoff: i32,
) {
    player!(p).set_crossfeed(Crossfeed {
        mode: crossfeed_mode(mode),
        direct_gain,
        cross_gain,
        high_freq_gain: hf_gain,
        high_freq_cutoff: hf_cutoff,
    });
}

/// Perceptual Bass Enhancement: `strength` percent (0 = off … 100),
/// `precut` headroom in tenths of a dB (≤ 0).
#[no_mangle]
pub extern "C" fn rb_player_set_bass_enhancement(p: *mut Player, strength: i32, precut: i32) {
    player!(p).set_bass_enhancement(BassEnhancement { strength, precut });
}

/// Auditory Fatigue Reduction: 0 off, 1 weak, 2 moderate, 3 strong.
#[no_mangle]
pub extern "C" fn rb_player_set_fatigue_reduction(p: *mut Player, strength: i32) {
    player!(p).set_fatigue_reduction(strength);
}

/// Haas surround: `delay_ms` (0 = off), `balance` in percent, band-split
/// cutoffs in Hz (0/0 = defaults).
#[no_mangle]
pub extern "C" fn rb_player_set_surround(
    p: *mut Player,
    delay_ms: i32,
    balance: i32,
    cutoff_low_hz: i32,
    cutoff_high_hz: i32,
) {
    player!(p).set_surround(Surround {
        delay_ms,
        balance,
        cutoff_low_hz,
        cutoff_high_hz,
    });
}

/// Channel-mixing mode: 0 stereo, 1 mono, 2 custom, 3 mono-left,
/// 4 mono-right, 5 karaoke, 6 swap.
#[no_mangle]
pub extern "C" fn rb_player_set_channel_mode(p: *mut Player, mode: i32) {
    player!(p).set_channel_mode(channel_mode(mode));
}

/// Custom stereo width in percent (only audible with channel mode 2).
#[no_mangle]
pub extern "C" fn rb_player_set_stereo_width(p: *mut Player, percent: i32) {
    player!(p).set_stereo_width(percent);
}

/// Dynamic-range compressor (`threshold_db` of 0 disables it). `ratio` and
/// `knee` are indices; times in ms. See `rockbox_playback::Compressor`.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn rb_player_set_compressor(
    p: *mut Player,
    threshold_db: i32,
    makeup_gain: i32,
    ratio: i32,
    knee: i32,
    attack_ms: i32,
    release_ms: i32,
) {
    player!(p).set_compressor(Compressor {
        threshold_db,
        makeup_gain,
        ratio,
        knee,
        attack_ms,
        release_ms,
    });
}

/// Enable output dithering + noise shaping.
#[no_mangle]
pub extern "C" fn rb_player_set_dither(p: *mut Player, enabled: bool) {
    player!(p).set_dither(enabled);
}

/// Pitch/speed ratio (10000 = normal); pitch and tempo shift together.
#[no_mangle]
pub extern "C" fn rb_player_set_pitch(p: *mut Player, ratio: i32) {
    player!(p).set_pitch(ratio);
}

#[derive(Serialize)]
struct EqBandJson {
    cutoff_hz: i32,
    q: f32,
    gain_db: f32,
}

#[derive(Serialize)]
struct EqualizerJson {
    enabled: bool,
    precut_db: f32,
    bands: Vec<EqBandJson>,
}

#[derive(Serialize)]
struct ToneJson {
    bass_db: i32,
    treble_db: i32,
    bass_cutoff_hz: i32,
    treble_cutoff_hz: i32,
}

#[derive(Serialize)]
struct SurroundJson {
    delay_ms: i32,
    balance: i32,
    cutoff_low_hz: i32,
    cutoff_high_hz: i32,
}

#[derive(Serialize)]
struct CompressorJson {
    threshold_db: i32,
    makeup_gain: i32,
    ratio: i32,
    knee: i32,
    attack_ms: i32,
    release_ms: i32,
}

#[derive(Serialize)]
struct CrossfeedJson {
    /// "off" | "meier" | "custom"
    mode: &'static str,
    direct_gain: i32,
    cross_gain: i32,
    high_freq_gain: i32,
    high_freq_cutoff: i32,
}

#[derive(Serialize)]
struct BassEnhancementJson {
    strength: i32,
    precut: i32,
}

#[derive(Serialize)]
struct DspSettingsJson {
    equalizer: EqualizerJson,
    tone: ToneJson,
    crossfeed: CrossfeedJson,
    surround: SurroundJson,
    channel_mode: &'static str,
    stereo_width: i32,
    bass_enhancement: BassEnhancementJson,
    fatigue_reduction: i32,
    compressor: CompressorJson,
    dither: bool,
    pitch: i32,
}

fn crossfeed_mode_name(mode: CrossfeedMode) -> &'static str {
    match mode {
        CrossfeedMode::Off => "off",
        CrossfeedMode::Meier => "meier",
        CrossfeedMode::Custom => "custom",
    }
}

fn channel_mode_name(mode: ChannelMode) -> &'static str {
    match mode {
        ChannelMode::Stereo => "stereo",
        ChannelMode::Mono => "mono",
        ChannelMode::Custom => "custom",
        ChannelMode::MonoLeft => "mono_left",
        ChannelMode::MonoRight => "mono_right",
        ChannelMode::Karaoke => "karaoke",
        ChannelMode::Swap => "swap",
    }
}

impl From<&rockbox_playback::DspSettings> for DspSettingsJson {
    fn from(s: &rockbox_playback::DspSettings) -> Self {
        DspSettingsJson {
            equalizer: EqualizerJson {
                enabled: s.equalizer.enabled,
                precut_db: s.equalizer.precut_db,
                bands: s
                    .equalizer
                    .bands
                    .iter()
                    .map(|b| EqBandJson {
                        cutoff_hz: b.cutoff_hz,
                        q: b.q,
                        gain_db: b.gain_db,
                    })
                    .collect(),
            },
            tone: ToneJson {
                bass_db: s.tone.bass_db,
                treble_db: s.tone.treble_db,
                bass_cutoff_hz: s.tone.bass_cutoff_hz,
                treble_cutoff_hz: s.tone.treble_cutoff_hz,
            },
            crossfeed: CrossfeedJson {
                mode: crossfeed_mode_name(s.crossfeed.mode),
                direct_gain: s.crossfeed.direct_gain,
                cross_gain: s.crossfeed.cross_gain,
                high_freq_gain: s.crossfeed.high_freq_gain,
                high_freq_cutoff: s.crossfeed.high_freq_cutoff,
            },
            surround: SurroundJson {
                delay_ms: s.surround.delay_ms,
                balance: s.surround.balance,
                cutoff_low_hz: s.surround.cutoff_low_hz,
                cutoff_high_hz: s.surround.cutoff_high_hz,
            },
            channel_mode: channel_mode_name(s.channel_mode),
            stereo_width: s.stereo_width,
            bass_enhancement: BassEnhancementJson {
                strength: s.bass_enhancement.strength,
                precut: s.bass_enhancement.precut,
            },
            fatigue_reduction: s.fatigue_reduction,
            compressor: CompressorJson {
                threshold_db: s.compressor.threshold_db,
                makeup_gain: s.compressor.makeup_gain,
                ratio: s.compressor.ratio,
                knee: s.compressor.knee,
                attack_ms: s.compressor.attack_ms,
                release_ms: s.compressor.release_ms,
            },
            dither: s.dither,
            pitch: s.pitch,
        }
    }
}

/// A JSON snapshot of the full DSP-chain configuration (EQ, tone, surround,
/// channel mixing, stereo width, compressor, dither, pitch). Free with
/// `rb_string_free`. Returns null on a null handle.
#[no_mangle]
pub extern "C" fn rb_player_dsp_settings_json(p: *mut Player) -> *mut c_char {
    if p.is_null() {
        return std::ptr::null_mut();
    }
    let settings = unsafe { &*p }.dsp_settings();
    match serde_json::to_string(&DspSettingsJson::from(&settings)) {
        Ok(s) => into_cstring(s),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Current volume, 0.0..=1.0 (0.0 on a null handle).
#[no_mangle]
pub extern "C" fn rb_player_volume(p: *mut Player) -> f32 {
    if p.is_null() {
        return 0.0;
    }
    unsafe { &*p }.volume()
}

/// Current stereo balance, -100 (full left) ..= +100 (full right); 0 on a
/// null handle.
#[no_mangle]
pub extern "C" fn rb_player_balance(p: *mut Player) -> i32 {
    if p.is_null() {
        return 0;
    }
    unsafe { &*p }.balance()
}

/// The output sample rate everything is resampled to (0 on a null handle).
#[no_mangle]
pub extern "C" fn rb_player_sample_rate(p: *mut Player) -> u32 {
    if p.is_null() {
        return 0;
    }
    unsafe { &*p }.sample_rate()
}

fn repeat_name(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "off",
        RepeatMode::One => "one",
        RepeatMode::All => "all",
    }
}

#[derive(Serialize)]
struct StatusJson {
    /// "stopped" | "playing" | "paused"
    state: &'static str,
    index: Option<usize>,
    position_ms: u64,
    duration_ms: u64,
    queue_len: usize,
    shuffle: bool,
    /// "off" | "one" | "all"
    repeat: &'static str,
    metadata: Option<MetadataJson>,
}

impl From<&Status> for StatusJson {
    fn from(s: &Status) -> Self {
        StatusJson {
            state: match s.state {
                PlaybackState::Stopped => "stopped",
                PlaybackState::Playing => "playing",
                PlaybackState::Paused => "paused",
            },
            index: s.index,
            position_ms: s.position.as_millis() as u64,
            duration_ms: s.duration.as_millis() as u64,
            queue_len: s.queue_len,
            shuffle: s.shuffle,
            repeat: repeat_name(s.repeat),
            metadata: s.metadata.as_ref().map(Into::into),
        }
    }
}

/// A JSON snapshot of the player's status. Free with `rb_string_free`.
/// Returns null on a null handle.
#[no_mangle]
pub extern "C" fn rb_player_status_json(p: *mut Player) -> *mut c_char {
    if p.is_null() {
        return std::ptr::null_mut();
    }
    let status = unsafe { &*p }.status();
    match serde_json::to_string(&StatusJson::from(&status)) {
        Ok(s) => into_cstring(s),
        Err(_) => std::ptr::null_mut(),
    }
}

// ---- resume (auto-persist / restore) ------------------------------------

#[derive(Serialize)]
struct ResumeStateJson {
    tracks: Vec<String>,
    index: usize,
    elapsed_ms: u64,
}

impl From<&ResumeState> for ResumeStateJson {
    fn from(s: &ResumeState) -> Self {
        ResumeStateJson {
            tracks: s
                .tracks
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            index: s.index,
            elapsed_ms: s.elapsed.as_millis() as u64,
        }
    }
}

/// Restore the queue and exact position saved by a previous session (from the
/// `resume_file` given to `rb_player_new_with_config_ex`). Does NOT start
/// playback — call `rb_player_play` to resume from the stored position.
/// Returns the restored state as JSON (free with `rb_string_free`), or null if
/// resume is disabled or there's nothing to resume.
#[no_mangle]
pub extern "C" fn rb_player_resume(p: *mut Player) -> *mut c_char {
    let player = player_or!(p, std::ptr::null_mut());
    match player.resume() {
        Some(state) => match serde_json::to_string(&ResumeStateJson::from(&state)) {
            Ok(s) => into_cstring(s),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

/// Force an immediate write of the resume file (no-op when resume is disabled).
#[no_mangle]
pub extern "C" fn rb_player_save_resume(p: *mut Player) {
    player!(p).save_resume();
}

/// Delete the resume file so the next launch starts fresh.
#[no_mangle]
pub extern "C" fn rb_player_clear_resume(p: *mut Player) {
    player!(p).clear_resume();
}

/// Peek at a resume snapshot on disk without constructing a player (e.g. to
/// offer "resume playback" in a UI). Returns JSON (free with `rb_string_free`)
/// or null if the file is missing / not a resume file / empty.
#[no_mangle]
pub extern "C" fn rb_load_resume_json(path: *const c_char) -> *mut c_char {
    let Some(path) = cstr(path) else {
        return std::ptr::null_mut();
    };
    match rockbox_playback::load_resume(path) {
        Some(state) => match serde_json::to_string(&ResumeStateJson::from(&state)) {
            Ok(s) => into_cstring(s),
            Err(_) => std::ptr::null_mut(),
        },
        None => std::ptr::null_mut(),
    }
}

// ---- m3u / m3u8 playlists -----------------------------------------------

/// Import an `.m3u` / `.m3u8` file into the queue at `position` (see
/// [`insert_position`]; `index` used only for position 7). Returns the
/// imported paths as a JSON array (free with `rb_string_free`), or null if the
/// file can't be read.
#[no_mangle]
pub extern "C" fn rb_player_import_m3u(
    p: *mut Player,
    path: *const c_char,
    position: i32,
    index: usize,
) -> *mut c_char {
    let player = player_or!(p, std::ptr::null_mut());
    let Some(path) = cstr(path) else {
        return std::ptr::null_mut();
    };
    match player.import_m3u(path, insert_position(position, index)) {
        Ok(tracks) => paths_json(&tracks),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Replace the queue with the contents of an `.m3u` / `.m3u8` file. Does not
/// change playback state. Returns the loaded paths as a JSON array (free with
/// `rb_string_free`), or null on read error.
#[no_mangle]
pub extern "C" fn rb_player_load_m3u(p: *mut Player, path: *const c_char) -> *mut c_char {
    let player = player_or!(p, std::ptr::null_mut());
    let Some(path) = cstr(path) else {
        return std::ptr::null_mut();
    };
    match player.load_m3u(path) {
        Ok(tracks) => paths_json(&tracks),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Export the current queue to an `.m3u8` file (atomic write). Returns 0 on
/// success, -1 on a null handle or I/O error.
#[no_mangle]
pub extern "C" fn rb_player_export_m3u(p: *mut Player, path: *const c_char) -> i32 {
    let player = player_or!(p, -1);
    let Some(path) = cstr(path) else {
        return -1;
    };
    match player.export_m3u(path) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Read an `.m3u` / `.m3u8` file into a JSON array of entries — each
/// `{path, duration_ms, title}` (duration/title null when no `#EXTINF`).
/// Relative paths resolve against the file's directory. Free with
/// `rb_string_free`; null on read error.
#[no_mangle]
pub extern "C" fn rb_m3u_read_json(path: *const c_char) -> *mut c_char {
    let Some(path) = cstr(path) else {
        return std::ptr::null_mut();
    };
    let entries = match m3u::read(std::path::Path::new(path)) {
        Ok(e) => e,
        Err(_) => return std::ptr::null_mut(),
    };
    let json: Vec<M3uEntryJson> = entries.iter().map(Into::into).collect();
    match serde_json::to_string(&json) {
        Ok(s) => into_cstring(s),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Write a JSON array of path strings to `path` as an `.m3u8` playlist
/// (atomic). Returns 0 on success, -1 on invalid JSON or I/O error.
#[no_mangle]
pub extern "C" fn rb_m3u_write_json(path: *const c_char, json: *const c_char) -> i32 {
    let (Some(path), Some(json)) = (cstr(path), cstr(json)) else {
        return -1;
    };
    let Ok(paths) = serde_json::from_str::<Vec<String>>(json) else {
        return -1;
    };
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    match m3u::write_paths(std::path::Path::new(path), &paths) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

/// Whether `s` looks like an `http(s)://` URL (vs. a local path).
#[no_mangle]
pub extern "C" fn rb_is_url(s: *const c_char) -> bool {
    cstr(s).map(rockbox_playback::is_url).unwrap_or(false)
}

#[derive(Serialize)]
struct M3uEntryJson {
    path: String,
    duration_ms: Option<u64>,
    title: Option<String>,
}

impl From<&m3u::M3uEntry> for M3uEntryJson {
    fn from(e: &m3u::M3uEntry) -> Self {
        M3uEntryJson {
            path: e.path.to_string_lossy().into_owned(),
            duration_ms: e.duration.map(|d| d.as_millis() as u64),
            title: e.title.clone(),
        }
    }
}

/// Serialize a slice of paths to a JSON-array C string.
fn paths_json(paths: &[PathBuf]) -> *mut c_char {
    let v: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    match serde_json::to_string(&v) {
        Ok(s) => into_cstring(s),
        Err(_) => std::ptr::null_mut(),
    }
}
