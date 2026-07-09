defmodule Rockbox.Metadata do
  @moduledoc "Audio file metadata / tag parsing."

  @doc """
  Parse the metadata of the audio file at `path`.

  Returns `{:ok, map}` with atom keys (`:codec`, `:title`, `:duration_ms`,
  `:sample_rate`, a nested `:replaygain` map, …) or `{:error, reason}`.
  """
  @spec read(Path.t()) :: {:ok, map()} | {:error, term()}
  def read(path) do
    case :rockbox_ffi_nif.meta_read_json(to_bin(path)) do
      nil -> {:error, :parse_failed}
      json when is_binary(json) -> {:ok, Rockbox.decode_json(json)}
    end
  end

  @doc "Same as `read/1` but raises on error."
  @spec read!(Path.t()) :: map()
  def read!(path) do
    case read(path) do
      {:ok, meta} -> meta
      {:error, reason} -> raise "rockbox metadata read failed: #{inspect(reason)}"
    end
  end

  @doc """
  Guess the codec label (e.g. `"FLAC"`) from a filename's extension without
  opening the file. Returns `nil` for an unknown extension.
  """
  @spec probe(String.t()) :: String.t() | nil
  def probe(filename) do
    case :rockbox_ffi_nif.meta_probe(to_bin(filename)) do
      nil -> nil
      label when is_binary(label) -> label
    end
  end

  defp to_bin(p), do: IO.iodata_to_binary([p])
end
