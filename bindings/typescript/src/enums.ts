// Enum constants for the Rockbox FFI.
//
// NOTE: the DSP and player use *different* ReplayGain mode integers (a quirk
// of the underlying C ABI). Use DspReplayGainMode with `Dsp.setReplaygain`
// and ReplayGainMode with `Player.setReplaygain`.

/** For `Dsp.setReplaygain` (native Rockbox values). */
export const DspReplayGainMode = {
  TRACK: 0,
  ALBUM: 1,
  SHUFFLE: 2,
  OFF: 3,
} as const;

/** For `Player.setReplaygain`. */
export const ReplayGainMode = {
  OFF: 0,
  TRACK: 1,
  ALBUM: 2,
} as const;

export const CrossfadeMode = {
  OFF: 0,
  AUTO_SKIP: 1,
  MANUAL_SKIP: 2,
  SHUFFLE: 3,
  SHUFFLE_OR_MANUAL: 4,
  ALWAYS: 5,
} as const;

export const MixMode = {
  CROSSFADE: 0,
  MIX: 1,
} as const;

/** For `Player.setCrossfeed` (0 off, 1 Meier, 2 custom). */
export const CrossfeedMode = {
  Off: 0,
  Meier: 1,
  Custom: 2,
} as const;

/** For `Player.insert` / `Player.importM3u` (queue insert position). */
export const InsertPosition = {
  PREPEND: 0,
  INSERT: 1,
  INSERT_NEXT: 2,
  INSERT_LAST: 3,
  INSERT_SHUFFLED: 4,
  INSERT_LAST_SHUFFLED: 5,
  REPLACE: 6,
  INDEX: 7,
} as const;

export const ChannelConfig = {
  STEREO: 0,
  MONO: 1,
  CUSTOM: 2,
  MONO_LEFT: 3,
  MONO_RIGHT: 4,
  KARAOKE: 5,
  SWAP: 6,
} as const;

/** For `Player.setEqPreset`. */
export const EqPreset = {
  Flat: 0,
  Acoustic: 1,
  BassBoost: 2,
  BassReducer: 3,
  Classical: 4,
  Dance: 5,
  Deep: 6,
  Electronic: 7,
  HipHop: 8,
  Jazz: 9,
  Latin: 10,
  Loudness: 11,
  Lounge: 12,
  Piano: 13,
  Pop: 14,
  RnB: 15,
  Rock: 16,
  SmallSpeakers: 17,
  TrebleBoost: 18,
  TrebleReducer: 19,
  VocalBoost: 20,
} as const;

/** For `Player.setRepeat` / `Player.repeat` (0 off, 1 one, 2 all). */
export const RepeatMode = {
  Off: 0,
  One: 1,
  All: 2,
} as const;

/** The integer values of `RepeatMode` (0, 1 or 2). */
export type RepeatMode = (typeof RepeatMode)[keyof typeof RepeatMode];

/** For `Player.setChannelMode`. */
export const ChannelMode = {
  Stereo: 0,
  Mono: 1,
  Custom: 2,
  MonoLeft: 3,
  MonoRight: 4,
  Karaoke: 5,
  Swap: 6,
} as const;
