// Play an audio source through the real output device.
//
// The queue entry can be a local file, a remote http(s):// file, an
// internet-radio stream, or an HLS (.m3u8) / MPEG-DASH (.mpd) manifest —
// the engine detects each kind automatically.
//
// Run: bun run examples/play.bun.ts [path-or-URL]
//      bun run examples/play.bun.ts hls    # public HLS test stream
//      bun run examples/play.bun.ts dash   # public MPEG-DASH test stream
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as api from "../src/bun.ts";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..", "..");
const fixture = join(
  repo, "crates", "rocksky", "fixtures",
  "08 - Internet Money - Speak(Explicit).m4a",
);

// Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
// for more).
const HLS_DEFAULT = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8";
const DASH_DEFAULT = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd";

const arg = process.argv[2] ?? fixture;
const file = arg === "hls" ? HLS_DEFAULT : arg === "dash" ? DASH_DEFAULT : arg;

// DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift. Setters return
// `this`, so the whole setup chains fluently into a single expression.
const player = new api.Player({ volume: 0.8 })
  .setQueue([file])
  .setEqPreset(api.EqPreset.BassBoost)
  .setBass(7)
  .setTreble(4)
  .play();

console.log(`▶ playing ${file}`);
console.log("eq: BassBoost preset, bass +7 dB, treble +4 dB");

// Poll status until playback finishes (state returns to "stopped").
// A live stream reports duration 0 and plays until Ctrl-C.
const timer = setInterval(() => {
  const st = player.status();
  const pos = (st.position_ms / 1000).toFixed(1);
  const dur = (st.duration_ms / 1000).toFixed(1);
  const clock = st.duration_ms === 0 ? `${pos}s / LIVE` : `${pos}s / ${dur}s`;
  // The codec label carries the protocol for adaptive streams (e.g. "HLS AAC").
  const codec = st.metadata?.codec ?? "";
  process.stdout.write(`\r[${st.state}] ${codec} ${clock}   `);
  if (st.state === "stopped" && st.position_ms > 0) {
    clearInterval(timer);
    console.log("\n✔ done");
    player.close();
    process.exit(0);
  }
}, 500);

// Clean shutdown on Ctrl-C. NOTE: do NOT call player.stop()/close() here —
// those are blocking native FFI calls that can deadlock against the audio
// engine thread and hang the process. Just exit; the OS reclaims the device.
process.on("SIGINT", () => {
  clearInterval(timer);
  console.log("\nstopped");
  process.exit(130);
});
