//! Flat C ABI over [`rockbox_playback::Player`].
//!
//! A player owns a background engine thread and a live cpal output stream,
//! so it only works where a real output device exists. The handle is an
//! opaque pointer; call `rb_player_free` to stop playback and join the
//! engine. Status is returned as a JSON C string.

use crate::meta::MetadataJson;
use crate::util::{cstr, into_cstring};
use rockbox_playback::{
    m3u, CrossfadeMode, CrossfadeSettings, InsertPosition, MixMode, PlaybackState, Player,
    PlayerConfig, ReplayGainMode, ResumeState, Status,
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
