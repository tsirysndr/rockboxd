defmodule Rockbox do
  @moduledoc """
  Elixir bindings for the Rockbox DSP / metadata / playback engine.

  Thin wrappers over the shared `:rockbox_ffi_nif` NIF (see `c_src/` and
  `src/`). The interesting modules are `Rockbox.Metadata`, `Rockbox.Dsp`, and
  `Rockbox.Player`.
  """

  @doc "ABI major version of the loaded native library."
  @spec abi_version() :: non_neg_integer()
  def abi_version, do: :rockbox_ffi_nif.abi_version()

  @doc """
  Peek at a resume file (an .m3u8 saved by a player) without a live player.

  Returns `{:ok, %{tracks: [...], index: i, elapsed_ms: ms}}` or
  `{:error, :absent}`.
  """
  @spec load_resume(Path.t()) :: {:ok, map()} | {:error, :absent}
  def load_resume(path) do
    case :rockbox_ffi_nif.load_resume_json(to_bin(path)) do
      nil -> {:error, :absent}
      json when is_binary(json) -> {:ok, decode_json(json)}
    end
  end

  @doc """
  Parse an .m3u/.m3u8 playlist file into a list of entry maps with atom keys
  (`:path`, `:duration_ms`, `:title`). Returns `{:error, reason}` on failure.
  """
  @spec m3u_read(Path.t()) :: {:ok, [map()]} | {:error, term()}
  def m3u_read(path) do
    case :rockbox_ffi_nif.m3u_read_json(to_bin(path)) do
      nil -> {:error, :read_failed}
      json when is_binary(json) -> {:ok, decode_json(json)}
    end
  end

  @doc "Write a list of paths/URLs to `path` as an .m3u8 file (atomic write)."
  @spec m3u_write(Path.t(), [Path.t()]) :: :ok | {:error, :write_failed}
  def m3u_write(path, paths) do
    json = :json.encode(Enum.map(paths, &IO.iodata_to_binary([&1])))

    case :rockbox_ffi_nif.m3u_write_json(to_bin(path), IO.iodata_to_binary(json)) do
      0 -> :ok
      _ -> {:error, :write_failed}
    end
  end

  @doc "Whether a string looks like an `http(s)://` URL."
  @spec is_url?(String.t()) :: boolean()
  def is_url?(s), do: :rockbox_ffi_nif.is_url(to_bin(s))

  defp to_bin(p), do: IO.iodata_to_binary([p])

  @doc false
  # Decode a JSON binary using OTP 27+'s built-in :json module (keys as
  # binaries), turning them into atom-keyed maps one level deep for ergonomics.
  def decode_json(bin) when is_binary(bin) do
    bin
    |> :json.decode()
    |> atomize()
  end

  defp atomize(map) when is_map(map) do
    Map.new(map, fn {k, v} -> {String.to_atom(k), atomize(v)} end)
  end

  defp atomize(list) when is_list(list), do: Enum.map(list, &atomize/1)
  defp atomize(other), do: other
end
