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

  defp nilok(nil), do: :ok
  defp nilok(other), do: other
end
