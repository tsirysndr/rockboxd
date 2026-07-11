// Types mirroring the JSON shapes emitted by crates/rockbox-ffi (snake_case).

export interface ReplayGain {
  track_gain_db: number | null;
  album_gain_db: number | null;
  track_peak: number | null;
  album_peak: number | null;
  raw_track_gain: number;
  raw_album_gain: number;
  raw_track_peak: number;
  raw_album_peak: number;
}

export interface AlbumArt {
  kind: "bmp" | "png" | "jpeg" | "unknown";
  offset: number;
  size: number;
  id3_unsync: boolean;
  vorbis_base64: boolean;
}

export interface Cuesheet {
  offset: number;
  size: number;
  encoding: "latin1" | "utf8" | "utf16le" | "utf16be" | "unknown";
}

export interface Metadata {
  codec: string;
  codec_id: number;
  title: string;
  artist: string;
  album: string;
  albumartist: string;
  composer: string;
  grouping: string;
  comment: string;
  genre: string;
  year_string: string;
  track_string: string;
  disc_string: string;
  mb_track_id: string;
  track_number: number | null;
  disc_number: number | null;
  year: number | null;
  duration_ms: number;
  bitrate: number;
  sample_rate: number;
  filesize: number;
  samples: number;
  vbr: boolean;
  first_frame_offset: number;
  replaygain: ReplayGain;
  album_art: AlbumArt | null;
  cuesheet: Cuesheet | null;
}

export interface PlayerStatus {
  state: "stopped" | "playing" | "paused";
  index: number | null;
  position_ms: number;
  duration_ms: number;
  queue_len: number;
  metadata: Metadata | null;
}

export interface PlayerConfig {
  sampleRate?: number; // 0 / undefined => device default
  bufferSeconds?: number;
  volume?: number;
  replaygainMode?: number; // ReplayGainMode
  replaygainPreampDb?: number;
  replaygainPreventClipping?: boolean;
  crossfadeMode?: number; // CrossfadeMode
  fadeOutDelayMs?: number;
  fadeOutDurationMs?: number;
  fadeInDelayMs?: number;
  fadeInDurationMs?: number;
  mixMode?: number; // MixMode
  resumeFile?: string; // .m3u8 path to auto-persist queue+position; unset disables
  resumeSaveIntervalMs?: number; // 0 => 5 s default
}

/** Restored playback session (from `Player.resume` / `loadResume`). */
export interface ResumeState {
  tracks: string[];
  index: number;
  elapsedMs: number;
}

/** One entry parsed from an .m3u/.m3u8 playlist (from `m3uRead`). */
export interface M3uEntry {
  path: string;
  durationMs: number | null;
  title: string | null;
}
