defmodule RockboxTest do
  use ExUnit.Case

  @fixture Path.expand(
             "../../../crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a",
             __DIR__
           )

  test "abi version" do
    assert Rockbox.abi_version() == 1
  end

  test "metadata read and probe" do
    {:ok, meta} = Rockbox.Metadata.read(@fixture)
    assert meta.codec == "AAC"
    assert meta.sample_rate == 44_100
    assert meta.duration_ms > 0
    assert is_map(meta.replaygain)

    assert Rockbox.Metadata.probe("song.flac") == "FLAC"
    assert Rockbox.Metadata.probe("nope.xyz") == nil
  end

  test "DSP -6.02 dB track gain halves amplitude" do
    d = Rockbox.Dsp.new(44_100)
    :ok = Rockbox.Dsp.set_replaygain(d, 0, false, 0.0)
    :ok = Rockbox.Dsp.set_replaygain_gains(d, -6.0206, nil, nil, nil)

    sine =
      for i <- 0..(44_100 - 1), into: <<>> do
        s = round(:math.sin(i * 2 * :math.pi() * 1000 / 44_100) * 16_000)
        <<s::16-little-signed, s::16-little-signed>>
      end

    out = Rockbox.Dsp.process(d, sine)
    peak = out |> Rockbox.Dsp.binary_to_samples() |> Enum.map(&abs/1) |> Enum.max()
    assert peak in 7_600..8_400, "expected ~8000, got #{peak}"
  end

  test "player construct, sample rate, status (no playback)" do
    p = Rockbox.Player.new(volume: 0.0)
    assert p != nil
    assert Rockbox.Player.sample_rate(p) > 0

    :ok = Rockbox.Player.set_volume(p, 0.0)
    :ok = Rockbox.Player.set_queue(p, [@fixture])
    # the queue command is applied asynchronously by the engine thread
    Process.sleep(100)

    status = Rockbox.Player.status(p)
    assert status.state == "stopped"
    assert status.queue_len == 1
  end
end
