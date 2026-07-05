//! FFI bindings to the Rockbox DSP pipeline (`lib/rbcodec/dsp/`), compiled
//! standalone by `build.rs` — no firmware, no kernel, no SDL.
//!
//! Pipeline stages (fixed order, each individually enable-able):
//! pre-gain (replaygain / EQ precut) → timestretch → resampler → crossfeed → 10-band EQ →
//! tone controls → bass enhancement (PBE) → auditory fatigue reduction →
//! Haas surround → channel modes → compressor.
//!
//! The raw API mirrors `dsp_core.h`; [`Dsp`] is a minimal safe wrapper for
//! the common case (interleaved 16-bit stereo in → interleaved 16-bit
//! stereo out), suitable for feeding a cpal output callback.
//!
//! Unit conventions (see `dsp_filter.c`): EQ/tone gains and Q are in
//! **tenths** (`gain = -80` → −8.0 dB, `q = 7` → Q 0.7 — pass `q * 10`).

#![allow(non_camel_case_types)]

use std::ffi::{c_int, c_long, c_uint, c_void};
use std::sync::Once;

/* enum dsp_ids */
pub const CODEC_IDX_AUDIO: c_uint = 0;
pub const CODEC_IDX_VOICE: c_uint = 1;

/* enum dsp_settings */
pub const DSP_RESET: c_uint = 0;
pub const DSP_SET_FREQUENCY: c_uint = 1;
pub const DSP_SET_SAMPLE_DEPTH: c_uint = 2;
pub const DSP_SET_STEREO_MODE: c_uint = 3;
pub const DSP_FLUSH: c_uint = 4;
pub const DSP_SET_PITCH: c_uint = 5;
pub const DSP_SET_OUT_FREQUENCY: c_uint = 6;
pub const DSP_GET_OUT_FREQUENCY: c_uint = 7;

/* enum dsp_stereo_modes */
pub const STEREO_INTERLEAVED: isize = 0;
pub const STEREO_NONINTERLEAVED: isize = 1;
pub const STEREO_MONO: isize = 2;

pub const EQ_NUM_BANDS: usize = 10;

/* enum replaygain_types (dsp_misc.h) */
pub const REPLAYGAIN_TRACK: c_int = 0;
pub const REPLAYGAIN_ALBUM: c_int = 1;
pub const REPLAYGAIN_SHUFFLE: c_int = 2; /* track gain if shuffling, else album */
pub const REPLAYGAIN_OFF: c_int = 3;

