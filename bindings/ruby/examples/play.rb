# frozen_string_literal: true

# Play an audio source through the real output device.
#
# The queue entry can be a local file, a remote http(s):// URL, an
# internet-radio stream, or an HLS (.m3u8) / MPEG-DASH (.mpd) manifest —
# the engine detects each kind automatically.
#
# Run: ruby -Ilib examples/play.rb [path-or-URL]
#      ruby -Ilib examples/play.rb hls    # public HLS test stream
#      ruby -Ilib examples/play.rb dash   # public MPEG-DASH test stream

$LOAD_PATH.unshift File.expand_path("../lib", __dir__)
require "rockbox_ffi"

REPO = File.expand_path("../../..", __dir__)
FIXTURE = File.join(REPO, "crates", "rocksky", "fixtures", "08 - Internet Money - Speak(Explicit).m4a")

# Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
# for more).
HLS_DEFAULT = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"
DASH_DEFAULT = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd"

# The queue accepts local files, http(s):// URLs (remote media / live radio),
# and HLS / MPEG-DASH manifest URLs.
arg = ARGV[0] || FIXTURE
file = { "hls" => HLS_DEFAULT, "dash" => DASH_DEFAULT }.fetch(arg, arg)
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
# A live stream reports duration 0 and plays until Ctrl-C.
loop do
  st = player.status
  pos = st[:position_ms] / 1000.0
  dur = st[:duration_ms] / 1000.0
  clock = dur.zero? ? format("%.1fs / LIVE", pos) : format("%.1fs / %.1fs", pos, dur)
  # The codec label carries the protocol for adaptive streams (e.g. "HLS AAC").
  codec = st.dig(:metadata, :codec) || ""
  printf("\r[%s] %s %s   ", st[:state], codec, clock)
  $stdout.flush

  if st[:state] == "stopped" && st[:position_ms].positive?
    puts "\n✔ done"
    break
  end
  sleep 0.5
end

exit!(0)
