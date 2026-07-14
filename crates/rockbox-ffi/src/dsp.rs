//! Flat C ABI over [`rockbox_dsp::Dsp`].
//!
//! The handle wraps the audio DSP singleton — create exactly one at a time
//! and use it from a single thread (the underlying `dsp_config` is a static
//! and is not `Send`/`Sync`).
//!
//! Optional ReplayGain gains use `NaN` as the "tag absent" sentinel.

use rockbox_dsp::{compressor_settings, Dsp};
use std::os::raw::c_int;

/// Create an audio DSP configured for interleaved S16LE stereo at
/// `sample_rate` (output at the same rate). Free with [`rb_dsp_free`].
#[no_mangle]
pub extern "C" fn rb_dsp_new(sample_rate: u32) -> *mut Dsp {
    Box::into_raw(Box::new(Dsp::new(sample_rate)))
}

/// Destroy a DSP handle. Null is ignored.
#[no_mangle]
pub extern "C" fn rb_dsp_free(p: *mut Dsp) {
    if !p.is_null() {
        unsafe { drop(Box::from_raw(p)) };
    }
}

/// Borrow the handle as `&mut Dsp`; returns without acting on null.
macro_rules! dsp {
    ($p:ident) => {{
        if $p.is_null() {
            return;
        }
        unsafe { &mut *$p }
    }};
}

/// Change the input sample rate (engages the resampler when it differs
/// from the output rate).
#[no_mangle]
pub extern "C" fn rb_dsp_set_input_frequency(p: *mut Dsp, hz: u32) {
    dsp!(p).set_input_frequency(hz);
}

/// Drop any samples buffered inside the pipeline (call on seek/stop).
#[no_mangle]
pub extern "C" fn rb_dsp_flush(p: *mut Dsp) {
    dsp!(p).flush();
}

#[no_mangle]
pub extern "C" fn rb_dsp_eq_enable(p: *mut Dsp, enable: bool) {
    dsp!(p).eq_enable(enable);
}

/// Bass/treble tone controls in plain dB (0/0 disables the stage).
#[no_mangle]
pub extern "C" fn rb_dsp_set_tone(p: *mut Dsp, bass_db: i32, treble_db: i32) {
    dsp!(p).set_tone(bass_db, treble_db);
}

/// Override tone-control shelf cutoffs in Hz (0 = defaults). Call before
/// [`rb_dsp_set_tone`].
#[no_mangle]
pub extern "C" fn rb_dsp_set_tone_cutoffs(p: *mut Dsp, bass_hz: i32, treble_hz: i32) {
    dsp!(p).set_tone_cutoffs(bass_hz, treble_hz);
}

/// Haas surround: `delay_ms > 0` enables the stage; `balance` in percent;
/// `fx1`/`fx2` are band-split cutoffs in Hz (0/0 = keep defaults).
#[no_mangle]
pub extern "C" fn rb_dsp_set_surround(
    p: *mut Dsp,
    delay_ms: i32,
    balance: i32,
    fx1: i32,
    fx2: i32,
) {
    dsp!(p).set_surround(delay_ms, balance, fx1, fx2);
}

/// Channel configuration (`SOUND_CHAN_*`: 0 stereo, 1 mono, 2 custom,
/// 3 mono-left, 4 mono-right, 5 karaoke, 6 swap).
#[no_mangle]
pub extern "C" fn rb_dsp_set_channel_config(p: *mut Dsp, mode: i32) {
    dsp!(p).set_channel_config(mode);
}

/// Stereo width in percent (only audible with the custom channel config).
#[no_mangle]
pub extern "C" fn rb_dsp_set_stereo_width(p: *mut Dsp, percent: i32) {
    dsp!(p).set_stereo_width(percent);
}

/// Dynamic-range compressor; `threshold` of 0 disables the stage.
#[no_mangle]
pub extern "C" fn rb_dsp_set_compressor(
    p: *mut Dsp,
    threshold: i32,
    makeup_gain: i32,
    ratio: i32,
    knee: i32,
    release_time: i32,
    attack_time: i32,
) {
    dsp!(p).set_compressor(&compressor_settings {
        threshold: threshold as c_int,
        makeup_gain: makeup_gain as c_int,
        ratio: ratio as c_int,
        knee: knee as c_int,
        release_time: release_time as c_int,
        attack_time: attack_time as c_int,
    });
}

/// ReplayGain mode (0 track, 1 album, 2 shuffle, 3 off), clipping
/// prevention, and preamp in plain dB.
#[no_mangle]
pub extern "C" fn rb_dsp_set_replaygain(p: *mut Dsp, mode: i32, noclip: bool, preamp_db: f32) {
    dsp!(p).set_replaygain(mode, noclip, preamp_db);
}

