defmodule Rockbox.RepeatMode do
  @moduledoc """
  Repeat modes for `Rockbox.Player.set_repeat/2`.

  These mirror the C ABI enum:

  | Atom   | Value | Meaning                        |
  | ------ | ----- | ------------------------------ |
  | `:off` | 0     | No repeat                      |
  | `:one` | 1     | Repeat the current track       |
  | `:all` | 2     | Repeat the whole queue         |

  `to_int/1` accepts either the atom or the raw integer, so callers can pass
  whichever they prefer; `from_int/1` maps a numeric code back to its atom.
  """

  @type t :: :off | :one | :all

  @codes %{
    off: 0,
    one: 1,
    all: 2
  }

  @atoms %{
    0 => :off,
    1 => :one,
    2 => :all
  }

  @doc "Numeric code for a repeat mode (atom or already an integer)."
  @spec to_int(t() | 0..2) :: 0..2
  def to_int(mode) when is_integer(mode) and mode in 0..2, do: mode

  def to_int(mode) when is_atom(mode) do
    Map.get(@codes, mode) ||
      raise ArgumentError, "unknown repeat mode: #{inspect(mode)}"
  end

  @doc "Atom for a numeric repeat-mode code."
  @spec from_int(0..2) :: t()
  def from_int(code) when is_integer(code) do
    Map.get(@atoms, code) ||
      raise ArgumentError, "unknown repeat mode code: #{inspect(code)}"
  end
end
