import { useState } from "react";
import { RockboxPlayer, RepeatMode } from "rockbox-wasm";
import { useRockbox } from "./useRockbox";
import { DspPanel } from "./DspPanel";

const CUTOFFS = RockboxPlayer.EQ_BAND_CUTOFFS;
const REPEAT_CYCLE = [RepeatMode.Off, RepeatMode.One, RepeatMode.All];
const nextRepeat = (r: RepeatMode) =>
  REPEAT_CYCLE[(REPEAT_CYCLE.indexOf(r) + 1) % 3];

const fmt = (ms: number) => {
  const s = Math.max(0, Math.floor((ms || 0) / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

const btn =
  "rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm font-medium " +
  "text-zinc-100 transition hover:enabled:border-indigo-500 hover:enabled:bg-zinc-700 " +
  "disabled:cursor-not-allowed disabled:opacity-40";
const btnActive = "border-transparent bg-indigo-500 text-white hover:enabled:bg-indigo-400";

export function App() {
  const { ensureReady, getPlayer, ready, status, progress, track, error } =
    useRockbox();

  const [url, setUrl] = useState("");
  const [seeking, setSeeking] = useState<number | null>(null);
  const [volume, setVolume] = useState(100);
  const [eqEnabled, setEqEnabled] = useState(false);
  const [eqGains, setEqGains] = useState<number[]>(() => CUTOFFS.map(() => 0));

  const md = track?.metadata ?? null;
  const live = progress?.live ?? track?.live ?? false;
  const playing = status.state === "playing";
  const duration = progress?.duration_ms ?? 0;
  const elapsed = progress?.elapsed_ms ?? 0;

  // ── actions ───────────────────────────────────────────────────────────────
  const playUrl = async () => {
    if (!url.trim()) return;
    (await ensureReady()).setQueue([url.trim()], true);
  };
  const enqueue = async () => {
    if (!url.trim()) return;
    (await ensureReady()).enqueue(url.trim());
  };
  const toggle = async () => (await ensureReady()).toggle();

  const player = ready ? getPlayer() : null;

  const onSeek = (v: number) => {
    setSeeking(null);
    if (duration > 0) player?.seek((v / 1000) * duration);
  };
  const onVolume = (v: number) => {
    setVolume(v);
    player?.setVolume(v / 100);
  };
  const onEqEnabled = async (on: boolean) => {
    setEqEnabled(on);
    (await ensureReady()).setEqEnabled(on);
  };
  const onEqBand = async (i: number, gain: number) => {
    setEqGains((g) => g.map((x, j) => (j === i ? gain : x)));
    const p = await ensureReady();
    p.setEqBand(i, CUTOFFS[i], 1.0, gain);
    if (!eqEnabled) {
      setEqEnabled(true);
      p.setEqEnabled(true);
    }
  };

  const seekValue =
    seeking ?? (duration > 0 ? Math.round((elapsed / duration) * 1000) : 0);

  return (
    <div className="mx-auto max-w-2xl px-4 py-10 text-zinc-100">
      <header className="mb-6">
        <h1 className="bg-gradient-to-r from-indigo-400 to-violet-400 bg-clip-text text-3xl font-bold tracking-tight text-transparent">
          rockbox-wasm
        </h1>
        <p className="mt-1 text-sm text-zinc-400">
          Rockbox decoders + DSP in WebAssembly, driven from React.
        </p>
      </header>

      {error && (
        <div className="mb-4 rounded-lg border border-red-500/40 bg-red-500/10 px-4 py-2 text-sm text-red-300">
          ⚠ {error}
        </div>
      )}

      <section className="mb-4 rounded-2xl border border-zinc-800 bg-zinc-900 p-5 shadow-xl shadow-black/30">
        <div className="flex flex-wrap gap-2">
          <input
            type="url"
            placeholder="https://…/song.flac  or an Icecast radio URL"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && playUrl()}
            className="min-w-0 flex-1 rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm outline-none placeholder:text-zinc-500 focus:border-indigo-500"
          />
          <button onClick={playUrl} className={`${btn} ${btnActive}`}>
            Play
          </button>
          <button onClick={enqueue} className={btn}>
            + Queue
          </button>
        </div>

        <div className="mt-4">
          <div className="truncate text-lg font-semibold">
            {md?.title ?? (live ? (md?.station ?? "Live stream") : "—")}
          </div>
          <div className="truncate text-sm text-zinc-400">
            {live ? (md?.artist ?? "Live stream") : (md?.artist ?? "")}
          </div>
          <div className="mt-1 font-mono text-xs text-zinc-500">
            {[
              md?.codec?.toUpperCase(),
              md?.sample_rate && `${(md.sample_rate / 1000).toFixed(1)} kHz`,
              md?.bitrate && `${md.bitrate} kbps`,
            ]
              .filter(Boolean)
              .join("  ·  ")}
          </div>
        </div>

        <div className="mt-4 flex items-center gap-3">
          <span className="w-12 font-mono text-xs text-zinc-400">
            {fmt(elapsed)}
          </span>
          <input
            type="range"
            min={0}
            max={1000}
            value={seekValue}
            disabled={!ready || live}
            onChange={(e) => setSeeking(Number(e.target.value))}
            onMouseUp={(e) => onSeek(Number((e.target as HTMLInputElement).value))}
            onTouchEnd={(e) =>
              onSeek(Number((e.target as HTMLInputElement).value))
            }
            className="flex-1 accent-indigo-500 disabled:opacity-50"
          />
          <span
            className={
              "w-12 text-right font-mono text-xs " +
              (live ? "font-semibold text-red-400" : "text-zinc-400")
            }
          >
            {live ? "LIVE" : fmt(duration)}
          </span>
        </div>

        <div className="mt-3 flex justify-center gap-2">
          <button disabled={!ready} onClick={() => player?.prev()} className={btn}>
            ⏮
          </button>
          <button onClick={toggle} className={`${btn} min-w-12 text-base`}>
            {playing ? "⏸" : "▶"}
          </button>
          <button disabled={!ready} onClick={() => player?.next()} className={btn}>
            ⏭
          </button>
          <button disabled={!ready} onClick={() => player?.stop()} className={btn}>
            ⏹
          </button>
          <button
            disabled={!ready}
            onClick={() => player?.setShuffle(!status.shuffle)}
            className={`${btn} ${status.shuffle ? btnActive : ""}`}
          >
            🔀
          </button>
          <button
            disabled={!ready}
            onClick={() => player?.setRepeat(nextRepeat(status.repeat))}
            title={`Repeat: ${status.repeat}`}
            className={`${btn} ${status.repeat !== RepeatMode.Off ? btnActive : ""}`}
          >
            🔁
          </button>
        </div>

        <div className="mt-4 flex items-center gap-3">
          <span
            className={
              "rounded-full border px-2.5 py-0.5 font-mono text-xs uppercase tracking-wide " +
              (playing
                ? "border-emerald-500 text-emerald-400"
                : "border-zinc-700 text-zinc-400")
            }
          >
            {status.state}
          </span>
          <span className="font-mono text-xs text-zinc-500">
            {status.queue_len
              ? `${status.index + 1} / ${status.queue_len}`
              : "queue empty"}
          </span>
        </div>

        <div className="mt-4 flex items-center gap-3">
          <label className="text-sm text-zinc-400">Volume</label>
          <input
            type="range"
            min={0}
            max={100}
            value={volume}
            onChange={(e) => onVolume(Number(e.target.value))}
            className="flex-1 accent-indigo-500"
          />
          <span className="w-12 text-right font-mono text-xs">{volume}%</span>
        </div>
      </section>

      <section className="rounded-2xl border border-zinc-800 bg-zinc-900 p-5 shadow-xl shadow-black/30">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-base font-semibold">Equalizer</h2>
          <label className="flex items-center gap-2 text-sm text-zinc-400">
            <input
              type="checkbox"
              checked={eqEnabled}
              onChange={(e) => onEqEnabled(e.target.checked)}
              className="accent-indigo-500"
            />
            enabled
          </label>
        </div>
        <div className="flex justify-between gap-1">
          {CUTOFFS.map((hz, i) => (
            <div key={hz} className="flex flex-1 flex-col items-center gap-2">
              <span className="font-mono text-xs text-zinc-300">
                {eqGains[i]}
              </span>
              <input
                type="range"
                min={-24}
                max={24}
                step={1}
                value={eqGains[i]}
                onChange={(e) => onEqBand(i, Number(e.target.value))}
                className="eq-slider accent-indigo-500"
              />
              <span className="font-mono text-[0.6rem] text-zinc-500">
                {hz >= 1000 ? `${hz / 1000}k` : hz}
              </span>
            </div>
          ))}
        </div>
      </section>

      <section className="mb-4 rounded-2xl border border-zinc-800 bg-zinc-900 p-5 shadow-xl shadow-black/30">
        <h2 className="mb-4 text-base font-semibold">DSP</h2>
        <DspPanel apply={(fn) => ensureReady().then(fn)} />
      </section>

      <footer className="mt-6 text-center text-xs text-zinc-600">
        First click boots the audio engine. No special server headers needed.
      </footer>
    </div>
  );
}
