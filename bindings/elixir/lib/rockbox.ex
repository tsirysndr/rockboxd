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
