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

/** When a track change crossfades (Rockbox's `crossfade` setting).
 *  Setters also accept the raw int (0–5, in this order). */
export enum CrossfadeMode {
  Off = "off",
  AutoSkip = "auto-skip",
  ManualSkip = "manual-skip",
  Shuffle = "shuffle",
  ShuffleOrManualSkip = "shuffle-or-manual",
  Always = "always",
}

/** How the outgoing track behaves during the crossfade overlap
 *  (`crossfade_fade_out_mixmode`). */
export enum CrossfadeMixMode {
  /** Both tracks fade — outgoing ramps to silence as incoming ramps up. */
  Crossfade = "crossfade",
  /** Outgoing stays at full volume; the incoming track is mixed on top. */
  Mix = "mix",
}

/** Crossfade options (seconds; Rockbox ranges — delays 0–7 s, durations 0–15 s). */
export interface CrossfadeOptions {
  fadeOutDelay?: number;
  fadeOutDuration?: number;
  fadeInDelay?: number;
  fadeInDuration?: number;
  mixMode?: CrossfadeMixMode | number;
}

/** Rockbox playlist insertion modes (apps/playlist.c). Setters also accept
 *  the raw int (0–7, in this order). */
export enum InsertMode {
  Prepend = "prepend",
  /** After the previous Insert batch (Rockbox's chained "Insert"). */
  Insert = "insert",
  /** Directly after the current track. */
  PlayNext = "insert-next",
  /** Append to the end of the queue. */
  PlayLast = "insert-last",
  /** A random slot after the current track. */
  InsertShuffled = "insert-shuffled",
  /** Append the batch in random order. */
  InsertLastShuffled = "insert-last-shuffled",
  /** Replace the whole queue. */
  Replace = "replace",
  /** Explicit position (pass `index`). */
  AtIndex = "index",
}

/** One `.m3u` / `.m3u8` entry. */
export interface M3uEntry {
  url: string;
  title: string | null;
  durationMs: number | null;
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
  /** Insert URL(s) with a Rockbox insertion mode; `index` only applies to
   *  InsertMode.AtIndex. */
  insert(urls: string | string[], mode?: InsertMode | number, index?: number): void;
  /** Remove the queue entry at `index` (0-based). Removing the current track
   *  hard-cuts to the one that slides into its place. */
  removeAt(index: number): void;
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
  /** Rockbox crossfade (the pcmbuf algorithm). See CrossfadeMode / CrossfadeOptions. */
  setCrossfade(mode: CrossfadeMode | number, opts?: CrossfadeOptions): void;
  setChannelMode(mode: ChannelMode | number): void;
  setStereoWidth(percent: number): void;
  setCompressor(threshold: number, makeup: number, ratio: number, knee: number, release: number, attack: number): void;
  setReplaygain(mode: ReplayGainMode | number, noclip: boolean, preampDb: number): void;

  /** The 10 default EQ band centre frequencies (Hz). */
  static readonly EQ_BAND_CUTOFFS: number[];

  // M3U / M3U8 playlists
  /** Whether `url` looks like an .m3u / .m3u8 playlist. */
  static isM3uUrl(url: string): boolean;
  /** Parse playlist text; relative paths resolve against `baseUrl`. */
  static parseM3u(text: string, baseUrl?: string): M3uEntry[];
  /** Serialize entries (strings or M3uEntry-likes) to `.m3u8` text. */
  static serializeM3u(
    entries: (string | { url: string; title?: string | null; durationMs?: number | null })[],
  ): string;
  /** Replace the queue with a playlist's entries; returns the URLs. */
  loadM3u(text: string, opts?: { autoplay?: boolean; baseUrl?: string }): string[];
  /** Add a playlist's entries to the queue with any InsertMode; returns the URLs. */
  enqueueM3u(text: string, opts?: { baseUrl?: string; mode?: InsertMode | number }): string[];
  /** Fetch an .m3u/.m3u8 URL and replace the queue with it. */
  loadM3uUrl(url: string, autoplay?: boolean): Promise<string[]>;
  /** Fetch an .m3u/.m3u8 URL and add it to the queue. */
  enqueueM3uUrl(url: string, mode?: InsertMode | number): Promise<string[]>;
  /** The current queue as `.m3u8` text. */
  exportM3u(): string;

  // Events
  on<K extends keyof RockboxEventMap>(event: K, cb: (data: RockboxEventMap[K]) => void): this;
  off<K extends keyof RockboxEventMap>(event: K, cb: (data: RockboxEventMap[K]) => void): this;

  // Persisted settings (localStorage)
  getSettings(): Record<string, unknown>;
}

export default RockboxPlayer;
