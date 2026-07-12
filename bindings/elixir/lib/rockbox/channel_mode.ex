defmodule Rockbox.ChannelMode do
  @moduledoc """
  Channel mixing modes for `Rockbox.Player.set_channel_mode/2`.

  These mirror the C ABI enum:

  | Atom          | Value | Meaning                                    |
  | ------------- | ----- | ------------------------------------------ |
  | `:stereo`     | 0     | Normal stereo                              |
  | `:mono`       | 1     | Both channels mixed to mono                |
  | `:custom`     | 2     | Custom stereo width (see `set_stereo_width/2`) |
  | `:mono_left`  | 3     | Left channel on both outputs               |
  | `:mono_right` | 4     | Right channel on both outputs              |
  | `:karaoke`    | 5     | Subtract to cancel center-panned vocals    |
  | `:swap`       | 6     | Swap left and right                        |

  `to_int/1` accepts either the atom or the raw integer, so callers can pass
  whichever they prefer.
  """

  @type t ::
          :stereo
          | :mono
          | :custom
          | :mono_left
          | :mono_right
          | :karaoke
          | :swap

  @codes %{
    stereo: 0,
    mono: 1,
    custom: 2,
    mono_left: 3,
    mono_right: 4,
    karaoke: 5,
    swap: 6
  }

  @doc "Numeric code for a channel mode (atom or already an integer)."
  @spec to_int(t() | 0..6) :: 0..6
  def to_int(mode) when is_integer(mode) and mode in 0..6, do: mode

  def to_int(mode) when is_atom(mode) do
    Map.get(@codes, mode) ||
      raise ArgumentError, "unknown channel mode: #{inspect(mode)}"
  end
end
