//// Play an audio file through the real output device.
////
//// Run: gleam run -m play
////
//// The `Player` handle is a NIF resource freed by the BEAM garbage collector
//// (which stops playback) — no explicit close is needed. Ctrl-C twice opens
//// the BEAM break menu, which halts the VM and the output device with it.

import gleam/int
import gleam/io
import rockbox/player

const fixture = "../../crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a"

pub fn main() {
  let p = player.with_config(player.Config(..player.default_config(), volume: 0.8))
  player.set_queue(p, [fixture])
  player.play(p)
  io.println("▶ playing " <> fixture)

  poll(p)
}

// Poll status until playback finishes (state returns to "stopped").
fn poll(p: player.Player) -> Nil {
  let st = player.status(p)
  let pos = int.to_string(st.position_ms / 1000)
  let dur = int.to_string(st.duration_ms / 1000)
  io.print("\r[" <> st.state <> "] " <> pos <> "s / " <> dur <> "s   ")

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
