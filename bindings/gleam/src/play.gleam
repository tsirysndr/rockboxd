//// Play an audio source through the real output device.
////
//// The queue entry can be a local file, a remote `http(s)://` file, an
//// internet-radio stream, or an HLS (`.m3u8`) / MPEG-DASH (`.mpd`)
//// manifest — the engine detects each kind automatically.
////
//// Run: gleam run -m play [path-or-URL]
////      gleam run -m play hls    # public HLS test stream
////      gleam run -m play dash   # public MPEG-DASH test stream
////
//// The `Player` handle is a NIF resource freed by the BEAM garbage collector
//// (which stops playback) — no explicit close is needed. Ctrl-C twice opens
//// the BEAM break menu, which halts the VM and the output device with it.

import gleam/int
import gleam/io
import gleam/list
import rockbox/player

const fixture = "../../crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a"

// Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
// for more).
const hls_default = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"

const dash_default = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd"

pub fn main() {
  let source = case list.map(plain_arguments(), charlist_to_string) {
    ["hls", ..] -> hls_default
    ["dash", ..] -> dash_default
    [path, ..] -> path
    [] -> fixture
  }

  let p = player.with_config(player.Config(..player.default_config(), volume: 0.8))
  // DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift.
  let p =
    p
    |> player.set_queue([source])
    |> player.set_eq_preset(player.BassBoost)
    |> player.set_bass(7)
    |> player.set_treble(4)
    |> player.play
  io.println("▶ playing " <> source)
  io.println("eq: BassBoost preset, bass +7 dB, treble +4 dB")

  poll(p)
}

// Poll status until playback finishes (state returns to "stopped").
// A live stream reports duration 0 and plays until Ctrl-C.
fn poll(p: player.Player) -> Nil {
  let st = player.status(p)
  let pos = int.to_string(st.position_ms / 1000)
  let clock = case st.duration_ms {
    0 -> pos <> "s / LIVE"
    _ -> pos <> "s / " <> int.to_string(st.duration_ms / 1000) <> "s"
  }
  io.print("\r[" <> st.state <> "] " <> clock <> "   ")

  case st.state == "stopped" && st.position_ms > 0 {
    True -> io.println("\n✔ done")
    False -> {
      sleep(500)
      poll(p)
    }
  }
}

@external(erlang, "timer", "sleep")
fn sleep(ms: Int) -> a

// CLI arguments arrive as Erlang charlists; convert each to a Gleam String.
type Charlist

@external(erlang, "init", "get_plain_arguments")
fn plain_arguments() -> List(Charlist)

@external(erlang, "unicode", "characters_to_binary")
fn charlist_to_string(chars: Charlist) -> String
