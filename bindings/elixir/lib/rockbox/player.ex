defmodule Rockbox.Player do
  @moduledoc """
  Queue-based player with native ReplayGain and Rockbox crossfade.

  A player owns a live audio output device and a background engine thread, so
  it only works where an output device exists. The handle is a NIF resource,
  freed by the BEAM garbage collector (which stops playback).

  ReplayGain `mode` here uses the *player* values: `0` off, `1` track,
  `2` album. Crossfade `mode`: `0` off, `1` auto-skip, `2` manual-skip,
  `3` shuffle, `4` shuffle-or-manual, `5` always. Mix mode: `0` crossfade,
  `1` mix.

  ## Piping

  Every mutating/command function returns the player handle, so calls chain:

      p
      |> Player.set_queue(tracks)
      |> Player.set_shuffle(true)
      |> Player.set_repeat(:all)
      |> Player.play()

  Getters/queries (`status/1`, `volume/1`, `queue/1`, `repeat/1`,
  `dsp_settings/1`, `resume/1`, …) still return their values.
  """

  @typedoc "Opaque player handle (a NIF resource)."
  @opaque t :: reference()

  @default_config %{
    sample_rate: 0,
    buffer_seconds: 4.0,
    volume: 1.0,
    replaygain_mode: 0,
    replaygain_preamp_db: 0.0,
    replaygain_prevent_clipping: true,
    crossfade_mode: 0,
    fade_out_delay_ms: 0,
    fade_out_duration_ms: 2000,
    fade_in_delay_ms: 0,
    fade_in_duration_ms: 2000,
    mix_mode: 0,
    # Resume: when `resume_file` is a path (an .m3u8), the player auto-persists
    # the queue + exact position to it. `nil`/`""` disables resume. An interval
    # of 0 uses the native default (5 s).
    resume_file: nil,
    resume_save_interval_ms: 0
  }

  @doc "Create a player on the default device with default settings."
  @spec new() :: t() | nil
  def new, do: :rockbox_ffi_nif.player_new()

  @doc """
  Create a player with configuration overrides (see `@default_config` keys).
  `sample_rate: 0` means the device default.

  Pass `resume_file: "/path/to/state.m3u8"` (optionally with
  `resume_save_interval_ms:`) to enable automatic queue+position persistence;
  restore it later with `resume/1`.
  """
  @spec new(keyword() | map()) :: t() | nil
  def new(opts) do
    c = Map.merge(@default_config, Map.new(opts))

    resume_file =
      case c.resume_file do
        nil -> nil
        "" -> nil
        path -> IO.iodata_to_binary([path])
      end

    if resume_file == nil and c.resume_save_interval_ms == 0 do
      :rockbox_ffi_nif.player_new_with_config(
        c.sample_rate,
        c.buffer_seconds / 1,
        c.volume / 1,
        c.replaygain_mode,
        c.replaygain_preamp_db / 1,
        c.replaygain_prevent_clipping,
        c.crossfade_mode,
        c.fade_out_delay_ms,
        c.fade_out_duration_ms,
        c.fade_in_delay_ms,
        c.fade_in_duration_ms,
        c.mix_mode
      )
    else
      :rockbox_ffi_nif.player_new_with_config_ex(
        c.sample_rate,
        c.buffer_seconds / 1,
        c.volume / 1,
        c.replaygain_mode,
        c.replaygain_preamp_db / 1,
        c.replaygain_prevent_clipping,
        c.crossfade_mode,
        c.fade_out_delay_ms,
        c.fade_out_duration_ms,
        c.fade_in_delay_ms,
        c.fade_in_duration_ms,
        c.mix_mode,
        # nil disables resume in the NIF; a binary path enables it.
        resume_file || nil,
        c.resume_save_interval_ms
      )
    end
  end

  @doc """
  Replace the queue. Each entry may be a local file path, an `http(s)://`
  URL to a finite remote file, or a live-radio / streaming URL.
  """
  @spec set_queue(t(), [Path.t()]) :: t()
  def set_queue(p, paths) do
    json = :json.encode(Enum.map(paths, &IO.iodata_to_binary([&1])))
    then_self(:rockbox_ffi_nif.player_set_queue_json(p, IO.iodata_to_binary(json)), p)
  end

  @doc """
  Append one track to the queue. `path` may be a local file path, an
  `http(s)://` URL to a finite remote file, or a live-radio / streaming URL.
  """
  @spec enqueue(t(), Path.t()) :: t()
  def enqueue(p, path),
    do: then_self(:rockbox_ffi_nif.player_enqueue(p, IO.iodata_to_binary([path])), p)

  @spec play(t()) :: t()
  def play(p), do: then_self(:rockbox_ffi_nif.player_play(p), p)
  @spec pause(t()) :: t()
  def pause(p), do: then_self(:rockbox_ffi_nif.player_pause(p), p)
  @spec toggle(t()) :: t()
  def toggle(p), do: then_self(:rockbox_ffi_nif.player_toggle(p), p)
  @spec stop(t()) :: t()
  def stop(p), do: then_self(:rockbox_ffi_nif.player_stop(p), p)
  @spec next(t()) :: t()
  def next(p), do: then_self(:rockbox_ffi_nif.player_next(p), p)
  @spec previous(t()) :: t()
  def previous(p), do: then_self(:rockbox_ffi_nif.player_previous(p), p)
  @spec skip_to(t(), non_neg_integer()) :: t()
  def skip_to(p, index), do: then_self(:rockbox_ffi_nif.player_skip_to(p, index), p)
  @spec seek_ms(t(), non_neg_integer()) :: t()
  def seek_ms(p, ms), do: then_self(:rockbox_ffi_nif.player_seek_ms(p, ms), p)

  @spec set_volume(t(), number()) :: t()
  def set_volume(p, vol), do: then_self(:rockbox_ffi_nif.player_set_volume(p, vol / 1), p)

  @doc """
  Set the stereo balance, `-100` (full left) to `+100` (full right); `0` is
  centred.
  """
  @spec set_balance(t(), integer()) :: t()
  def set_balance(p, balance), do: then_self(:rockbox_ffi_nif.player_set_balance(p, balance), p)

  @spec volume(t()) :: float()
  def volume(p), do: :rockbox_ffi_nif.player_volume(p)

  @doc "Current stereo balance, `-100` (full left) to `+100` (full right)."
  @spec balance(t()) :: integer()
  def balance(p), do: :rockbox_ffi_nif.player_balance(p)

  @spec sample_rate(t()) :: non_neg_integer()
  def sample_rate(p), do: :rockbox_ffi_nif.player_sample_rate(p)

  @spec set_crossfade(t(), 0..5, integer(), integer(), integer(), integer(), 0..1) :: t()
  def set_crossfade(p, mode, fo_delay_ms, fo_dur_ms, fi_delay_ms, fi_dur_ms, mix_mode),
    do:
      then_self(
        :rockbox_ffi_nif.player_set_crossfade(
          p, mode, fo_delay_ms, fo_dur_ms, fi_delay_ms, fi_dur_ms, mix_mode
        ),
        p
      )

  @spec set_replaygain(t(), 0..2, number(), boolean()) :: t()
  def set_replaygain(p, mode, preamp_db, prevent_clipping),
    do:
      then_self(
        :rockbox_ffi_nif.player_set_replaygain(p, mode, preamp_db / 1, prevent_clipping),
        p
      )

  @doc "Enable or disable shuffle playback."
  @spec set_shuffle(t(), boolean()) :: t()
  def set_shuffle(p, enabled), do: then_self(:rockbox_ffi_nif.player_set_shuffle(p, enabled), p)

  @doc "Whether shuffle playback is currently enabled."
  @spec shuffle_enabled?(t()) :: boolean()
  def shuffle_enabled?(p), do: :rockbox_ffi_nif.player_is_shuffle_enabled(p)

  @doc """
  Set the repeat mode. Accepts a `Rockbox.RepeatMode` atom (`:off`, `:one`,
  `:all`) or its integer code (`0`, `1`, `2`).
  """
  @spec set_repeat(t(), Rockbox.RepeatMode.t() | 0..2) :: t()
  def set_repeat(p, mode),
    do: then_self(:rockbox_ffi_nif.player_set_repeat(p, Rockbox.RepeatMode.to_int(mode)), p)

  @doc "The current repeat mode as a `Rockbox.RepeatMode` atom (`:off`/`:one`/`:all`)."
  @spec repeat(t()) :: Rockbox.RepeatMode.t()
  def repeat(p), do: Rockbox.RepeatMode.from_int(:rockbox_ffi_nif.player_repeat(p))

  @doc "A snapshot of the player's status as an atom-keyed map."
  @spec status(t()) :: map()
  def status(p), do: Rockbox.decode_json(:rockbox_ffi_nif.player_status_json(p))

  @doc """
  Insert file paths / URLs into the queue at `position`.

  `position` is a `Rockbox.InsertPosition` atom (or its integer code); `index`
  is only used when `position` is `:index` (7).
  """
  @spec insert(t(), [Path.t()], Rockbox.InsertPosition.t() | 0..7, non_neg_integer()) :: t()
  def insert(p, paths, position, index \\ 0) do
    json = :json.encode(Enum.map(paths, &IO.iodata_to_binary([&1])))

    then_self(
      :rockbox_ffi_nif.player_insert_json(
        p,
        IO.iodata_to_binary(json),
        Rockbox.InsertPosition.to_int(position),
        index
      ),
      p
    )
  end

  @doc "The current queue as a list of path/URL strings."
  @spec queue(t()) :: [String.t()]
  def queue(p) do
    case :rockbox_ffi_nif.player_queue_json(p) do
      nil -> []
      json when is_binary(json) -> :json.decode(json)
    end
  end

  @doc """
  Restore the queue + exact position from the player's resume file (does NOT
  auto-play). Returns `{:ok, %{tracks: [...], index: i, elapsed_ms: ms}}` or
  `{:error, :absent}` when there is nothing to resume.
  """
  @spec resume(t()) :: {:ok, map()} | {:error, :absent}
  def resume(p) do
    case :rockbox_ffi_nif.player_resume(p) do
      nil -> {:error, :absent}
      json when is_binary(json) -> {:ok, Rockbox.decode_json(json)}
    end
  end

  @doc "Persist the queue + exact position to the resume file now. Returns the player handle."
  @spec save_resume(t()) :: t()
  def save_resume(p), do: then_self(:rockbox_ffi_nif.player_save_resume(p), p)

  @doc "Delete the resume file (forget the saved state). Returns the player handle."
  @spec clear_resume(t()) :: t()
  def clear_resume(p), do: then_self(:rockbox_ffi_nif.player_clear_resume(p), p)

  @doc """
  Import an .m3u/.m3u8 playlist into the queue at `position`.

  Returns `{:ok, [path, ...]}` with the imported entries, or `{:error, reason}`.
  `index` is only used when `position` is `:index` (7).
  """
  @spec import_m3u(t(), Path.t(), Rockbox.InsertPosition.t() | 0..7, non_neg_integer()) ::
          {:ok, [String.t()]} | {:error, term()}
  def import_m3u(p, path, position, index \\ 0) do
    case :rockbox_ffi_nif.player_import_m3u(
           p,
           IO.iodata_to_binary([path]),
           Rockbox.InsertPosition.to_int(position),
           index
         ) do
      nil -> {:error, :import_failed}
      json when is_binary(json) -> {:ok, :json.decode(json)}
    end
  end

  @doc """
  Replace the queue with an .m3u/.m3u8 playlist file.

  Returns `{:ok, [path, ...]}` with the loaded entries, or `{:error, reason}`.
  """
  @spec load_m3u(t(), Path.t()) :: {:ok, [String.t()]} | {:error, term()}
  def load_m3u(p, path) do
    case :rockbox_ffi_nif.player_load_m3u(p, IO.iodata_to_binary([path])) do
      nil -> {:error, :load_failed}
      json when is_binary(json) -> {:ok, :json.decode(json)}
    end
  end

  @doc "Export the current queue to an .m3u8 file (atomic write)."
  @spec export_m3u(t(), Path.t()) :: :ok | {:error, :export_failed}
  def export_m3u(p, path) do
    case :rockbox_ffi_nif.player_export_m3u(p, IO.iodata_to_binary([path])) do
      0 -> :ok
      _ -> {:error, :export_failed}
    end
  end

  # ---- DSP chain ------------------------------------------------------

  @doc "Enable or disable the 10-band parametric equalizer."
  @spec set_eq_enabled(t(), boolean()) :: t()
  def set_eq_enabled(p, enabled),
    do: then_self(:rockbox_ffi_nif.player_set_eq_enabled(p, enabled), p)

  @doc "Whether the equalizer is currently enabled."
  @spec eq_enabled?(t()) :: boolean()
  def eq_enabled?(p), do: :rockbox_ffi_nif.player_is_eq_enabled(p)

  @doc """
  Configure one EQ band (0..10). `cutoff_hz` in Hz, `q` a Q factor, `gain_db`
  in dB.
  """
  @spec set_eq_band(t(), 0..10, integer(), number(), number()) :: t()
  def set_eq_band(p, band, cutoff_hz, q, gain_db),
    do: then_self(:rockbox_ffi_nif.player_set_eq_band(p, band, cutoff_hz, q / 1, gain_db / 1), p)

  @doc "Global EQ pre-cut in dB (attenuation applied before the bands)."
  @spec set_eq_precut(t(), number()) :: t()
  def set_eq_precut(p, db), do: then_self(:rockbox_ffi_nif.player_set_eq_precut(p, db / 1), p)

  @doc """
  Apply a built-in EQ preset. Accepts a `Rockbox.EqPreset` atom (e.g. `:rock`)
  or its integer code.
  """
  @spec set_eq_preset(t(), Rockbox.EqPreset.t() | 0..20) :: t()
  def set_eq_preset(p, preset),
    do: then_self(:rockbox_ffi_nif.player_set_eq_preset(p, Rockbox.EqPreset.to_int(preset)), p)

  @doc """
  Bass/treble tone controls in dB, with band-split cutoffs in Hz (`0` = the
  native defaults).
  """
  @spec set_tone(t(), integer(), integer(), integer(), integer()) :: t()
  def set_tone(p, bass_db, treble_db, bass_cutoff_hz, treble_cutoff_hz),
    do:
      then_self(
        :rockbox_ffi_nif.player_set_tone(
          p, bass_db, treble_db, bass_cutoff_hz, treble_cutoff_hz
        ),
        p
      )

  @doc "Set the bass tone axis in dB, leaving treble and cutoffs unchanged."
  @spec set_bass(t(), integer()) :: t()
  def set_bass(p, bass_db), do: then_self(:rockbox_ffi_nif.player_set_bass(p, bass_db), p)

  @doc "Set the treble tone axis in dB, leaving bass and cutoffs unchanged."
  @spec set_treble(t(), integer()) :: t()
  def set_treble(p, treble_db), do: then_self(:rockbox_ffi_nif.player_set_treble(p, treble_db), p)

  @doc "Override the bass tone shelf cutoff in Hz (`0` = native default); gains unchanged."
  @spec set_bass_cutoff(t(), integer()) :: t()
  def set_bass_cutoff(p, hz), do: then_self(:rockbox_ffi_nif.player_set_bass_cutoff(p, hz), p)

  @doc "Override the treble tone shelf cutoff in Hz (`0` = native default); gains unchanged."
  @spec set_treble_cutoff(t(), integer()) :: t()
  def set_treble_cutoff(p, hz),
    do: then_self(:rockbox_ffi_nif.player_set_treble_cutoff(p, hz), p)

  @doc """
  Headphone crossfeed. `mode` accepts a `Rockbox.CrossfeedMode` atom (`:off`,
  `:meier`, `:custom`) or its integer code.

  `direct_gain`, `cross_gain` and `hf_gain` are in tenths of a dB (`<= 0`);
  `hf_cutoff_hz` is in Hz. The cross/high-frequency parameters only apply in
  `:custom` mode.
  """
  @spec set_crossfeed(
          t(),
          Rockbox.CrossfeedMode.t() | 0..2,
          integer(),
          integer(),
          integer(),
          integer()
        ) :: t()
  def set_crossfeed(p, mode, direct_gain, cross_gain, hf_gain, hf_cutoff_hz),
    do:
      then_self(
        :rockbox_ffi_nif.player_set_crossfeed(
          p,
          Rockbox.CrossfeedMode.to_int(mode),
          direct_gain,
          cross_gain,
          hf_gain,
          hf_cutoff_hz
        ),
        p
      )

  @doc """
  Perceptual bass enhancement. `strength` is a percentage (`0` = off); `precut`
  is in tenths of a dB (`<= 0`).
  """
  @spec set_bass_enhancement(t(), integer(), integer()) :: t()
  def set_bass_enhancement(p, strength, precut),
    do: then_self(:rockbox_ffi_nif.player_set_bass_enhancement(p, strength, precut), p)

  @doc """
  Auditory fatigue reduction: `0` off, `1` weak, `2` moderate, `3` strong.
  """
  @spec set_fatigue_reduction(t(), 0..3) :: t()
  def set_fatigue_reduction(p, strength),
    do: then_self(:rockbox_ffi_nif.player_set_fatigue_reduction(p, strength), p)

  @doc """
  Haas surround effect: `delay_ms` (`0` = off), `balance` %, and the band-split
  cutoffs in Hz.
  """
  @spec set_surround(t(), integer(), integer(), integer(), integer()) :: t()
  def set_surround(p, delay_ms, balance, cutoff_low_hz, cutoff_high_hz),
    do:
      then_self(
        :rockbox_ffi_nif.player_set_surround(
          p, delay_ms, balance, cutoff_low_hz, cutoff_high_hz
        ),
        p
      )

  @doc """
  Channel mixing mode. Accepts a `Rockbox.ChannelMode` atom (e.g. `:mono`) or
  its integer code.
  """
  @spec set_channel_mode(t(), Rockbox.ChannelMode.t() | 0..6) :: t()
  def set_channel_mode(p, mode),
    do: then_self(:rockbox_ffi_nif.player_set_channel_mode(p, Rockbox.ChannelMode.to_int(mode)), p)

  @doc "Custom stereo width in percent (audible with channel mode `:custom`)."
  @spec set_stereo_width(t(), integer()) :: t()
  def set_stereo_width(p, percent),
    do: then_self(:rockbox_ffi_nif.player_set_stereo_width(p, percent), p)

  @doc """
  Dynamic-range compressor. `threshold_db` `0` disables it; the remaining
  arguments are makeup gain, ratio, knee, attack (ms) and release (ms).
  """
  @spec set_compressor(t(), integer(), integer(), integer(), integer(), integer(), integer()) ::
          t()
  def set_compressor(p, threshold_db, makeup_gain, ratio, knee, attack_ms, release_ms),
    do:
      then_self(
        :rockbox_ffi_nif.player_set_compressor(
          p, threshold_db, makeup_gain, ratio, knee, attack_ms, release_ms
        ),
        p
      )

  @doc "Enable or disable output dithering + noise shaping."
  @spec set_dither(t(), boolean()) :: t()
  def set_dither(p, enabled), do: then_self(:rockbox_ffi_nif.player_set_dither(p, enabled), p)

  @doc "Pitch/speed ratio (`10000` = normal); pitch and tempo shift together."
  @spec set_pitch(t(), integer()) :: t()
  def set_pitch(p, ratio), do: then_self(:rockbox_ffi_nif.player_set_pitch(p, ratio), p)

  @doc "The full DSP-chain state as an atom-keyed map."
  @spec dsp_settings(t()) :: map()
  def dsp_settings(p), do: Rockbox.decode_json(:rockbox_ffi_nif.player_dsp_settings_json(p))

  # Run a NIF call for its side effect, then return the player handle so
  # mutating/command functions stay pipe-friendly.
  defp then_self(_ignored, p), do: p
end
