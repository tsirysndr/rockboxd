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

export const ChannelConfig = {
  STEREO: 0,
  MONO: 1,
  CUSTOM: 2,
  MONO_LEFT: 3,
  MONO_RIGHT: 4,
  KARAOKE: 5,
  SWAP: 6,
} as const;
