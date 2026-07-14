import { useEffect, useRef, useState } from "react";
import { useAtom } from "jotai";
import { RockboxPlayer, RepeatMode, InsertMode } from "rockbox-wasm";
import {
  IconArrowsShuffle,
  IconDownload,
  IconPlayerPauseFilled,
  IconPlayerPlayFilled,
  IconPlayerSkipBackFilled,
  IconPlayerSkipForwardFilled,
  IconPlayerStopFilled,
  IconPlaylistAdd,
  IconRepeat,
  IconRepeatOnce,
  IconTrash,
  IconVolume,
  IconX,
} from "@tabler/icons-react";
import { useRockbox } from "./useRockbox";
import { DspPanel } from "./DspPanel";
import {
  applySettings,
  eqEnabledAtom,
  eqGainsAtom,
  queueAtom,
  useSettingsSnapshot,
  volumeAtom,
} from "./settings";

const CUTOFFS = RockboxPlayer.EQ_BAND_CUTOFFS;
const REPEAT_CYCLE = [RepeatMode.Off, RepeatMode.One, RepeatMode.All];
const nextRepeat = (r: RepeatMode) =>
  REPEAT_CYCLE[(REPEAT_CYCLE.indexOf(r) + 1) % 3];

const INSERT_MODES: [InsertMode, string][] = [
  [InsertMode.PlayLast, "Play last"],
  [InsertMode.PlayNext, "Play next"],
  [InsertMode.Insert, "Insert"],
  [InsertMode.Prepend, "Prepend"],
  [InsertMode.InsertShuffled, "Insert shuffled"],
  [InsertMode.InsertLastShuffled, "Last shuffled"],
];

