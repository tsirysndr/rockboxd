// Runtime-agnostic high-level API, written once against the Raw backend.

import type { Raw } from "./ffi.ts";
import { CrossfadeMode, MixMode, ReplayGainMode } from "./enums.ts";
import type { Metadata, PlayerConfig, PlayerStatus } from "./types.ts";

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
  }

  return { abiVersion, metadata, Dsp, Player };
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
