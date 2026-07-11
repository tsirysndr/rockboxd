defmodule Rockbox.InsertPosition do
  @moduledoc """
  Insert-position codes for `Rockbox.Player.insert/4` and
  `Rockbox.Player.import_m3u/4`.

  These mirror the C ABI enum:

  | Atom                    | Value | Meaning                                  |
  | ----------------------- | ----- | ---------------------------------------- |
  | `:prepend`              | 0     | Before everything                        |
  | `:insert`              | 1     | Block right after the current track      |
  | `:insert_next`          | 2     | Immediately after the current track      |
  | `:insert_last`          | 3     | At the end of the queue                  |
  | `:insert_shuffled`      | 4     | At a random position                     |
  | `:insert_last_shuffled` | 5     | Shuffled into the tail                   |
  | `:replace`              | 6     | Replace the whole queue                  |
  | `:index`                | 7     | At the explicit `index` argument         |

  `to_int/1` accepts either the atom or the raw integer, so callers can pass
  whichever they prefer.
  """

  @type t ::
          :prepend
          | :insert
          | :insert_next
          | :insert_last
          | :insert_shuffled
          | :insert_last_shuffled
          | :replace
          | :index

  @codes %{
    prepend: 0,
    insert: 1,
    insert_next: 2,
    insert_last: 3,
    insert_shuffled: 4,
    insert_last_shuffled: 5,
    replace: 6,
    index: 7
  }

  @doc "Numeric code for an insert position (atom or already an integer)."
  @spec to_int(t() | 0..7) :: 0..7
  def to_int(pos) when is_integer(pos) and pos in 0..7, do: pos

  def to_int(pos) when is_atom(pos) do
    Map.get(@codes, pos) ||
      raise ArgumentError, "unknown insert position: #{inspect(pos)}"
  end
end
