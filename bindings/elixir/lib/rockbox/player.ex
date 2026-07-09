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
    mix_mode: 0
  }

  @doc "Create a player on the default device with default settings."
  @spec new() :: t() | nil
  def new, do: :rockbox_ffi_nif.player_new()

  @doc """
  Create a player with configuration overrides (see `@default_config` keys).
  `sample_rate: 0` means the device default.
  """
  @spec new(keyword() | map()) :: t() | nil
  def new(opts) do
    c = Map.merge(@default_config, Map.new(opts))

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
  end

  @doc "Replace the queue with a list of file paths."
  @spec set_queue(t(), [Path.t()]) :: :ok
  def set_queue(p, paths) do
    json = :json.encode(Enum.map(paths, &IO.iodata_to_binary([&1])))
    nilok(:rockbox_ffi_nif.player_set_queue_json(p, IO.iodata_to_binary(json)))
  end

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

  defp nilok(nil), do: :ok
  defp nilok(other), do: other
end