/// Per-track ReplayGain values from the file's tags. Gains in plain dB,
/// peaks as linear amplitude (1.0 = full scale). Pass `NaN` for any tag
/// that is absent.
#[no_mangle]
pub extern "C" fn rb_dsp_set_replaygain_gains(
    p: *mut Dsp,
    track_gain_db: f32,
    album_gain_db: f32,
    track_peak: f32,
    album_peak: f32,
) {
    let opt = |v: f32| if v.is_nan() { None } else { Some(v) };
    dsp!(p).set_replaygain_gains(
        opt(track_gain_db),
        opt(album_gain_db),
        opt(track_peak),
        opt(album_peak),
    );
}

/// Native-unit variant: Q7.24 linear scale factors exactly as Rockbox's
/// metadata parser stores them (`rb_meta_read_json` `replaygain.raw_*`),
/// 0 = not tagged.
#[no_mangle]
pub extern "C" fn rb_dsp_set_replaygain_gains_raw(
    p: *mut Dsp,
    track_gain: i64,
    album_gain: i64,
    track_peak: i64,
    album_peak: i64,
) {
    dsp!(p).set_replaygain_gains_raw(
        track_gain as std::os::raw::c_long,
        album_gain as std::os::raw::c_long,
        track_peak as std::os::raw::c_long,
        album_peak as std::os::raw::c_long,
    );
}

/// Configure one EQ band (0..=9). Band 0 is a low shelf, band 9 a high
/// shelf, the rest peaking. `q`/`gain_db` in plain units.
#[no_mangle]
pub extern "C" fn rb_dsp_set_eq_band(
    p: *mut Dsp,
    band: usize,
    cutoff_hz: i32,
    q: f32,
    gain_db: f32,
) {
    if band >= rockbox_dsp::EQ_NUM_BANDS {
        return;
    }
    dsp!(p).set_eq_band(band, cutoff_hz, q, gain_db);
}

/// EQ precut in dB (negative pre-gain to avoid clipping).
#[no_mangle]
pub extern "C" fn rb_dsp_set_eq_precut(p: *mut Dsp, db: f32) {
    dsp!(p).set_eq_precut(db);
}

/// Headphone crossfeed. `mode`: 0 off, 1 Meier (fixed profile), 2 custom.
/// `direct_gain`, `cross_lf_gain`, `cross_hf_gain` are in tenths of a dB (≤ 0);
/// `hf_cutoff` in Hz. The `cross_*`/`hf_cutoff` params only apply in custom mode.
#[no_mangle]
pub extern "C" fn rb_dsp_set_crossfeed(
    p: *mut Dsp,
    mode: i32,
    direct_gain: i32,
    cross_lf_gain: i32,
    cross_hf_gain: i32,
    hf_cutoff: i32,
) {
    let dsp = dsp!(p);
    dsp.set_crossfeed(mode);
    dsp.set_crossfeed_direct_gain(direct_gain);
    dsp.set_crossfeed_cross_params(cross_lf_gain, cross_hf_gain, hf_cutoff);
}

/// Perceptual Bass Enhancement (PBE). `strength` percent (0 = off … 100);
/// `precut` headroom in tenths of a dB (≤ 0), applied before the boost.
#[no_mangle]
pub extern "C" fn rb_dsp_set_pbe(p: *mut Dsp, strength: i32, precut: i32) {
    let dsp = dsp!(p);
    dsp.pbe_enable(strength);
    dsp.set_pbe_precut(precut);
}

/// Run interleaved stereo S16 frames through the pipeline. `input`/`in_len`
/// describe the input samples (length must be even). On return `*out_len`
/// holds the number of `i16` samples produced and the return value points
/// at a heap buffer the caller MUST free with `rb_buffer_free(ptr, *out_len)`.
/// Returns null (with `*out_len = 0`) on a null handle or odd length.
#[no_mangle]
pub extern "C" fn rb_dsp_process(
    p: *mut Dsp,
    input: *const i16,
    in_len: usize,
    out_len: *mut usize,
) -> *mut i16 {
    if !out_len.is_null() {
        unsafe { *out_len = 0 };
    }
    if p.is_null() || input.is_null() || in_len % 2 != 0 {
        return std::ptr::null_mut();
    }
    let dsp = unsafe { &mut *p };
    let inp = unsafe { std::slice::from_raw_parts(input, in_len) };
    let mut out: Vec<i16> = Vec::new();
    dsp.process(inp, &mut out);

    // Boxed slice guarantees capacity == length, so `rb_buffer_free` can
    // safely rebuild the Vec with `cap = len`.
    let boxed = out.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut i16;
    if !out_len.is_null() {
        unsafe { *out_len = len };
    }
    ptr
}