const fmt = (ms: number) => {
  const s = Math.max(0, Math.floor((ms || 0) / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** Display name for a queue URL (decoded basename). */
const trackName = (u: string) => {
  try {
    const seg = new URL(u, location.href).pathname.split("/").filter(Boolean).pop();
    return decodeURIComponent(seg || u);
  } catch {
    return u;
  }
};

const btn =
  "rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-2 text-sm font-medium " +
  "text-zinc-100 transition hover:enabled:border-indigo-500 hover:enabled:bg-zinc-700 " +
  "disabled:cursor-not-allowed disabled:opacity-40";
const btnActive = "border-transparent bg-indigo-500 text-white hover:enabled:bg-indigo-400";
const iconBtn = `${btn} flex items-center justify-center px-2.5`;

export function App() {
  const { ensureReady, getPlayer, ready, status, progress, track, queue, queueSeen, error } =
    useRockbox();

  const [url, setUrl] = useState("");
  const [insertMode, setInsertMode] = useState<InsertMode>(InsertMode.PlayLast);
  const [seeking, setSeeking] = useState<number | null>(null);
  // Persisted UI state (localStorage via jotai).
  const [volume, setVolume] = useAtom(volumeAtom);
  const [savedQueue, setSavedQueue] = useAtom(queueAtom);
  const [eqEnabled, setEqEnabled] = useAtom(eqEnabledAtom);
  const [eqGains, setEqGains] = useAtom(eqGainsAtom);
  const settings = useSettingsSnapshot();

  // When the engine boots, push every persisted setting so the audio matches
  // the restored UI, and restore the saved queue — unless the boot action
  // already set one (e.g. Play with a URL), which must win.
  useEffect(() => {
    if (!ready) return;
    const p = getPlayer();
    applySettings(p, settings);
    if (savedQueue.length > 0 && p.queue.length === 0) p.setQueue(savedQueue, false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready]);

  // Persist every queue change. `queueSeen` guards against overwriting the
  // saved queue with the initial empty state before the first worker event.
  useEffect(() => {
    if (queueSeen) setSavedQueue(queue.urls);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [queueSeen, queue.urls]);

  const md = track?.metadata ?? null;
  const live = progress?.live ?? track?.live ?? false;
  const playing = status.state === "playing";
  const duration = progress?.duration_ms ?? 0;
  const elapsed = progress?.elapsed_ms ?? 0;

  // ── actions ───────────────────────────────────────────────────────────────
  const playUrl = async () => {
    const u = url.trim();
    if (!u) return;
    const p = await ensureReady();
    // An .m3u/.m3u8 URL loads as a whole playlist.
    if (RockboxPlayer.isM3uUrl(u)) await p.loadM3uUrl(u, true);
    else p.setQueue([u], true);
  };
  const enqueue = async () => {
    const u = url.trim();
    if (!u) return;
    const p = await ensureReady();
    if (RockboxPlayer.isM3uUrl(u)) await p.enqueueM3uUrl(u, insertMode);
    else p.insert(u, insertMode);
  };
  const toggle = async () => (await ensureReady()).toggle();
  const fileRef = useRef<HTMLInputElement>(null);
  const importM3uFile = async (file: File | undefined) => {
    if (!file) return;
    const text = await file.text();
    const p = await ensureReady();
    // Empty queue: load + play. Otherwise add with the chosen insert mode.
    if (p.queue.length === 0) p.loadM3u(text, { autoplay: true });
    else p.enqueueM3u(text, { mode: insertMode });
  };
  const exportQueue = () => {
    if (!player || player.queue.length === 0) return;
    const blob = new Blob([player.exportM3u()], { type: "audio/x-mpegurl" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = "queue.m3u8";
    a.click();
    URL.revokeObjectURL(a.href);
  };

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
          <select
            value={insertMode}
            onChange={(e) => setInsertMode(e.target.value as InsertMode)}
            title="Where + Queue inserts (Rockbox insertion modes)"
            className="rounded-lg border border-zinc-700 bg-zinc-800 px-2 py-2 text-sm text-zinc-300"
          >
            {INSERT_MODES.map(([mode, label]) => (
              <option key={mode} value={mode}>{label}</option>
            ))}
          </select>
          <button
            onClick={() => fileRef.current?.click()}
            title="Import .m3u / .m3u8 playlist"
            className={iconBtn}
          >
            <IconPlaylistAdd size={18} />
          </button>
          <input
            ref={fileRef}
            type="file"
            accept=".m3u,.m3u8,audio/x-mpegurl"
            className="hidden"
            onChange={(e) => {
              importM3uFile(e.target.files?.[0]);
              e.target.value = ""; // allow re-importing the same file
            }}
          />
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
          <button disabled={!ready} onClick={() => player?.prev()} title="Previous"
            className={`${iconBtn}`}>
            <IconPlayerSkipBackFilled size={18} />
          </button>
          <button onClick={toggle} title={playing ? "Pause" : "Play"}
            className={`${iconBtn} min-w-12`}>
            {playing ? <IconPlayerPauseFilled size={18} /> : <IconPlayerPlayFilled size={18} />}
          </button>
          <button disabled={!ready} onClick={() => player?.next()} title="Next"
            className={`${iconBtn}`}>
            <IconPlayerSkipForwardFilled size={18} />
          </button>
          <button disabled={!ready} onClick={() => player?.stop()} title="Stop"
            className={`${iconBtn}`}>
            <IconPlayerStopFilled size={18} />
          </button>
          <button
            disabled={!ready}
            onClick={() => player?.setShuffle(!status.shuffle)}
            title={`Shuffle: ${status.shuffle ? "on" : "off"}`}
            className={`${iconBtn} ${status.shuffle ? btnActive : ""}`}
          >
            <IconArrowsShuffle size={18} />
          </button>
          <button
            disabled={!ready}
            onClick={() => player?.setRepeat(nextRepeat(status.repeat))}
            title={`Repeat: ${status.repeat}`}
            className={`${iconBtn} ${status.repeat !== RepeatMode.Off ? btnActive : ""}`}
          >
            {status.repeat === RepeatMode.One
              ? <IconRepeatOnce size={18} />
              : <IconRepeat size={18} />}
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
              ? `${status.index >= 0 ? status.index + 1 : "–"} / ${status.queue_len}`
              : "queue empty"}
          </span>
        </div>

        <div className="mt-4 flex items-center gap-3">
          <label className="flex items-center gap-1.5 text-sm text-zinc-400">
            <IconVolume size={16} /> Volume
          </label>
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

      {queue.urls.length > 0 && (
        <section className="mb-4 rounded-2xl border border-zinc-800 bg-zinc-900 p-5 shadow-xl shadow-black/30">
          <div className="mb-3 flex items-center justify-between">
            <h2 className="text-base font-semibold">
              Queue{" "}
              <span className="font-mono text-xs text-zinc-500">
                ({queue.urls.length})
              </span>
            </h2>
            <div className="flex gap-2">
              <button onClick={exportQueue} title="Export as .m3u8" className={iconBtn}>
                <IconDownload size={16} />
              </button>
              <button
                onClick={() => player?.clearQueue()}
                title="Clear queue"
                className={iconBtn}
              >
                <IconTrash size={16} />
              </button>
            </div>
          </div>
          <ol className="max-h-64 divide-y divide-zinc-800 overflow-y-auto">
            {queue.urls.map((u, i) => (
              <li
                key={`${i}-${u}`}
                className={
                  "group flex items-center gap-2 px-2 py-1.5 text-sm " +
                  (i === status.index
                    ? "rounded-md bg-indigo-500/15 text-indigo-300"
                    : "text-zinc-300")
                }
              >
                <span className="w-6 text-right font-mono text-xs text-zinc-500">
                  {i + 1}
                </span>
                <button
                  onClick={() => player?.skipTo(i)}
                  title={u}
                  className="min-w-0 flex-1 truncate text-left hover:text-indigo-300"
                >
                  {i === status.index && md?.title ? md.title : trackName(u)}
                </button>
                <button
                  onClick={() => player?.removeAt(i)}
                  title="Remove from queue"
                  className="rounded p-1 text-zinc-500 opacity-0 transition hover:bg-zinc-800 hover:text-red-400 group-hover:opacity-100"
                >
                  <IconX size={14} />
                </button>
              </li>
            ))}
          </ol>
        </section>
      )}

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
