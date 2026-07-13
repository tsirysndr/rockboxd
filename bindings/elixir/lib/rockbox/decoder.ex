defmodule Rockbox.Decoder do
  @moduledoc """
  Decode an audio file to interleaved-stereo S16LE PCM, one chunk at a time.

  Wraps the Rockbox codec engine. The codec state is process-wide, so only one
  decoder may decode at a time — opening a second one blocks until the first is
  freed. The handle is a NIF resource, freed by the BEAM garbage collector (no
  explicit close needed).
  """

  @typedoc "Opaque decoder handle (a NIF resource)."
  @opaque t :: reference()

  @doc """
  Open a decoder for the local audio file at `path`. Returns the handle, or
  `nil` if the file cannot be opened / no codec recognises it.
  """
  @spec open(Path.t()) :: t() | nil
  def open(path), do: :rockbox_ffi_nif.decoder_open(to_bin(path))

  @doc """
  Tags and stream properties (same shape as `Rockbox.Metadata.read/1`).

  Returns a map with atom keys (`:codec`, `:title`, `:duration_ms`,
  `:sample_rate`, a nested `:replaygain` map, …).
  """
  @spec metadata(t()) :: map()
  def metadata(d), do: Rockbox.decode_json(:rockbox_ffi_nif.decoder_metadata_json(d))

  @doc """
  Next buffer of decoded PCM as `{samples, sample_rate}`, or `:eof` at end of
  track (or after a codec error — see `finished/1`).

  `samples` is a little-endian int16 binary of interleaved stereo PCM (use
  `Rockbox.Dsp.binary_to_samples/1` to convert it to a list).
  """
  @spec next_chunk(t()) :: {binary(), pos_integer()} | :eof
  def next_chunk(d) do
    case :rockbox_ffi_nif.decoder_next_chunk(d) do
      nil -> :eof
      {samples, sample_rate} -> {samples, sample_rate}
    end
  end

  @doc "Request a seek to `ms` milliseconds."
  @spec seek_ms(t(), non_neg_integer()) :: :ok
  def seek_ms(d, ms), do: nilok(:rockbox_ffi_nif.decoder_seek_ms(d, ms))

  @doc "Playback position last reported by the codec, in milliseconds."
  @spec elapsed_ms(t()) :: non_neg_integer()
  def elapsed_ms(d), do: :rockbox_ffi_nif.decoder_elapsed_ms(d)

  @doc """
  `{done, code}`: `done` is `true` once the track has ended; `code` is the exit
  status (`0` = clean end, negative = codec error), valid only when `done`.
  """
  @spec finished(t()) :: {boolean(), integer()}
  def finished(d), do: :rockbox_ffi_nif.decoder_finished(d)

  defp to_bin(p), do: IO.iodata_to_binary([p])

  defp nilok(nil), do: :ok
  defp nilok(other), do: other
end
