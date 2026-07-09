//! Flat C ABI over [`rockbox_playback::Player`].
//!
//! A player owns a background engine thread and a live cpal output stream,
//! so it only works where a real output device exists. The handle is an
//! opaque pointer; call `rb_player_free` to stop playback and join the
//! engine. Status is returned as a JSON C string.

use crate::meta::MetadataJson;
use crate::util::{cstr, into_cstring};
use rockbox_playback::{
    CrossfadeMode, CrossfadeSettings, MixMode, PlaybackState, Player, PlayerConfig, ReplayGainMode,
    Status,
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
        volume,
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

/// Replace the queue from a JSON array of path strings, e.g.
/// `["a.flac","b.mp3"]`. Invalid JSON is ignored.
#[no_mangle]
pub extern "C" fn rb_player_set_queue_json(p: *mut Player, json: *const c_char) {
    let player = player!(p);
    let Some(json) = cstr(json) else { return };
    if let Ok(paths) = serde_json::from_str::<Vec<String>>(json) {
        player.set_queue(paths.into_iter().map(PathBuf::from).collect::<Vec<_>>());
    }
}

/// Append one track to the end of the queue.
#[no_mangle]
pub extern "C" fn rb_player_enqueue(p: *mut Player, path: *const c_char) {
    let player = player!(p);
    if let Some(path) = cstr(path) {
        player.enqueue(PathBuf::from(path));
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

/// Current volume, 0.0..=1.0 (0.0 on a null handle).
#[no_mangle]
pub extern "C" fn rb_player_volume(p: *mut Player) -> f32 {
    if p.is_null() {
        return 0.0;
    }
    unsafe { &*p }.volume()
}

/// The output sample rate everything is resampled to (0 on a null handle).
#[no_mangle]
pub extern "C" fn rb_player_sample_rate(p: *mut Player) -> u32 {
    if p.is_null() {
        return 0;
    }
    unsafe { &*p }.sample_rate()
}

#[derive(Serialize)]
struct StatusJson {
    /// "stopped" | "playing" | "paused"
    state: &'static str,
    index: Option<usize>,
    position_ms: u64,
    duration_ms: u64,
    queue_len: usize,
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