/// `struct sample_format` (dsp_core.h)
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct sample_format {
    pub version: u8,
    pub num_channels: u8,
    pub frac_bits: u8,
    pub output_scale: u8,
    pub frequency: i32,
    pub codec_frequency: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union dsp_buffer_ptrs {
    pub pin: [*const c_void; 2],
    pub p32: [*mut i32; 2],
    pub p16out: *mut i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union dsp_buffer_count {
    pub proc_mask: u32,
    pub bufcount: c_int,
}

/// `struct dsp_buffer` (dsp_core.h)
#[repr(C)]
pub struct dsp_buffer {
    pub remcount: i32,
    pub ptrs: dsp_buffer_ptrs,
    pub count: dsp_buffer_count,
    pub format: sample_format,
}

/// `struct eq_band_setting` (eq.h) — `q` and `gain` in tenths, `cutoff` in Hz
#[repr(C)]
#[derive(Clone, Copy)]
pub struct eq_band_setting {
    pub cutoff: c_int,
    pub q: c_int,
    pub gain: c_int,
}

/// `struct compressor_settings` (compressor.h) — threshold in dB (0 = off),
/// makeup_gain 0 = off / 1 = auto, ratio index (0 = 2:1 … 4 = limit),
/// knee index (0 = hard … 2 = soft), attack/release in ms
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct compressor_settings {
    pub threshold: c_int,
    pub makeup_gain: c_int,
    pub ratio: c_int,
    pub knee: c_int,
    pub release_time: c_int,
    pub attack_time: c_int,
}

/// `struct replaygain_settings` (dsp_misc.h) — `type_` is one of the
/// `REPLAYGAIN_*` constants, `preamp` in dB × 10, `noclip` scales down
/// to prevent clipping (works even without gain tags if a peak is known)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct replaygain_settings {
    pub noclip: bool,
    pub type_: c_int,
    pub preamp: c_int,
}

/* enum AUDIOHW_CHANNEL_CONFIG (shim/sound.h) — channel_config values */
pub const SOUND_CHAN_STEREO: c_int = 0;
pub const SOUND_CHAN_MONO: c_int = 1;
pub const SOUND_CHAN_CUSTOM: c_int = 2;
pub const SOUND_CHAN_MONO_LEFT: c_int = 3;
pub const SOUND_CHAN_MONO_RIGHT: c_int = 4;
pub const SOUND_CHAN_KARAOKE: c_int = 5;
pub const SOUND_CHAN_SWAP: c_int = 6;

/// Opaque `struct dsp_config`
#[repr(C)]
pub struct dsp_config {
    _private: [u8; 0],
}

extern "C" {
    pub fn dsp_init();
    pub fn dsp_get_config(dsp_id: c_uint) -> *mut dsp_config;
    pub fn dsp_get_id(dsp: *const dsp_config) -> c_uint;
    pub fn dsp_configure(dsp: *mut dsp_config, setting: c_uint, value: isize) -> isize;
    pub fn dsp_process(
        dsp: *mut dsp_config,
        src: *mut dsp_buffer,
        dst: *mut dsp_buffer,
        thread_yield: bool,
    );

    /* eq.c */
    pub fn dsp_eq_enable(enable: bool);
    pub fn dsp_set_eq_precut(precut: c_int);
    pub fn dsp_set_eq_coefs(band: c_int, setting: *const eq_band_setting);

    /* dsp_sample_output.c */
    pub fn dsp_dither_enable(enable: bool);

    /* tone_controls.c — gains in dB×10; prescale must be set LAST: it
     * recomputes the filter coefficients and enables/disables the stage */
    pub fn tone_set_bass(bass: c_int);
    pub fn tone_set_treble(treble: c_int);
    pub fn tone_set_bass_cutoff(hz: c_int);
    pub fn tone_set_treble_cutoff(hz: c_int);
    pub fn tone_set_prescale(prescale: c_int);

    /* surround.c — Haas surround. enable takes the delay in ms (>0 = on) */
    pub fn dsp_surround_enable(delay_ms: c_int);
    pub fn dsp_surround_set_balance(balance: c_int);
    pub fn dsp_surround_set_cutoff(frq_l: c_int, frq_h: c_int);
    pub fn dsp_surround_side_only(side_only: bool);
    pub fn dsp_surround_mix(mix: c_int);

    /* channel_mode.c — SOUND_CHAN_* config + custom stereo width in % */
    pub fn channel_mode_set_config(value: c_int);
    pub fn channel_mode_custom_set_width(value: c_int);

    /* compressor.c */
    pub fn dsp_set_compressor(settings: *const compressor_settings);

    /* dsp_misc.c */
    pub fn dsp_replaygain_set_settings(settings: *const replaygain_settings);
    pub fn dsp_set_pitch(pitch: i32);
    pub fn dsp_get_pitch() -> i32;
    pub fn dsp_set_all_output_frequency(samplerate: c_uint);
    pub fn dsp_get_output_frequency(dsp: *mut dsp_config) -> c_uint;

    /* shim/rbdsp_shim.c — sends REPLAYGAIN_SET_GAINS to the audio DSP.
     * Gains/peaks are Q7.24 linear factors (get_replaygain_int output);
     * 0 = not tagged. */
    pub fn rbdsp_replaygain_set_gains(
        track_gain: c_long,
        album_gain: c_long,
        track_peak: c_long,
        album_peak: c_long,
    );
    /* dB × 100 → Q7.24 linear scale factor */
    pub fn get_replaygain_int(int_gain: c_long) -> c_long;
}

static DSP_INIT: Once = Once::new();

/// Safe wrapper around the audio DSP instance for interleaved 16-bit stereo.
///
/// The underlying `dsp_config` is a static singleton (`CODEC_IDX_AUDIO`), so
/// only one `Dsp` may exist at a time and it is not `Send`/`Sync`.
pub struct Dsp {
    cfg: *mut dsp_config,
}

impl Dsp {
    /// Set up the audio DSP for interleaved S16LE stereo at `sample_rate`,
    /// output at the same rate (no resampling).
    pub fn new(sample_rate: u32) -> Self {
        DSP_INIT.call_once(|| unsafe { dsp_init() });
        let cfg = unsafe { dsp_get_config(CODEC_IDX_AUDIO) };
        assert!(!cfg.is_null());
        unsafe {
            dsp_configure(cfg, DSP_RESET, 0);
            dsp_configure(cfg, DSP_SET_OUT_FREQUENCY, sample_rate as isize);
            dsp_configure(cfg, DSP_SET_FREQUENCY, sample_rate as isize);
            dsp_configure(cfg, DSP_SET_SAMPLE_DEPTH, 16);
            dsp_configure(cfg, DSP_SET_STEREO_MODE, STEREO_INTERLEAVED);
        }
        Dsp { cfg }
    }

    pub fn raw(&mut self) -> *mut dsp_config {
        self.cfg
    }

    /// Change the input sample rate (e.g. on track change); the resampler
    /// stage engages automatically when it differs from the output rate.
    pub fn set_input_frequency(&mut self, hz: u32) {
        unsafe { dsp_configure(self.cfg, DSP_SET_FREQUENCY, hz as isize) };
    }

    /// Drop any samples buffered inside the pipeline (call on seek/stop).
    pub fn flush(&mut self) {
        unsafe { dsp_configure(self.cfg, DSP_FLUSH, 0) };
    }

    pub fn eq_enable(&mut self, enable: bool) {
        unsafe { dsp_eq_enable(enable) };
    }

    /// Bass/treble tone controls — shelving filters at fixed cutoffs
    /// (200 Hz bass, 3.5 kHz treble). Gains in plain dB, matching
    /// rockboxd's `bass` / `treble` settings.toml keys. 0/0 disables
    /// the stage. Mirrors firmware sound.c: values ×10, prescale by
    /// the larger boost to avoid clipping.
    pub fn set_tone(&mut self, bass_db: i32, treble_db: i32) {
        let bass = bass_db * 10;
        let treble = treble_db * 10;
        unsafe {
            tone_set_bass(bass);
            tone_set_treble(treble);
            tone_set_prescale(bass.max(treble).max(0));
        }
    }

    /// Override the tone-control shelf cutoffs in Hz (0 = defaults:
    /// 200 Hz bass, 3500 Hz treble). Call before [`Dsp::set_tone`] — the
    /// coefficients are recomputed by its prescale step.
    pub fn set_tone_cutoffs(&mut self, bass_hz: i32, treble_hz: i32) {
        unsafe {
            tone_set_bass_cutoff(bass_hz);
            tone_set_treble_cutoff(treble_hz);
        }
    }

    /// Haas surround. `delay_ms` > 0 enables the stage (rockboxd's
    /// `surround_enabled` key IS the delay in ms, 0 = off); `balance` in
    /// percent, `fx1`/`fx2` are the band-split cutoffs in Hz. Argument
    /// order mirrors apps/settings.c: balance, cutoffs, then enable.
    pub fn set_surround(&mut self, delay_ms: i32, balance: i32, fx1: i32, fx2: i32) {
        unsafe {
            dsp_surround_set_balance(balance);
            if fx1 > 0 && fx2 > 0 {
                dsp_surround_set_cutoff(fx1, fx2);
            }
            dsp_surround_enable(delay_ms);
        }
    }

    /// Channel configuration (`SOUND_CHAN_*`: stereo / mono / custom /
    /// karaoke / swap …).
    pub fn set_channel_config(&mut self, mode: i32) {
        unsafe { channel_mode_set_config(mode) };
    }

    /// Stereo width in percent — only audible when the channel config is
    /// `SOUND_CHAN_CUSTOM` (100 = unchanged, 0 = mono, >100 = wider).
    pub fn set_stereo_width(&mut self, percent: i32) {
        unsafe { channel_mode_custom_set_width(percent) };
    }

    /// Dynamic-range compressor; `threshold` of 0 disables the stage.
    pub fn set_compressor(&mut self, settings: &compressor_settings) {
        unsafe { dsp_set_compressor(settings) };
    }

    /// Replaygain mode (`REPLAYGAIN_TRACK` / `_ALBUM` / `_SHUFFLE` /
    /// `_OFF`), clipping prevention, and preamp in plain dB. This is the
    /// per-player setting; the per-track values come from
    /// [`Dsp::set_replaygain_gains`] — both are needed before the
    /// pre-gain stage engages.
    pub fn set_replaygain(&mut self, mode: i32, noclip: bool, preamp_db: f32) {
        let settings = replaygain_settings {
            noclip,
            type_: mode,
            preamp: (preamp_db * 10.0).round() as c_int,
        };
        unsafe { dsp_replaygain_set_settings(&settings) };
    }

    /// Per-track replaygain values as tagged in the file's metadata:
    /// gains in plain dB, peaks as linear peak amplitude (1.0 = full
    /// scale), `None` = tag absent. Call on every track change — the
    /// `DSP_RESET` issued by [`Dsp::new`] zeroes the gains but keeps the
    /// mode from [`Dsp::set_replaygain`].
    pub fn set_replaygain_gains(
        &mut self,
        track_gain_db: Option<f32>,
        album_gain_db: Option<f32>,
        track_peak: Option<f32>,
        album_peak: Option<f32>,
    ) {
        let gain = |db: Option<f32>| {
            db.map_or(0, |db| unsafe {
                get_replaygain_int((db * 100.0).round() as c_long)
            })
        };
        let peak = |p: Option<f32>| p.map_or(0, |p| (p * (1 << 24) as f32).round() as c_long);
        unsafe {
            rbdsp_replaygain_set_gains(
                gain(track_gain_db),
                gain(album_gain_db),
                peak(track_peak),
                peak(album_peak),
            )
        };
    }

    /// Native-unit variant of [`Dsp::set_replaygain_gains`]: Q7.24 linear
    /// factors exactly as Rockbox's metadata parser stores them in
    /// `mp3entry` (`get_replaygain_int` output), 0 = not tagged.
    pub fn set_replaygain_gains_raw(
        &mut self,
        track_gain: c_long,
        album_gain: c_long,
        track_peak: c_long,
        album_peak: c_long,
    ) {
        unsafe { rbdsp_replaygain_set_gains(track_gain, album_gain, track_peak, album_peak) };
    }

    /// Configure one EQ band. Band 0 is a low shelf, band `EQ_NUM_BANDS-1`
    /// a high shelf, the rest are peaking filters. `q` and `gain_db` are in
    /// plain units here; the ×10 fixed-point convention is applied inside.
    pub fn set_eq_band(&mut self, band: usize, cutoff_hz: i32, q: f32, gain_db: f32) {
        assert!(band < EQ_NUM_BANDS);
        let setting = eq_band_setting {
            cutoff: cutoff_hz,
            q: (q * 10.0).round() as c_int,
            gain: (gain_db * 10.0).round() as c_int,
        };
        unsafe { dsp_set_eq_coefs(band as c_int, &setting) };
    }

    /// Configure one EQ band with native Rockbox units — `q` and `gain` in
    /// tenths (`q = 70` → Q 7.0, `gain = -125` → −12.5 dB), exactly the
    /// format of rockboxd's `[[eq_band_settings]]` in settings.toml.
    pub fn set_eq_band_raw(&mut self, band: usize, setting: eq_band_setting) {
        assert!(band < EQ_NUM_BANDS);
        unsafe { dsp_set_eq_coefs(band as c_int, &setting) };
    }

    /// EQ precut in dB (applied as negative pre-gain to avoid clipping).
    pub fn set_eq_precut(&mut self, db: f32) {
        unsafe { dsp_set_eq_precut((db * 10.0).round() as c_int) };
    }

    /// Run interleaved stereo S16 frames through the pipeline, appending
    /// processed interleaved stereo S16 frames to `out`. Returns the number
    /// of frames produced (not necessarily equal to the input count — the
    /// resampler/timestretch stages may buffer internally).
    pub fn process(&mut self, input: &[i16], out: &mut Vec<i16>) -> usize {
        assert!(input.len() % 2 == 0, "input must be interleaved stereo");
        let mut produced = 0usize;
        let mut chunk = [0i16; 8192]; // 4096 frames per dsp_process call

        let mut src = dsp_buffer {
            remcount: (input.len() / 2) as i32,
            ptrs: dsp_buffer_ptrs {
                pin: [input.as_ptr() as *const c_void; 2],
            },
            count: dsp_buffer_count { proc_mask: 0 },
            format: sample_format::default(), // tagged by dsp_process
        };

        loop {
            let mut dst = dsp_buffer {
                remcount: 0,
                ptrs: dsp_buffer_ptrs {
                    p16out: chunk.as_mut_ptr(),
                },
                count: dsp_buffer_count {
                    bufcount: (chunk.len() / 2) as c_int,
                },
                format: sample_format::default(),
            };
            unsafe { dsp_process(self.cfg, &mut src, &mut dst, false) };
            let frames = dst.remcount as usize;
            if frames == 0 && src.remcount <= 0 {
                break;
            }
            out.extend_from_slice(&chunk[..frames * 2]);
            produced += frames;
            if src.remcount <= 0 && frames < chunk.len() / 2 {
                break; // input drained and pipeline made no full chunk
            }
        }
        produced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /* The underlying dsp_config is a static singleton, so all assertions
     * against it live in one test. */
    #[test]
    fn replaygain_track_gain_scales_amplitude() {
        let mut dsp = Dsp::new(44100);
        dsp.set_replaygain(REPLAYGAIN_TRACK, false, 0.0);
        dsp.set_replaygain_gains(Some(-6.0206), None, None, None); /* ×0.5 */

        let input: Vec<i16> = (0..44100)
            .flat_map(|i| {
                let s = ((i as f32 * 2.0 * std::f32::consts::PI * 1000.0 / 44100.0).sin() * 16000.0)
                    as i16;
                [s, s]
            })
            .collect();
        let mut out = Vec::new();
        dsp.process(&input, &mut out);

        let peak = out.iter().map(|s| (*s as i32).abs()).max().unwrap();
        assert!(
            (7600..=8400).contains(&peak),
            "expected ~8000 after -6.02 dB track gain, got peak {peak}"
        );

        /* REPLAYGAIN_OFF restores unity */
        dsp.set_replaygain(REPLAYGAIN_OFF, false, 0.0);
        dsp.flush();
        out.clear();
        dsp.process(&input, &mut out);
        let peak = out.iter().map(|s| (*s as i32).abs()).max().unwrap();
        assert!(
            (15200..=16000).contains(&peak),
            "expected ~16000 with replaygain off, got peak {peak}"
        );
    }
}
