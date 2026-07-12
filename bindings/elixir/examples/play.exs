# Play an audio file through the real output device.
#
# Run: mix run examples/play.exs [path-to-audio]

fixture =
  Path.expand(
    "../../../crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a",
    __DIR__
  )

file =
  case System.argv() do
    [path | _] -> path
    [] -> fixture
  end

p = Rockbox.Player.new(volume: 0.8)

# Mutating Player functions return the handle, so setup pipes cleanly.
# DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift.
p
|> Rockbox.Player.set_queue([file])
|> Rockbox.Player.set_eq_preset(:bass_boost)
|> Rockbox.Player.set_bass(7)
|> Rockbox.Player.set_treble(4)
|> Rockbox.Player.play()
IO.puts("▶ playing #{file}")
IO.puts("eq: BassBoost preset, bass +7 dB, treble +4 dB")

# Poll status until playback finishes (state returns to "stopped").
#
# The player handle is a NIF resource freed by the BEAM garbage collector
# (which stops playback) — no explicit close is needed. Ctrl-C twice opens
# the BEAM break menu, which halts the VM and the output device with it.
poll = fn poll ->
  st = Rockbox.Player.status(p)
  pos = :erlang.float_to_binary(st.position_ms / 1000, decimals: 1)
  dur = :erlang.float_to_binary(st.duration_ms / 1000, decimals: 1)
  IO.write("\r[#{st.state}] #{pos}s / #{dur}s   ")

  if st.state == "stopped" and st.position_ms > 0 do
    IO.puts("\n✔ done")
  else
    Process.sleep(500)
    poll.(poll)
  end
end

poll.(poll)
