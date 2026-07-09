// Shared smoke-test body, run by both the Bun and Deno entry scripts.
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { sineStereo } from "../src/api.ts";
import { DspReplayGainMode } from "../src/enums.ts";

// Minimal shape of the runtime module both backends export.
interface Api {
  abiVersion: () => number;
  metadata: { read: (p: string) => any; probe: (n: string) => string | null };
  Dsp: new (rate: number) => any;
  Player: new (config?: any) => any;
}

export function runSmoke(api: Api, runtime: string): void {
  const here = dirname(fileURLToPath(import.meta.url));
  const repo = join(here, "..", "..", "..");
  const fixture = join(
    repo, "crates", "rocksky", "fixtures",
    "08 - Internet Money - Speak(Explicit).m4a",
  );

  console.log(`[${runtime}] ABI version: ${api.abiVersion()}`);

  // -- metadata -----------------------------------------------------------
  const meta = api.metadata.read(fixture);
  console.log(
    `[${runtime}] codec=${meta.codec} title=${JSON.stringify(meta.title)} ` +
      `artist=${JSON.stringify(meta.artist)} duration_ms=${meta.duration_ms} ` +
      `sample_rate=${meta.sample_rate}`,
  );
  if (!meta.codec) throw new Error("codec label should not be empty");

  const probe = api.metadata.probe("song.flac");
  console.log(`[${runtime}] probe('song.flac') = ${probe}`);
  if (probe !== "FLAC") throw new Error(`expected FLAC, got ${probe}`);

  // -- DSP: -6.0206 dB track gain halves amplitude ------------------------
  const rate = 44100;
  const dsp = new api.Dsp(rate);
  try {
    dsp.setReplaygain(DspReplayGainMode.TRACK, false, 0.0);
    dsp.setReplaygainGains(-6.0206); // x0.5
    const sine = sineStereo(1000, 1.0, rate, 16000);
    const out: Int16Array = dsp.process(sine);
    let peak = 0;
    for (const s of out) peak = Math.max(peak, Math.abs(s));
    console.log(`[${runtime}] DSP peak after -6.02 dB track gain: ${peak} (expected ~8000)`);
    if (peak < 7600 || peak > 8400) throw new Error(`peak ${peak} out of range`);
  } finally {
    dsp.close();
  }

  // -- Player (construct only, no audible playback) -----------------------
  const player = new api.Player({ volume: 0.0 });
  try {
    const sr = player.sampleRate();
    console.log(`[${runtime}] Player sample_rate=${sr}`);
    if (sr <= 0) throw new Error("sample_rate should be > 0");
    player.setVolume(0.0);
    player.setQueue([fixture]);
    const st = player.status();
    console.log(`[${runtime}] Player status: state=${st.state} queue_len=${st.queue_len}`);
    if (st.state !== "stopped") throw new Error(`expected stopped, got ${st.state}`);
    if (st.queue_len !== 1) throw new Error(`expected queue_len 1, got ${st.queue_len}`);
  } finally {
    player.close();
  }

  console.log(`[${runtime}] ALL CHECKS PASSED ✔`);
}
