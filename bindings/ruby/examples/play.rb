# frozen_string_literal: true

# Play an audio file through the real output device.
#
# Run: ruby -Ilib examples/play.rb [path-to-audio]

$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "rockbox_ffi"

REPO = File.expand_path("../../..", __dir__)
FIXTURE = File.join(REPO, "crates", "rocksky", "fixtures", "08 - Internet Money - Speak(Explicit).m4a")

file = ARGV[0] || FIXTURE

player = RockboxFFI::Player.new(volume: 0.8)
player.set_queue([file])
# DSP: Bass Boost preset + a +3 dB bass/treble lift.
player.set_eq_preset(RockboxFFI::EqPreset::BASS_BOOST)
player.set_bass(3)
player.set_treble(3)
player.play
puts "▶ playing #{file}"
puts "eq: BassBoost preset, bass +3 dB, treble +3 dB"

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
