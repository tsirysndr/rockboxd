defmodule Rockbox.Dsp do
  @moduledoc """
  The DSP pipeline: EQ, tone, surround, compressor, ReplayGain, resampler.

  A DSP handle wraps the process-wide audio DSP singleton, so only one should
  be alive at a time. The handle is a NIF resource — it is freed by the BEAM
  garbage collector, no explicit close needed.

  ReplayGain `mode` here uses the *DSP-native* values (see `t:rg_mode/0`):
  `0` track, `1` album, `2` shuffle, `3` off.
  """

  @typedoc "Opaque DSP handle (a NIF resource)."
  @opaque t :: reference()

  @typedoc "DSP ReplayGain mode: 0 track, 1 album, 2 shuffle, 3 off."
  @type rg_mode :: 0..3

  @doc "Create a DSP for interleaved S16LE stereo at `sample_rate` Hz."
  @spec new(pos_integer()) :: t()
  def new(sample_rate), do: :rockbox_ffi_nif.dsp_new(sample_rate)

  @spec set_input_frequency(t(), pos_integer()) :: :ok
  def set_input_frequency(d, hz), do: nilok(:rockbox_ffi_nif.dsp_set_input_frequency(d, hz))

  @spec flush(t()) :: :ok
  def flush(d), do: nilok(:rockbox_ffi_nif.dsp_flush(d))

  @spec eq_enable(t(), boolean()) :: :ok
  def eq_enable(d, enable), do: nilok(:rockbox_ffi_nif.dsp_eq_enable(d, enable))

  @doc "Configure one EQ band (0..9). Band 0 low shelf, 9 high shelf."
  @spec set_eq_band(t(), 0..9, integer(), number(), number()) :: :ok
  def set_eq_band(d, band, cutoff_hz, q, gain_db),
    do: nilok(:rockbox_ffi_nif.dsp_set_eq_band(d, band, cutoff_hz, q / 1, gain_db / 1))

  @spec set_eq_precut(t(), number()) :: :ok
  def set_eq_precut(d, db), do: nilok(:rockbox_ffi_nif.dsp_set_eq_precut(d, db / 1))

  @spec set_tone(t(), integer(), integer()) :: :ok
  def set_tone(d, bass_db, treble_db), do: nilok(:rockbox_ffi_nif.dsp_set_tone(d, bass_db, treble_db))

  @spec set_tone_cutoffs(t(), integer(), integer()) :: :ok
  def set_tone_cutoffs(d, bass_hz, treble_hz),
    do: nilok(:rockbox_ffi_nif.dsp_set_tone_cutoffs(d, bass_hz, treble_hz))

  @spec set_surround(t(), integer(), integer(), integer(), integer()) :: :ok
  def set_surround(d, delay_ms, balance, fx1, fx2),
    do: nilok(:rockbox_ffi_nif.dsp_set_surround(d, delay_ms, balance, fx1, fx2))

  @spec set_channel_config(t(), integer()) :: :ok
  def set_channel_config(d, mode), do: nilok(:rockbox_ffi_nif.dsp_set_channel_config(d, mode))

  @spec set_stereo_width(t(), integer()) :: :ok
  def set_stereo_width(d, pct), do: nilok(:rockbox_ffi_nif.dsp_set_stereo_width(d, pct))

  @spec set_compressor(t(), integer(), integer(), integer(), integer(), integer(), integer()) :: :ok
  def set_compressor(d, threshold, makeup_gain, ratio, knee, release_time, attack_time),
    do:
      nilok(
        :rockbox_ffi_nif.dsp_set_compressor(
          d, threshold, makeup_gain, ratio, knee, release_time, attack_time
        )
      )

  @doc "ReplayGain mode (see `t:rg_mode/0`), clipping prevention, preamp in dB."
  @spec set_replaygain(t(), rg_mode(), boolean(), number()) :: :ok
  def set_replaygain(d, mode, noclip, preamp_db),
    do: nilok(:rockbox_ffi_nif.dsp_set_replaygain(d, mode, noclip, preamp_db / 1))

  @doc """
  Per-track gains in plain dB / peaks as linear amplitude. Pass `nil` for an
  absent tag.
  """
  @spec set_replaygain_gains(
          t(),
          number() | nil,
          number() | nil,
          number() | nil,
          number() | nil
        ) :: :ok
  def set_replaygain_gains(d, track_gain_db, album_gain_db, track_peak, album_peak),
    do:
      nilok(
        :rockbox_ffi_nif.dsp_set_replaygain_gains(
          d, f(track_gain_db), f(album_gain_db), f(track_peak), f(album_peak)
        )
      )

  @doc "Native Q7.24 factors (the `raw_*` metadata fields); 0 = not tagged."
  @spec set_replaygain_gains_raw(t(), integer(), integer(), integer(), integer()) :: :ok
  def set_replaygain_gains_raw(d, track_gain, album_gain, track_peak, album_peak),
    do:
      nilok(
        :rockbox_ffi_nif.dsp_set_replaygain_gains_raw(
          d, track_gain, album_gain, track_peak, album_peak
        )
      )

  @doc """
  Run interleaved stereo S16 samples through the pipeline. Accepts and returns
  a binary of little-endian int16 samples (use `samples_to_binary/1` /
  `binary_to_samples/1` to convert lists).
  """
  @spec process(t(), binary()) :: binary()
  def process(d, pcm) when is_binary(pcm), do: :rockbox_ffi_nif.dsp_process(d, pcm)

  @doc "Convert a list of int16 samples to a little-endian binary."
  @spec samples_to_binary([integer()]) :: binary()
  def samples_to_binary(samples),
    do: for(s <- samples, into: <<>>, do: <<s::16-little-signed>>)

  @doc "Convert a little-endian int16 binary to a list of samples."
  @spec binary_to_samples(binary()) :: [integer()]
  def binary_to_samples(bin), do: for(<<s::16-little-signed <- bin>>, do: s)

  # nil (absent) or a float; keep nil so the NIF maps it to NaN.
  defp f(nil), do: nil
  defp f(n), do: n / 1

  defp nilok(nil), do: :ok
  defp nilok(other), do: other
end
