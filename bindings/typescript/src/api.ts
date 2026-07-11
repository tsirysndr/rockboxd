// Runtime-agnostic high-level API, written once against the Raw backend.

import type { Raw } from "./ffi.ts";
import { CrossfadeMode, InsertPosition, MixMode, ReplayGainMode } from "./enums.ts";
import type {
  M3uEntry,
  Metadata,
  PlayerConfig,
  PlayerStatus,
  ResumeState,
} from "./types.ts";

/** Map the snake_case resume JSON to the camelCase ResumeState shape. */
function toResumeState(json: string): ResumeState {
  const r = JSON.parse(json) as {
    tracks: string[];
    index: number;
    elapsed_ms: number;
  };
  return { tracks: r.tracks, index: r.index, elapsedMs: r.elapsed_ms };
}

/** Map the snake_case m3u-entry JSON array to the camelCase M3uEntry shape. */
function toM3uEntries(json: string): M3uEntry[] {
  const arr = JSON.parse(json) as {
    path: string;
    duration_ms: number | null;
    title: string | null;
  }[];
  return arr.map((e) => ({
    path: e.path,
    durationMs: e.duration_ms,
    title: e.title,
  }));
}

const opt = (v: number | null | undefined): number =>
  v === null || v === undefined ? Number.NaN : v;

