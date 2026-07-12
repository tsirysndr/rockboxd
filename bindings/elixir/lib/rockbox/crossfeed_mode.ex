defmodule Rockbox.CrossfeedMode do
  @moduledoc """
  Headphone crossfeed modes for `Rockbox.Player.set_crossfeed/6`.

  These mirror the C ABI enum:

  | Atom      | Value | Meaning                                              |
  | --------- | ----- | ---------------------------------------------------- |
  | `:off`    | 0     | Crossfeed disabled                                   |
  | `:meier`  | 1     | Meier preset (direct/cross/high-frequency defaults)  |
  | `:custom` | 2     | Custom direct/cross/high-frequency gains and cutoff  |

  `to_int/1` accepts either the atom or the raw integer, so callers can pass
  whichever they prefer.
  """

  @type t :: :off | :meier | :custom

  @codes %{
    off: 0,
    meier: 1,
    custom: 2
  }

  @doc "Numeric code for a crossfeed mode (atom or already an integer)."
  @spec to_int(t() | 0..2) :: 0..2
  def to_int(mode) when is_integer(mode) and mode in 0..2, do: mode

  def to_int(mode) when is_atom(mode) do
    Map.get(@codes, mode) ||
      raise ArgumentError, "unknown crossfeed mode: #{inspect(mode)}"
  end
end
