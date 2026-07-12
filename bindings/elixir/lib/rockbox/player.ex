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
  @spec set_queue(t(), [Path.t()]) :: :ok
  def set_queue(p, paths) do
    json = :json.encode(Enum.map(paths, &IO.iodata_to_binary([&1])))
    nilok(:rockbox_ffi_nif.player_set_queue_json(p, IO.iodata_to_binary(json)))
  end

  @doc """
  Append one track to the queue. `path` may be a local file path, an
  `http(s)://` URL to a finite remote file, or a live-radio / streaming URL.
  """
  @spec enqueue(t(), Path.t()) :: :ok
  def enqueue(p, path), do: nilok(:rockbox_ffi_nif.player_enqueue(p, IO.iodata_to_binary([path])))

  @spec play(t()) :: :ok
  def play(p), do: nilok(:rockbox_ffi_nif.player_play(p))
  @spec pause(t()) :: :ok
  def pause(p), do: nilok(:rockbox_ffi_nif.player_pause(p))
  @spec toggle(t()) :: :ok
  def toggle(p), do: nilok(:rockbox_ffi_nif.player_toggle(p))
  @spec stop(t()) :: :ok
  def stop(p), do: nilok(:rockbox_ffi_nif.player_stop(p))
  @spec next(t()) :: :ok
  def next(p), do: nilok(:rockbox_ffi_nif.player_next(p))
  @spec previous(t()) :: :ok
  def previous(p), do: nilok(:rockbox_ffi_nif.player_previous(p))
  @spec skip_to(t(), non_neg_integer()) :: :ok
  def skip_to(p, index), do: nilok(:rockbox_ffi_nif.player_skip_to(p, index))
  @spec seek_ms(t(), non_neg_integer()) :: :ok
  def seek_ms(p, ms), do: nilok(:rockbox_ffi_nif.player_seek_ms(p, ms))

  @spec set_volume(t(), number()) :: :ok
  def set_volume(p, vol), do: nilok(:rockbox_ffi_nif.player_set_volume(p, vol / 1))

  @spec volume(t()) :: float()
  def volume(p), do: :rockbox_ffi_nif.player_volume(p)

  @spec sample_rate(t()) :: non_neg_integer()
  def sample_rate(p), do: :rockbox_ffi_nif.player_sample_rate(p)

  @spec set_crossfade(t(), 0..5, integer(), integer(), integer(), integer(), 0..1) :: :ok
  def set_crossfade(p, mode, fo_delay_ms, fo_dur_ms, fi_delay_ms, fi_dur_ms, mix_mode),
    do:
      nilok(
        :rockbox_ffi_nif.player_set_crossfade(
          p, mode, fo_delay_ms, fo_dur_ms, fi_delay_ms, fi_dur_ms, mix_mode
        )
      )

  @spec set_replaygain(t(), 0..2, number(), boolean()) :: :ok
  def set_replaygain(p, mode, preamp_db, prevent_clipping),
    do: nilok(:rockbox_ffi_nif.player_set_replaygain(p, mode, preamp_db / 1, prevent_clipping))

  @doc "A snapshot of the player's status as an atom-keyed map."
  @spec status(t()) :: map()
  def status(p), do: Rockbox.decode_json(:rockbox_ffi_nif.player_status_json(p))

  @doc """
  Insert file paths / URLs into the queue at `position`.

  `position` is a `Rockbox.InsertPosition` atom (or its integer code); `index`
  is only used when `position` is `:index` (7).
  """
  @spec insert(t(), [Path.t()], Rockbox.InsertPosition.t() | 0..7, non_neg_integer()) :: :ok
  def insert(p, paths, position, index \\ 0) do
    json = :json.encode(Enum.map(paths, &IO.iodata_to_binary([&1])))

    nilok(
      :rockbox_ffi_nif.player_insert_json(
        p,
        IO.iodata_to_binary(json),
        Rockbox.InsertPosition.to_int(position),
        index
      )
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

  @doc "Persist the queue + exact position to the resume file now."
  @spec save_resume(t()) :: :ok
  def save_resume(p), do: nilok(:rockbox_ffi_nif.player_save_resume(p))

  @doc "Delete the resume file (forget the saved state)."
  @spec clear_resume(t()) :: :ok
  def clear_resume(p), do: nilok(:rockbox_ffi_nif.player_clear_resume(p))

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
  @spec set_eq_enabled(t(), boolean()) :: :ok
  def set_eq_enabled(p, enabled),
    do: nilok(:rockbox_ffi_nif.player_set_eq_enabled(p, enabled))

  @doc "Whether the equalizer is currently enabled."
  @spec eq_enabled?(t()) :: boolean()
  def eq_enabled?(p), do: :rockbox_ffi_nif.player_is_eq_enabled(p)

  @doc """
  Configure one EQ band (0..10). `cutoff_hz` in Hz, `q` a Q factor, `gain_db`
  in dB.
  """
  @spec set_eq_band(t(), 0..10, integer(), number(), number()) :: :ok
  def set_eq_band(p, band, cutoff_hz, q, gain_db),
    do: nilok(:rockbox_ffi_nif.player_set_eq_band(p, band, cutoff_hz, q / 1, gain_db / 1))

  @doc "Global EQ pre-cut in dB (attenuation applied before the bands)."
  @spec set_eq_precut(t(), number()) :: :ok
  def set_eq_precut(p, db), do: nilok(:rockbox_ffi_nif.player_set_eq_precut(p, db / 1))

  @doc """
  Apply a built-in EQ preset. Accepts a `Rockbox.EqPreset` atom (e.g. `:rock`)
  or its integer code.
  """
  @spec set_eq_preset(t(), Rockbox.EqPreset.t() | 0..20) :: :ok
  def set_eq_preset(p, preset),
    do: nilok(:rockbox_ffi_nif.player_set_eq_preset(p, Rockbox.EqPreset.to_int(preset)))

  @doc """
  Bass/treble tone controls in dB, with band-split cutoffs in Hz (`0` = the
  native defaults).
  """
  @spec set_tone(t(), integer(), integer(), integer(), integer()) :: :ok
  def set_tone(p, bass_db, treble_db, bass_cutoff_hz, treble_cutoff_hz),
    do:
      nilok(
        :rockbox_ffi_nif.player_set_tone(
          p, bass_db, treble_db, bass_cutoff_hz, treble_cutoff_hz
        )
      )

  @doc "Set the bass tone axis in dB, leaving treble and cutoffs unchanged."
  @spec set_bass(t(), integer()) :: :ok
  def set_bass(p, bass_db), do: nilok(:rockbox_ffi_nif.player_set_bass(p, bass_db))

  @doc "Set the treble tone axis in dB, leaving bass and cutoffs unchanged."
  @spec set_treble(t(), integer()) :: :ok
  def set_treble(p, treble_db), do: nilok(:rockbox_ffi_nif.player_set_treble(p, treble_db))

  @doc """
  Haas surround effect: `delay_ms` (`0` = off), `balance` %, and the band-split
  cutoffs in Hz.
  """
  @spec set_surround(t(), integer(), integer(), integer(), integer()) :: :ok
  def set_surround(p, delay_ms, balance, cutoff_low_hz, cutoff_high_hz),
    do:
      nilok(
        :rockbox_ffi_nif.player_set_surround(
          p, delay_ms, balance, cutoff_low_hz, cutoff_high_hz
        )
      )

  @doc """
  Channel mixing mode. Accepts a `Rockbox.ChannelMode` atom (e.g. `:mono`) or
  its integer code.
  """
  @spec set_channel_mode(t(), Rockbox.ChannelMode.t() | 0..6) :: :ok
  def set_channel_mode(p, mode),
    do: nilok(:rockbox_ffi_nif.player_set_channel_mode(p, Rockbox.ChannelMode.to_int(mode)))

  @doc "Custom stereo width in percent (audible with channel mode `:custom`)."
  @spec set_stereo_width(t(), integer()) :: :ok
  def set_stereo_width(p, percent),
    do: nilok(:rockbox_ffi_nif.player_set_stereo_width(p, percent))

  @doc """
  Dynamic-range compressor. `threshold_db` `0` disables it; the remaining
  arguments are makeup gain, ratio, knee, attack (ms) and release (ms).
  """
  @spec set_compressor(t(), integer(), integer(), integer(), integer(), integer(), integer()) ::
          :ok
  def set_compressor(p, threshold_db, makeup_gain, ratio, knee, attack_ms, release_ms),
    do:
      nilok(
        :rockbox_ffi_nif.player_set_compressor(
          p, threshold_db, makeup_gain, ratio, knee, attack_ms, release_ms
        )
      )

  @doc "Enable or disable output dithering + noise shaping."
  @spec set_dither(t(), boolean()) :: :ok
  def set_dither(p, enabled), do: nilok(:rockbox_ffi_nif.player_set_dither(p, enabled))

  @doc "Pitch/speed ratio (`10000` = normal); pitch and tempo shift together."
  @spec set_pitch(t(), integer()) :: :ok
  def set_pitch(p, ratio), do: nilok(:rockbox_ffi_nif.player_set_pitch(p, ratio))

  @doc "The full DSP-chain state as an atom-keyed map."
  @spec dsp_settings(t()) :: map()
  def dsp_settings(p), do: Rockbox.decode_json(:rockbox_ffi_nif.player_dsp_settings_json(p))

  defp nilok(nil), do: :ok
  defp nilok(other), do: other
end
