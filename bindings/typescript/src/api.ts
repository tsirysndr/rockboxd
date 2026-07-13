// Runtime-agnostic high-level API, written once against the Raw backend.

import type { Raw } from "./ffi.ts";
import { CrossfadeMode, InsertPosition, MixMode, RepeatMode, ReplayGainMode } from "./enums.ts";
import type { RepeatMode as RepeatModeValue } from "./enums.ts";
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

  /**
   * Streaming decoder for one audio file. Decodes to interleaved-stereo S16
   * PCM, one chunk at a time.
   *
   * The codec state is process-wide, so only one `Decoder` may decode at a
   * time — constructing a second one blocks until the first is closed. Call
   * `close()` (or use `using`) when done.
   */
  class Decoder {
    #h: unknown;
    constructor(path: string) {
      this.#h = s.rb_decoder_open(raw.cstr(path));
      if (raw.isNull(this.#h)) throw new Error(`could not open decoder for ${path}`);
    }
    close(): void {
      if (!raw.isNull(this.#h)) {
        s.rb_decoder_free(this.#h);
        this.#h = null;
      }
    }
    [Symbol.dispose](): void {
      this.close();
    }

    /** Tags and stream properties (same shape as `metadata.read`). */
    metadata(): Metadata {
      const json = raw.takeString(s.rb_decoder_metadata_json(this.#h));
      if (json === null) throw new Error("rb_decoder_metadata_json returned NULL");
      return JSON.parse(json) as Metadata;
    }
    /**
     * Next buffer of decoded PCM as `{ samples, sampleRate }`, where `samples`
     * is interleaved stereo Int16. Returns `null` at end of track (or after a
     * codec error — see `finished`).
     */
    nextChunk(): { samples: Int16Array; sampleRate: number } | null {
      const outLen = raw.sizeOut();
      const outRate = raw.u32Out();
      const ptr = s.rb_decoder_next_chunk(this.#h, outLen.arg, outRate.arg);
      const n = outLen.value();
      if (raw.isNull(ptr) || n === 0) return null;
      return { samples: raw.takeI16(ptr, n), sampleRate: outRate.value() };
    }
    /** Request a seek to `ms` milliseconds. */
    seekMs(ms: number | bigint): void {
      s.rb_decoder_seek_ms(this.#h, BigInt(ms));
    }
    /** Playback position last reported by the codec, in milliseconds. */
    elapsedMs(): number {
      return Number(s.rb_decoder_elapsed_ms(this.#h));
    }
    /**
     * `{ done, code }`: `done` is true once the track has ended; `code` is the
     * exit status (0 = clean end, negative = codec error), valid only when
     * `done`.
     */
    finished(): { done: boolean; code: number } {
      const outCode = raw.i32Out();
      const done = Boolean(s.rb_decoder_finished(this.#h, outCode.arg));
      return { done, code: outCode.value() };
    }
  }

  /**
   * Queue-based player. Owns a live output device + engine thread.
   *
   * All mutating methods (setters, transport, DSP) return `this`, so calls
   * can be fluently chained, e.g.:
   * ```ts
   * player.setQueue([file]).setShuffle(true).setRepeat(RepeatMode.All).play();
   * ```
   * Getters (`status`, `volume`, `queue`, …) and lifecycle (`close`) return
   * their own values as usual.
   */
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

    /**
     * Replace the queue. Each entry may be a local file path, an
     * `http(s)://` URL to a finite remote file, or a live-radio /
     * streaming URL.
     */
    setQueue(paths: string[]): this {
      s.rb_player_set_queue_json(this.#h, raw.cstr(JSON.stringify(paths)));
      return this;
    }
    /**
     * Append one track to the queue. `path` may be a local file path, an
     * `http(s)://` URL to a finite remote file, or a live-radio / streaming
     * URL.
     */
    enqueue(path: string): this {
      s.rb_player_enqueue(this.#h, raw.cstr(path));
      return this;
    }
    /** Insert paths/URLs at `position` (InsertPosition); `index` used for INDEX. */
    insert(paths: string[], position: number = InsertPosition.INSERT_LAST, index = 0): this {
      s.rb_player_insert_json(this.#h, raw.cstr(JSON.stringify(paths)), position, index);
      return this;
    }
    /** The current queue as an array of paths/URLs. */
    queue(): string[] {
      const json = raw.takeString(s.rb_player_queue_json(this.#h));
      if (json === null) throw new Error("rb_player_queue_json returned NULL");
      return JSON.parse(json) as string[];
    }
    play(): this {
      s.rb_player_play(this.#h);
      return this;
    }
    pause(): this {
      s.rb_player_pause(this.#h);
      return this;
    }
    toggle(): this {
      s.rb_player_toggle(this.#h);
      return this;
    }
    stop(): this {
      s.rb_player_stop(this.#h);
      return this;
    }
    next(): this {
      s.rb_player_next(this.#h);
      return this;
    }
    previous(): this {
      s.rb_player_previous(this.#h);
      return this;
    }
    skipTo(index: number): this {
      s.rb_player_skip_to(this.#h, index);
      return this;
    }
    seekMs(ms: number | bigint): this {
      s.rb_player_seek_ms(this.#h, BigInt(ms));
      return this;
    }
    setVolume(vol: number): this {
      s.rb_player_set_volume(this.#h, vol);
      return this;
    }
    /** Stereo balance, -100 (full left) to +100 (full right); 0 = centre. */
    setBalance(balance: number): this {
      s.rb_player_set_balance(this.#h, balance);
      return this;
    }
    volume(): number {
      return Number(s.rb_player_volume(this.#h));
    }
    /** Current stereo balance, -100 (full left) to +100 (full right). */
    balance(): number {
      return Number(s.rb_player_balance(this.#h));
    }
    sampleRate(): number {
      return Number(s.rb_player_sample_rate(this.#h));
    }
    setCrossfade(
      mode: number, foDelayMs = 0, foDurMs = 2000, fiDelayMs = 0, fiDurMs = 2000,
      mixMode = MixMode.CROSSFADE,
    ): this {
      s.rb_player_set_crossfade(this.#h, mode, foDelayMs, foDurMs, fiDelayMs, fiDurMs, mixMode);
      return this;
    }
    /** mode: ReplayGainMode (OFF=0, TRACK=1, ALBUM=2). */
    setReplaygain(mode: number, preampDb: number, preventClipping: boolean): this {
      s.rb_player_set_replaygain(this.#h, mode, preampDb, preventClipping);
      return this;
    }
    status(): PlayerStatus {
      const json = raw.takeString(s.rb_player_status_json(this.#h));
      if (json === null) throw new Error("rb_player_status_json returned NULL");
      return JSON.parse(json) as PlayerStatus;
    }
    /** Enable or disable the graphic EQ. */
    setEqEnabled(enabled: boolean): this {
      s.rb_player_set_eq_enabled(this.#h, enabled);
      return this;
    }
    /** Whether the graphic EQ is currently enabled. */
    isEqEnabled(): boolean {
      return Boolean(s.rb_player_is_eq_enabled(this.#h));
    }
    /** Configure one EQ band; q and gainDb are plain (not tenths) units. */
    setEqBand(band: number, cutoffHz: number, q: number, gainDb: number): this {
      s.rb_player_set_eq_band(this.#h, band, cutoffHz, q, gainDb);
      return this;
    }
    setEqPrecut(db: number): this {
      s.rb_player_set_eq_precut(this.#h, db);
      return this;
    }
    /** preset: EqPreset. */
    setEqPreset(preset: number): this {
      s.rb_player_set_eq_preset(this.#h, preset);
      return this;
    }
    setTone(
      bassDb: number, trebleDb: number, bassCutoffHz: number, trebleCutoffHz: number,
    ): this {
      s.rb_player_set_tone(this.#h, bassDb, trebleDb, bassCutoffHz, trebleCutoffHz);
      return this;
    }
    setBass(bassDb: number): this {
      s.rb_player_set_bass(this.#h, bassDb);
      return this;
    }
    setTreble(trebleDb: number): this {
      s.rb_player_set_treble(this.#h, trebleDb);
      return this;
    }
    setBassCutoff(hz: number): this {
      s.rb_player_set_bass_cutoff(this.#h, hz);
      return this;
    }
    setTrebleCutoff(hz: number): this {
      s.rb_player_set_treble_cutoff(this.#h, hz);
      return this;
    }
    /** mode: CrossfeedMode (Off=0, Meier=1, Custom=2). */
    setCrossfeed(
      mode: number, directGain: number, crossGain: number, hfGain: number,
      hfCutoff: number,
    ): this {
      s.rb_player_set_crossfeed(this.#h, mode, directGain, crossGain, hfGain, hfCutoff);
      return this;
    }
    setBassEnhancement(strength: number, precut: number): this {
      s.rb_player_set_bass_enhancement(this.#h, strength, precut);
      return this;
    }
    setFatigueReduction(strength: number): this {
      s.rb_player_set_fatigue_reduction(this.#h, strength);
      return this;
    }
    setSurround(
      delayMs: number, balance: number, cutoffLowHz: number, cutoffHighHz: number,
    ): this {
      s.rb_player_set_surround(this.#h, delayMs, balance, cutoffLowHz, cutoffHighHz);
      return this;
    }
    /** mode: ChannelMode. */
    setChannelMode(mode: number): this {
      s.rb_player_set_channel_mode(this.#h, mode);
      return this;
    }
    setStereoWidth(percent: number): this {
      s.rb_player_set_stereo_width(this.#h, percent);
      return this;
    }
    setCompressor(
      thresholdDb: number, makeupGain: number, ratio: number, knee: number,
      attackMs: number, releaseMs: number,
    ): this {
      s.rb_player_set_compressor(
        this.#h, thresholdDb, makeupGain, ratio, knee, attackMs, releaseMs,
      );
      return this;
    }
    setDither(enabled: boolean): this {
      s.rb_player_set_dither(this.#h, enabled);
      return this;
    }
    setPitch(ratio: number): this {
      s.rb_player_set_pitch(this.#h, ratio);
      return this;
    }
    /** The current DSP settings as a plain object (parsed from JSON). */
    dspSettings(): Record<string, unknown> {
      const json = raw.takeString(s.rb_player_dsp_settings_json(this.#h));
      if (json === null) throw new Error("rb_player_dsp_settings_json returned NULL");
      return JSON.parse(json) as Record<string, unknown>;
    }
    /** Enable or disable queue shuffle. */
    setShuffle(enabled: boolean): this {
      s.rb_player_set_shuffle(this.#h, enabled);
      return this;
    }
    /** Whether queue shuffle is currently enabled. */
    isShuffleEnabled(): boolean {
      return Boolean(s.rb_player_is_shuffle_enabled(this.#h));
    }
    /** Set the repeat mode (RepeatMode: Off=0, One=1, All=2). */
    setRepeat(mode: RepeatModeValue | number): this {
      s.rb_player_set_repeat(this.#h, mode);
      return this;
    }
    /** The current repeat mode (RepeatMode: Off=0, One=1, All=2). */
    repeat(): RepeatModeValue {
      return Number(s.rb_player_repeat(this.#h)) as RepeatModeValue;
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

  return { abiVersion, metadata, Dsp, Decoder, Player, loadResume, m3uRead, m3uWrite, isUrl };
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
