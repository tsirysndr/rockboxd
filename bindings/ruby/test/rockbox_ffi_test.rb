# frozen_string_literal: true

require "minitest/autorun"
require "tmpdir"
require "fileutils"
require "rockbox_ffi"

class RockboxFFITest < Minitest::Test
  FIXTURE = File.expand_path(
    "../../../crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a", __dir__
  )

  def test_abi_version
    assert_equal 1, RockboxFFI.abi_version
  end

  def test_metadata_read_and_probe
    meta = RockboxFFI::Metadata.read(FIXTURE)
    assert_equal "AAC", meta[:codec]
    assert_equal 44_100, meta[:sample_rate]
    assert_operator meta[:duration_ms], :>, 0
    assert_kind_of Hash, meta[:replaygain]

    assert_equal "FLAC", RockboxFFI::Metadata.probe("song.flac")
    assert_nil RockboxFFI::Metadata.probe("nope.xyz")
  end

  def test_dsp_replaygain_halves_amplitude
    RockboxFFI::Dsp.open(44_100) do |dsp|
      dsp.set_replaygain(RockboxFFI::DspReplayGainMode::TRACK, false, 0.0)
      dsp.set_replaygain_gains(track_gain_db: -6.0206) # x0.5

      sine = RockboxFFI.sine_stereo(1000, 1.0, 44_100, 16_000)
      peak = dsp.process(sine).map(&:abs).max
      assert_includes 7_600..8_400, peak, "expected ~8000, got #{peak}"
    end
  end

  def test_player_construct_and_status
    RockboxFFI::Player.open(volume: 0.0) do |player|
      assert_operator player.sample_rate, :>, 0

      player.set_volume(0.0)
      player.set_queue([FIXTURE])
      sleep 0.1 # queue command is applied asynchronously by the engine thread

      status = player.status
      assert_equal "stopped", status[:state]
      assert_equal 1, status[:queue_len]
    end
  end

  def test_player_queue_and_insert
    RockboxFFI::Player.open(volume: 0.0) do |player|
      player.set_queue([FIXTURE])
      sleep 0.1
      assert_equal [FIXTURE], player.queue

      player.insert(FIXTURE, RockboxFFI::InsertPosition::INSERT_LAST)
      sleep 0.1
      assert_equal 2, player.queue.length
    end
  end

  def test_player_export_and_load_m3u
    dir = Dir.mktmpdir
    m3u = File.join(dir, "queue.m3u8")
    RockboxFFI::Player.open(volume: 0.0) do |player|
      player.set_queue([FIXTURE])
      sleep 0.1
      assert player.export_m3u(m3u)
      assert_path_exists m3u

      loaded = player.load_m3u(m3u)
      assert_equal [FIXTURE], loaded
    end
  ensure
    FileUtils.remove_entry(dir) if dir
  end

  def test_module_m3u_read_write_and_resume
    dir = Dir.mktmpdir
    m3u = File.join(dir, "list.m3u8")
    assert RockboxFFI.m3u_write(m3u, [FIXTURE])
    assert_path_exists m3u

    entries = RockboxFFI.m3u_read(m3u)
    assert_kind_of Array, entries
    assert_equal FIXTURE, entries.first[:path]

    # No resume file written yet -> nil.
    assert_nil RockboxFFI.load_resume(File.join(dir, "nope.m3u8"))
  ensure
    FileUtils.remove_entry(dir) if dir
  end

  def test_is_url
    assert RockboxFFI.is_url?("http://example.com/stream.mp3")
    assert RockboxFFI.is_url?("https://example.com/stream.mp3")
    refute RockboxFFI.is_url?("/local/path/song.flac")
  end
end
