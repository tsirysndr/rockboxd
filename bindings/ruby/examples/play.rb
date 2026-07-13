# frozen_string_literal: true

# Play a local audio file or an http(s):// URL through the real output device.
#
# Run: ruby -Ilib examples/play.rb [path-to-audio | http(s)-url]

$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "rockbox_ffi"

REPO = File.expand_path("../../..", __dir__)
FIXTURE = File.join(REPO, "crates", "rocksky", "fixtures", "08 - Internet Money - Speak(Explicit).m4a")

# The queue accepts local files and http(s):// URLs (remote media / live radio).
file = ARGV[0] || FIXTURE
if !RockboxFFI.is_url?(file) && !File.exist?(file)
  abort "no such file: #{file}"
end

player = RockboxFFI::Player.new(volume: 0.8)
# Mutating setters return self, so the setup reads as one fluent chain.
# DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift.
player
  .set_queue([file])
  .set_eq_preset(RockboxFFI::EqPreset::BASS_BOOST)
  .set_bass(7)
  .set_treble(4)
  .play
puts "▶ playing #{file}"
puts "eq: BassBoost preset, bass +7 dB, treble +4 dB"

# Reinstall a SIGINT handler AFTER the player boots: the native audio engine
# installs its own signal handler while starting the output device, which
# otherwise swallows Ctrl-C. We exit! straight away instead of calling
# player.stop/close — those are blocking native calls that can deadlock
# against the engine thread. The OS reclaims the output device on exit.
trap("INT") do
  puts "\nstopped"
  exit!(130)
end

# Poll status until playback finishes (state returns to "stopped").
loop do
  st = player.status
  pos = st[:position_ms] / 1000.0
  dur = st[:duration_ms] / 1000.0
  printf("\r[%s] %.1fs / %.1fs   ", st[:state], pos, dur)
  $stdout.flush

  if st[:state] == "stopped" && st[:position_ms].positive?
    puts "\n✔ done"
    break
  end
  sleep 0.5
end

exit!(0)
