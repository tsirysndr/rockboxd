defmodule Rockbox.EqPreset do
  @moduledoc """
  Built-in equalizer presets for `Rockbox.Player.set_eq_preset/2`.

  These mirror the C ABI `EqPreset::ALL` order:

  | Atom              | Value |
  | ----------------- | ----- |
  | `:flat`           | 0     |
  | `:acoustic`       | 1     |
  | `:bass_boost`     | 2     |
  | `:bass_reducer`   | 3     |
  | `:classical`      | 4     |
  | `:dance`          | 5     |
  | `:deep`           | 6     |
  | `:electronic`     | 7     |
  | `:hip_hop`        | 8     |
  | `:jazz`           | 9     |
  | `:latin`          | 10    |
  | `:loudness`       | 11    |
  | `:lounge`         | 12    |
  | `:piano`          | 13    |
  | `:pop`            | 14    |
  | `:rnb`            | 15    |
  | `:rock`           | 16    |
  | `:small_speakers` | 17    |
  | `:treble_boost`   | 18    |
  | `:treble_reducer` | 19    |
  | `:vocal_boost`    | 20    |

  `to_int/1` accepts either the atom or the raw integer, so callers can pass
  whichever they prefer.
  """

  @type t ::
          :flat
          | :acoustic
          | :bass_boost
          | :bass_reducer
          | :classical
          | :dance
          | :deep
          | :electronic
          | :hip_hop
          | :jazz
          | :latin
          | :loudness
          | :lounge
          | :piano
          | :pop
          | :rnb
          | :rock
          | :small_speakers
          | :treble_boost
          | :treble_reducer
          | :vocal_boost

  @codes %{
    flat: 0,
    acoustic: 1,
    bass_boost: 2,
    bass_reducer: 3,
    classical: 4,
    dance: 5,
    deep: 6,
    electronic: 7,
    hip_hop: 8,
    jazz: 9,
    latin: 10,
    loudness: 11,
    lounge: 12,
    piano: 13,
    pop: 14,
    rnb: 15,
    rock: 16,
    small_speakers: 17,
    treble_boost: 18,
    treble_reducer: 19,
    vocal_boost: 20
  }

  @doc "Numeric code for an EQ preset (atom or already an integer)."
  @spec to_int(t() | 0..20) :: 0..20
  def to_int(preset) when is_integer(preset) and preset in 0..20, do: preset

  def to_int(preset) when is_atom(preset) do
    Map.get(@codes, preset) ||
      raise ArgumentError, "unknown eq preset: #{inspect(preset)}"
  end
end
