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

    p
    |> Rockbox.Player.set_volume(0.0)
    |> Rockbox.Player.set_queue([@fixture])

    # the queue command is applied asynchronously by the engine thread
    Process.sleep(100)

    status = Rockbox.Player.status(p)
    assert status.state == "stopped"
    assert status.queue_len == 1
  end

  test "queue insert and read back" do
    p = Rockbox.Player.new(volume: 0.0)

    p
    |> Rockbox.Player.set_queue([@fixture])
    |> Rockbox.Player.insert([@fixture], :insert_last)

    Process.sleep(100)

    q = Rockbox.Player.queue(p)
    assert is_list(q)
    assert length(q) == 2
    assert Enum.all?(q, &is_binary/1)
  end

  test "insert position atom maps to code" do
    assert Rockbox.InsertPosition.to_int(:prepend) == 0
    assert Rockbox.InsertPosition.to_int(:index) == 7
    assert Rockbox.InsertPosition.to_int(3) == 3
  end

  test "is_url? classifies strings" do
    assert Rockbox.is_url?("http://example.com/stream.mp3")
    assert Rockbox.is_url?("https://example.com/stream.mp3")
    refute Rockbox.is_url?("/local/path/song.flac")
  end

  test "m3u write then read round-trips" do
    path = Path.join(System.tmp_dir!(), "rockbox_test_#{System.unique_integer([:positive])}.m3u8")
    on_exit(fn -> File.rm(path) end)

    :ok = Rockbox.m3u_write(path, [@fixture])
    {:ok, entries} = Rockbox.m3u_read(path)
    assert is_list(entries)
    assert length(entries) == 1
    assert hd(entries).path =~ "Internet Money"
  end

  test "player export_m3u and load_m3u" do
    p = Rockbox.Player.new(volume: 0.0)
    Rockbox.Player.set_queue(p, [@fixture])
    Process.sleep(100)

    path = Path.join(System.tmp_dir!(), "rockbox_export_#{System.unique_integer([:positive])}.m3u8")
    on_exit(fn -> File.rm(path) end)

    :ok = Rockbox.Player.export_m3u(p, path)
    assert File.exists?(path)

    {:ok, loaded} = Rockbox.Player.load_m3u(p, path)
    assert is_list(loaded)
    assert length(loaded) == 1
  end

  test "resume config: save, load_resume, clear" do
    path = Path.join(System.tmp_dir!(), "rockbox_resume_#{System.unique_integer([:positive])}.m3u8")
    on_exit(fn -> File.rm(path) end)

    p = Rockbox.Player.new(volume: 0.0, resume_file: path)
    Rockbox.Player.set_queue(p, [@fixture])
    Process.sleep(100)

    Rockbox.Player.save_resume(p)
    # save_resume is processed asynchronously by the engine thread.
    Process.sleep(100)
    assert File.exists?(path)

    {:ok, state} = Rockbox.load_resume(path)
    assert is_list(state.tracks)
    assert is_integer(state.index)
    assert is_integer(state.elapsed_ms)

    Rockbox.Player.clear_resume(p)
    refute File.exists?(path)
  end
end
