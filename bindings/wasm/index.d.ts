// Type declarations for rockbox-wasm (implementation: dist/rockbox.js).

export interface RockboxPlayerOptions {
  /**
   * Base URL the package's dist files are served from, e.g. `"/rockbox"` or
   * `"https://cdn.example.com/rockbox-wasm/dist"`. All three asset URLs derive
   * from it. If omitted they resolve as siblings of the loaded module.
   */
  baseUrl?: string;
  /** Explicit URL of `rockbox-core.js` (overrides `baseUrl`). */
  coreUrl?: string;
  /** Explicit URL of `rockbox-audio-worklet.js` (overrides `baseUrl`). */
  workletUrl?: string;
  /** Explicit URL of `rockbox-decoder-worker.js` (overrides `baseUrl`). */
  workerUrl?: string;
}

/** "stopped" | "playing" | "paused". */
export type PlaybackState = "stopped" | "playing" | "paused";

/** Repeat mode. Setters also accept the raw int (0 off, 1 one, 2 all). */
export enum RepeatMode {
  Off = "off",
  One = "one",
  All = "all",
}

/** ReplayGain mode. Setters also accept the raw int (0 track, 1 album, 2 shuffle, 3 off). */
export enum ReplayGainMode {
  Off = "off",
  Track = "track",
  Album = "album",
  Shuffle = "shuffle",
}

/** Channel mixing mode. Setters also accept the raw int (0–6). */
export enum ChannelMode {
  Stereo = "stereo",
  Mono = "mono",
  Custom = "custom",
  MonoLeft = "mono-left",
  MonoRight = "mono-right",
  Karaoke = "karaoke",
  Swap = "swap",
}

/** Headphone crossfeed mode. Setters also accept the raw int (0 off, 1 Meier, 2 custom). */
export enum CrossfeedMode {
  Off = "off",
  Meier = "meier",
  Custom = "custom",
}

export interface TrackMetadata {
  codec?: string;
  title?: string;
  artist?: string;
  album?: string;
  albumartist?: string;
  genre?: string;
  year?: number;
  duration_ms?: number;
  bitrate?: number;
  sample_rate?: number;
  /** Live-radio station name (from the ICY `icy-name` header), when available. */
  station?: string;
  [key: string]: unknown;
}

export interface StatusEvent {
  state: PlaybackState;
  index: number;
  queue_len: number;
  shuffle: boolean;
  repeat: RepeatMode;
}

export interface TrackEvent {
  index: number;
  url: string;
  /** True for an unbounded live stream (no duration, not seekable). */
  live: boolean;
  metadata: TrackMetadata | null;
}

export interface ProgressEvent {
  state: PlaybackState;
  index: number;
  live: boolean;
  elapsed_ms: number;
  /** 0 for live streams (unknown / infinite). */
  duration_ms: number;
  metadata: TrackMetadata | null;
}

export interface QueueEvent {
  urls: string[];
  index: number;
}

export interface ErrorEvent {
  message: string;
  index?: number;
}

export interface RockboxEventMap {
  status: StatusEvent;
  track: TrackEvent;
  progress: ProgressEvent;
  queue: QueueEvent;
  error: ErrorEvent;
}

/**
 * A browser music player backed by the Rockbox decode + DSP core (WebAssembly).
 *
 * ```ts
 * const player = new RockboxPlayer();
 * await player.init();                 // from a user gesture (click)
 * player.setQueue(["song.flac"], true);
 * ```
 */
export class RockboxPlayer {
  constructor(opts?: RockboxPlayerOptions);

  /** Boot the audio graph + decoder worker. Call from a user gesture. */
  init(): Promise<void>;

  readonly ready: boolean;
  readonly audioContext: AudioContext | null;
  readonly volume: number;

  /** Latest status snapshot (also delivered via the `status` event). */
  state: StatusEvent;
  progress: { elapsed_ms: number; duration_ms: number };
  metadata: TrackMetadata | null;
  queue: string[];

  // Transport
  setQueue(urls: string[], autoplay?: boolean): void;
  enqueue(url: string): void;
  clearQueue(): void;
  play(): void;
  pause(): void;
  toggle(): void;
  stop(): void;
  next(): void;
  prev(): void;
  skipTo(index: number): void;
  seek(ms: number): void;
  setShuffle(on: boolean): void;
  setRepeat(mode: RepeatMode | number): void;
  /** Output volume 0.0..=1.0 (a Web Audio GainNode; not a DSP stage). */
  setVolume(v: number): void;

  // DSP / equalizer (forwarded to rockbox-dsp)
  setEqEnabled(on: boolean): void;
  /** band 0..9, cutoff in Hz, Q factor, gain in dB. */
  setEqBand(band: number, cutoffHz: number, q: number, gainDb: number): void;
  setEqPrecut(db: number): void;
  setTone(bassDb: number, trebleDb: number): void;
  setToneCutoffs(bassHz: number, trebleHz: number): void;
  setSurround(delayMs: number, balance: number, fx1: number, fx2: number): void;
  /** Headphone crossfeed. `mode`: CrossfeedMode (or the raw int). Gains in tenths of dB (≤0). */
  setCrossfeed(
    mode: CrossfeedMode | number,
    directGain?: number,
    crossLfGain?: number,
    crossHfGain?: number,
    hfCutoff?: number,
  ): void;
  /** Perceptual Bass Enhancement: strength 0–100, precut in tenths of dB (≤0). */
  setPbe(strength: number, precut?: number): void;
  setChannelMode(mode: ChannelMode | number): void;
  setStereoWidth(percent: number): void;
  setCompressor(threshold: number, makeup: number, ratio: number, knee: number, release: number, attack: number): void;
  setReplaygain(mode: ReplayGainMode | number, noclip: boolean, preampDb: number): void;

  /** The 10 default EQ band centre frequencies (Hz). */
  static readonly EQ_BAND_CUTOFFS: number[];

  // Events
  on<K extends keyof RockboxEventMap>(event: K, cb: (data: RockboxEventMap[K]) => void): this;
  off<K extends keyof RockboxEventMap>(event: K, cb: (data: RockboxEventMap[K]) => void): this;

  // Persisted settings (localStorage)
  getSettings(): Record<string, unknown>;
}

export default RockboxPlayer;