export function makeApi(raw: Raw) {
  const s = raw.sym;

  const abiVersion = (): number => Number(s.rb_ffi_abi_version());

  const metadata = {
    /** Parse the metadata of the audio file at `path`. */
    read(path: string): Metadata {
      const json = raw.takeString(s.rb_meta_read_json(raw.cstr(path)));
      if (json === null) throw new Error(`could not read metadata from ${path}`);
      return JSON.parse(json) as Metadata;
    },
    /** Guess the codec label from a filename's extension (null if unknown). */
    probe(filename: string): string | null {
      return raw.takeString(s.rb_meta_probe(raw.cstr(filename)));
    },
  };

  /** Interleaved-S16LE-stereo DSP instance. Single instance per process. */
  class Dsp {
    #h: unknown;
    constructor(sampleRate: number) {
      this.#h = s.rb_dsp_new(sampleRate);
      if (raw.isNull(this.#h)) throw new Error("rb_dsp_new returned NULL");
    }
    close(): void {
      if (!raw.isNull(this.#h)) {
        s.rb_dsp_free(this.#h);
        this.#h = null;
      }
    }
    [Symbol.dispose](): void {
      this.close();
    }

    setInputFrequency(hz: number): void {
      s.rb_dsp_set_input_frequency(this.#h, hz);
    }
    flush(): void {
      s.rb_dsp_flush(this.#h);
    }
    eqEnable(enable: boolean): void {
      s.rb_dsp_eq_enable(this.#h, enable);
    }
    setEqBand(band: number, cutoffHz: number, q: number, gainDb: number): void {
      s.rb_dsp_set_eq_band(this.#h, band, cutoffHz, q, gainDb);
    }
    setEqPrecut(db: number): void {
      s.rb_dsp_set_eq_precut(this.#h, db);
    }
    setTone(bassDb: number, trebleDb: number): void {
      s.rb_dsp_set_tone(this.#h, bassDb, trebleDb);
    }
    setToneCutoffs(bassHz: number, trebleHz: number): void {
      s.rb_dsp_set_tone_cutoffs(this.#h, bassHz, trebleHz);
    }
    setSurround(delayMs: number, balance: number, fx1: number, fx2: number): void {
      s.rb_dsp_set_surround(this.#h, delayMs, balance, fx1, fx2);
    }
    setChannelConfig(mode: number): void {
      s.rb_dsp_set_channel_config(this.#h, mode);
    }
    setStereoWidth(percent: number): void {
      s.rb_dsp_set_stereo_width(this.#h, percent);
    }
    setCompressor(
      threshold: number, makeupGain: number, ratio: number, knee: number,
      releaseTime: number, attackTime: number,
    ): void {
      s.rb_dsp_set_compressor(this.#h, threshold, makeupGain, ratio, knee, releaseTime, attackTime);
    }
    /** mode: DspReplayGainMode (TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3). */
    setReplaygain(mode: number, noclip: boolean, preampDb: number): void {
      s.rb_dsp_set_replaygain(this.#h, mode, noclip, preampDb);
    }
    /** Per-track gains in plain dB, peaks as linear amplitude. undefined => absent. */
    setReplaygainGains(
      trackGainDb?: number | null, albumGainDb?: number | null,
      trackPeak?: number | null, albumPeak?: number | null,
    ): void {
      s.rb_dsp_set_replaygain_gains(
        this.#h, opt(trackGainDb), opt(albumGainDb), opt(trackPeak), opt(albumPeak),
      );
    }
    /** Native Q7.24 factors (the `raw_*` metadata fields); use BigInt. */
    setReplaygainGainsRaw(
      trackGain: bigint, albumGain: bigint, trackPeak: bigint, albumPeak: bigint,
    ): void {
      s.rb_dsp_set_replaygain_gains_raw(this.#h, trackGain, albumGain, trackPeak, albumPeak);
    }
    /** Run interleaved stereo S16 samples through the pipeline. */
    process(samples: Int16Array): Int16Array {
      if (samples.length % 2 !== 0) {
        throw new Error("input must be interleaved stereo (even length)");
      }
      const out = raw.sizeOut();
      const ptr = s.rb_dsp_process(this.#h, raw.i16in(samples), samples.length, out.arg);
      const n = out.value();
      if (raw.isNull(ptr) || n === 0) return new Int16Array(0);
      return raw.takeI16(ptr, n);
    }
  }

  /** Queue-based player. Owns a live output device + engine thread. */
  class Player {
    #h: unknown;
    constructor(config?: PlayerConfig) {
      if (config === undefined) {
        this.#h = s.rb_player_new();
      } else if (
        config.resumeFile !== undefined ||
        config.resumeSaveIntervalMs !== undefined
      ) {
        this.#h = s.rb_player_new_with_config_ex(
          config.sampleRate ?? 0,
          config.bufferSeconds ?? 4.0,
          config.volume ?? 1.0,
          config.replaygainMode ?? ReplayGainMode.OFF,
          config.replaygainPreampDb ?? 0.0,
          config.replaygainPreventClipping ?? true,
          config.crossfadeMode ?? CrossfadeMode.OFF,
          config.fadeOutDelayMs ?? 0,
          config.fadeOutDurationMs ?? 2000,
          config.fadeInDelayMs ?? 0,
          config.fadeInDurationMs ?? 2000,
          config.mixMode ?? MixMode.CROSSFADE,
          raw.cstr(config.resumeFile ?? ""),
          config.resumeSaveIntervalMs ?? 0,
        );
      } else {
        this.#h = s.rb_player_new_with_config(
          config.sampleRate ?? 0,
          config.bufferSeconds ?? 4.0,
          config.volume ?? 1.0,
          config.replaygainMode ?? ReplayGainMode.OFF,
          config.replaygainPreampDb ?? 0.0,
          config.replaygainPreventClipping ?? true,
          config.crossfadeMode ?? CrossfadeMode.OFF,
          config.fadeOutDelayMs ?? 0,
          config.fadeOutDurationMs ?? 2000,
          config.fadeInDelayMs ?? 0,
          config.fadeInDurationMs ?? 2000,
          config.mixMode ?? MixMode.CROSSFADE,
        );
      }
      if (raw.isNull(this.#h)) {
        throw new Error("failed to create Player (no output device?)");
      }
    }
    close(): void {
      if (!raw.isNull(this.#h)) {
        s.rb_player_free(this.#h);
        this.#h = null;
      }
    }
    [Symbol.dispose](): void {
      this.close();
    }

    setQueue(paths: string[]): void {
      s.rb_player_set_queue_json(this.#h, raw.cstr(JSON.stringify(paths)));
    }
    enqueue(path: string): void {
      s.rb_player_enqueue(this.#h, raw.cstr(path));
    }
    /** Insert paths/URLs at `position` (InsertPosition); `index` used for INDEX. */
    insert(paths: string[], position: number = InsertPosition.INSERT_LAST, index = 0): void {
      s.rb_player_insert_json(this.#h, raw.cstr(JSON.stringify(paths)), position, index);
    }
    /** The current queue as an array of paths/URLs. */
    queue(): string[] {
      const json = raw.takeString(s.rb_player_queue_json(this.#h));
      if (json === null) throw new Error("rb_player_queue_json returned NULL");
      return JSON.parse(json) as string[];
    }
    play(): void {
      s.rb_player_play(this.#h);
    }
    pause(): void {
      s.rb_player_pause(this.#h);
    }
    toggle(): void {
      s.rb_player_toggle(this.#h);
    }
    stop(): void {
      s.rb_player_stop(this.#h);
    }
    next(): void {
      s.rb_player_next(this.#h);
    }
    previous(): void {
      s.rb_player_previous(this.#h);
    }
    skipTo(index: number): void {
      s.rb_player_skip_to(this.#h, index);
    }
    seekMs(ms: number | bigint): void {
      s.rb_player_seek_ms(this.#h, BigInt(ms));
    }
    setVolume(vol: number): void {
      s.rb_player_set_volume(this.#h, vol);
    }
    volume(): number {
      return Number(s.rb_player_volume(this.#h));
    }
    sampleRate(): number {
      return Number(s.rb_player_sample_rate(this.#h));
    }
    setCrossfade(
      mode: number, foDelayMs = 0, foDurMs = 2000, fiDelayMs = 0, fiDurMs = 2000,
      mixMode = MixMode.CROSSFADE,
    ): void {
      s.rb_player_set_crossfade(this.#h, mode, foDelayMs, foDurMs, fiDelayMs, fiDurMs, mixMode);
    }
    /** mode: ReplayGainMode (OFF=0, TRACK=1, ALBUM=2). */
    setReplaygain(mode: number, preampDb: number, preventClipping: boolean): void {
      s.rb_player_set_replaygain(this.#h, mode, preampDb, preventClipping);
    }
    status(): PlayerStatus {
      const json = raw.takeString(s.rb_player_status_json(this.#h));
      if (json === null) throw new Error("rb_player_status_json returned NULL");
      return JSON.parse(json) as PlayerStatus;
    }
    /** Restore the persisted queue + position (does NOT auto-play); null if none. */
    resume(): ResumeState | null {
      const json = raw.takeString(s.rb_player_resume(this.#h));
      return json === null ? null : toResumeState(json);
    }
    /** Persist the current queue + exact position to the configured resume file. */
    saveResume(): void {
      s.rb_player_save_resume(this.#h);
    }
    /** Delete the persisted resume state. */
    clearResume(): void {
      s.rb_player_clear_resume(this.#h);
    }
    /** Import a playlist file into the queue at `position`; null on error. */
    importM3u(path: string, position: number = InsertPosition.INSERT_LAST, index = 0): string[] | null {
      const json = raw.takeString(
        s.rb_player_import_m3u(this.#h, raw.cstr(path), position, index),
      );
      return json === null ? null : (JSON.parse(json) as string[]);
    }
    /** Replace the queue with a playlist file; returns loaded paths (null on error). */
    loadM3u(path: string): string[] | null {
      const json = raw.takeString(s.rb_player_load_m3u(this.#h, raw.cstr(path)));
      return json === null ? null : (JSON.parse(json) as string[]);
    }
    /** Export the current queue to an .m3u8 (atomic); true on success. */
    exportM3u(path: string): boolean {
      return Number(s.rb_player_export_m3u(this.#h, raw.cstr(path))) === 0;
    }
  }

  /** Peek at a resume file without a player; null if absent/invalid. */
  const loadResume = (path: string): ResumeState | null => {
    const json = raw.takeString(s.rb_load_resume_json(raw.cstr(path)));
    return json === null ? null : toResumeState(json);
  };

  /** Parse a playlist file into its entries; null on error. */
  const m3uRead = (path: string): M3uEntry[] | null => {
    const json = raw.takeString(s.rb_m3u_read_json(raw.cstr(path)));
    return json === null ? null : toM3uEntries(json);
  };

  /** Write an array of paths as an .m3u8; true on success. */
  const m3uWrite = (path: string, paths: string[]): boolean =>
    Number(s.rb_m3u_write_json(raw.cstr(path), raw.cstr(JSON.stringify(paths)))) === 0;

  /** Whether a string looks like an http(s):// URL. */
  const isUrl = (str: string): boolean => Boolean(s.rb_is_url(raw.cstr(str)));

  return { abiVersion, metadata, Dsp, Player, loadResume, m3uRead, m3uWrite, isUrl };
}

/** Generate `seconds` of a sine as interleaved stereo Int16. */
export function sineStereo(
  freqHz: number, seconds: number, rate: number, amplitude = 16000,
): Int16Array {
  const n = Math.floor(seconds * rate);
  const buf = new Int16Array(n * 2);
  for (let i = 0; i < n; i++) {
    let v = Math.round(Math.sin((i * 2 * Math.PI * freqHz) / rate) * amplitude);
    v = Math.max(-32768, Math.min(32767, v));
    buf[i * 2] = v;
    buf[i * 2 + 1] = v;
  }
  return buf;
}
