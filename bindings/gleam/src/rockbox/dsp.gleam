//// The DSP pipeline: EQ, tone, surround, compressor, ReplayGain, resampler.
////
//// A `Dsp` wraps the process-wide audio DSP singleton (only one should be
//// alive at a time). It is a NIF resource, freed by the BEAM garbage
//// collector — no explicit close.
////
//// ReplayGain `mode` here uses the DSP-native values: 0 track, 1 album,
//// 2 shuffle, 3 off.

import gleam/dynamic.{type Dynamic}
import gleam/option.{type Option, None, Some}

/// Opaque DSP handle (a NIF resource).
pub type Dsp

/// Create a DSP for interleaved S16LE stereo at `sample_rate` Hz.
@external(erlang, "rockbox_ffi_nif", "dsp_new")
pub fn new(sample_rate: Int) -> Dsp

@external(erlang, "rockbox_ffi_nif", "dsp_set_input_frequency")
pub fn set_input_frequency(dsp: Dsp, hz: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_flush")
pub fn flush(dsp: Dsp) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_eq_enable")
pub fn eq_enable(dsp: Dsp, enable: Bool) -> Nil

/// Configure one EQ band (0..9). Band 0 low shelf, 9 high shelf.
@external(erlang, "rockbox_ffi_nif", "dsp_set_eq_band")
pub fn set_eq_band(
  dsp: Dsp,
  band: Int,
  cutoff_hz: Int,
  q: Float,
  gain_db: Float,
) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_eq_precut")
pub fn set_eq_precut(dsp: Dsp, db: Float) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_tone")
pub fn set_tone(dsp: Dsp, bass_db: Int, treble_db: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_tone_cutoffs")
pub fn set_tone_cutoffs(dsp: Dsp, bass_hz: Int, treble_hz: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_surround")
pub fn set_surround(
  dsp: Dsp,
  delay_ms: Int,
  balance: Int,
  fx1: Int,
  fx2: Int,
) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_channel_config")
pub fn set_channel_config(dsp: Dsp, mode: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_stereo_width")
pub fn set_stereo_width(dsp: Dsp, percent: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_compressor")
pub fn set_compressor(
  dsp: Dsp,
  threshold: Int,
  makeup_gain: Int,
  ratio: Int,
  knee: Int,
  release_time: Int,
  attack_time: Int,
) -> Nil

/// ReplayGain mode (0 track, 1 album, 2 shuffle, 3 off), clipping prevention,
/// preamp in dB.
@external(erlang, "rockbox_ffi_nif", "dsp_set_replaygain")
pub fn set_replaygain(dsp: Dsp, mode: Int, noclip: Bool, preamp_db: Float) -> Nil

@external(erlang, "rockbox_ffi_nif", "dsp_set_replaygain_gains")
fn ffi_set_replaygain_gains(
  dsp: Dsp,
  tg: Dynamic,
  ag: Dynamic,
  tp: Dynamic,
  ap: Dynamic,
) -> Nil

/// Per-track gains in plain dB / peaks as linear amplitude. `None` = absent.
pub fn set_replaygain_gains(
  dsp: Dsp,
  track_gain_db: Option(Float),
  album_gain_db: Option(Float),
  track_peak: Option(Float),
  album_peak: Option(Float),
) -> Nil {
  ffi_set_replaygain_gains(
    dsp,
    opt(track_gain_db),
    opt(album_gain_db),
    opt(track_peak),
    opt(album_peak),
  )
}

/// Native Q7.24 factors (the `raw_*` metadata fields); 0 = not tagged.
@external(erlang, "rockbox_ffi_nif", "dsp_set_replaygain_gains_raw")
pub fn set_replaygain_gains_raw(
  dsp: Dsp,
  track_gain: Int,
  album_gain: Int,
  track_peak: Int,
  album_peak: Int,
) -> Nil

/// Run interleaved stereo S16 samples through the pipeline. Both the input
/// and the result are little-endian int16 sample buffers.
@external(erlang, "rockbox_ffi_nif", "dsp_process")
pub fn process(dsp: Dsp, pcm: BitArray) -> BitArray

// Some(f) -> the float term; None -> the `nil` atom (the ABI's absent sentinel).
fn opt(o: Option(Float)) -> Dynamic {
  case o {
    Some(f) -> dynamic.float(f)
    None -> dynamic.nil()
  }
}
