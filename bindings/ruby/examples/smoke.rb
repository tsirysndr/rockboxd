# frozen_string_literal: true

# End-to-end smoke test for the Ruby bindings.
#
# Run: ruby -Ilib examples/smoke.rb

$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "rockbox_ffi"

REPO = File.expand_path("../../..", __dir__)
FIXTURE = File.join(REPO, "crates", "rocksky", "fixtures", "08 - Internet Money - Speak(Explicit).m4a")

puts "ABI version: #{RockboxFFI.abi_version}"

# -- metadata -----------------------------------------------------------
meta = RockboxFFI::Metadata.read(FIXTURE)
puts "codec=#{meta[:codec].inspect} title=#{meta[:title].inspect} " \
     "artist=#{meta[:artist].inspect} duration_ms=#{meta[:duration_ms]} " \
     "sample_rate=#{meta[:sample_rate]}"
raise "codec label should not be empty" if meta[:codec].to_s.empty?

probe = RockboxFFI::Metadata.probe("song.flac")
puts "probe('song.flac') = #{probe.inspect}"
raise "expected FLAC, got #{probe.inspect}" unless probe == "FLAC"

# -- DSP: -6.0206 dB track gain should HALVE amplitude ------------------
rate = 44_100
RockboxFFI::Dsp.open(rate) do |dsp|
  dsp.set_replaygain(RockboxFFI::DspReplayGainMode::TRACK, false, 0.0)
  dsp.set_replaygain_gains(track_gain_db: -6.0206) # x0.5

  sine = RockboxFFI.sine_stereo(1000, 1.0, rate, 16_000)
  out = dsp.process(sine)
  peak = out.map(&:abs).max
  puts "DSP peak after -6.02 dB track gain: #{peak} (expected ~8000)"
  raise "peak #{peak} out of range" unless (7_600..8_400).cover?(peak)
end

# -- Player (construct only, no audible playback) -----------------------
RockboxFFI::Player.open(volume: 0.0) do |player|
  sr = player.sample_rate
  puts "Player sample_rate=#{sr}"
  raise "sample_rate should be > 0" unless sr.positive?

  player.set_volume(0.0)
  player.set_queue([FIXTURE])
  sleep 0.1 # the queue command is applied asynchronously by the engine thread

  st = player.status
  puts "Player status: state=#{st[:state].inspect} queue_len=#{st[:queue_len]}"
  raise "expected stopped, got #{st[:state].inspect}" unless st[:state] == "stopped"
  raise "expected queue_len 1, got #{st[:queue_len]}" unless st[:queue_len] == 1
end

puts "\nALL CHECKS PASSED ✔"
