// Play an audio file through the real output device.
// Run: bun run examples/play.bun.ts [path-to-audio]
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import * as api from "../src/bun.ts";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..", "..");
const fixture = join(
  repo, "crates", "rocksky", "fixtures",
  "08 - Internet Money - Speak(Explicit).m4a",
);

const file = process.argv[2] ?? fixture;

// DSP: Bass Boost preset + a +3 dB bass/treble lift. Setters return `this`,
// so the whole setup chains fluently into a single expression.
const player = new api.Player({ volume: 0.8 })
  .setQueue([file])
  .setEqPreset(api.EqPreset.BassBoost)
  .setBass(3)
  .setTreble(3)
  .play();

console.log(`▶ playing ${file}`);
console.log("eq: BassBoost preset, bass +3 dB, treble +3 dB");

// Poll status until playback finishes (state returns to "stopped").
const timer = setInterval(() => {
  const st = player.status();
  const pos = (st.position_ms / 1000).toFixed(1);
  const dur = (st.duration_ms / 1000).toFixed(1);
  process.stdout.write(`\r[${st.state}] ${pos}s / ${dur}s   `);
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
