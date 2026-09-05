# Play an audio source through the real output device.
#
# The queue entry can be a local file, a remote http(s):// file, an
# internet-radio stream, or an HLS (.m3u8) / MPEG-DASH (.mpd) manifest —
# the engine detects each kind automatically.
#
# Run: mix run examples/play.exs [path-or-URL]
#      mix run examples/play.exs hls    # public HLS test stream
#      mix run examples/play.exs dash   # public MPEG-DASH test stream

fixture =
  Path.expand(
    "../../../crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a",
    __DIR__
  )

# Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
# for more).
hls_default = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"
dash_default = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd"

file =
  case System.argv() do
    ["hls" | _] -> hls_default
    ["dash" | _] -> dash_default
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
# A live stream reports duration 0 and plays until Ctrl-C.
#
# The player handle is a NIF resource freed by the BEAM garbage collector
# (which stops playback) — no explicit close is needed. Ctrl-C twice opens
# the BEAM break menu, which halts the VM and the output device with it.
poll = fn poll ->
  st = Rockbox.Player.status(p)
  pos = :erlang.float_to_binary(st.position_ms / 1000, decimals: 1)

  clock =
    if st.duration_ms == 0 do
      "#{pos}s / LIVE"
    else
      dur = :erlang.float_to_binary(st.duration_ms / 1000, decimals: 1)
      "#{pos}s / #{dur}s"
    end

  IO.write("\r[#{st.state}] #{clock}   ")

  if st.state == "stopped" and st.position_ms > 0 do
    IO.puts("\n✔ done")
  else
    Process.sleep(500)
    poll.(poll)
  end
end

poll.(poll)
