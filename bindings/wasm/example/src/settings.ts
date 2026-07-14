// All player/DSP settings as jotai atoms, each persisted to localStorage
// (atomWithStorage). Components read/write them like useState; `applySettings`
// pushes the current values to the player (used once the engine boots so the
// audio matches the restored UI).
import { useAtomValue } from "jotai";
import { atomWithStorage } from "jotai/utils";
import {
  RockboxPlayer,
  ReplayGainMode,
  ChannelMode,
  CrossfeedMode,
} from "rockbox-wasm";

const CUTOFFS = RockboxPlayer.EQ_BAND_CUTOFFS;

export const volumeAtom = atomWithStorage("rb.volume", 100);

export const eqEnabledAtom = atomWithStorage("rb.eqEnabled", false);
export const eqGainsAtom = atomWithStorage<number[]>("rb.eqGains", CUTOFFS.map(() => 0));
export const eqPrecutAtom = atomWithStorage("rb.eqPrecut", 0);

export const bassAtom = atomWithStorage("rb.bass", 0);
export const trebleAtom = atomWithStorage("rb.treble", 0);

export const rgModeAtom = atomWithStorage<ReplayGainMode>("rb.rgMode", ReplayGainMode.Off);
export const rgPreampAtom = atomWithStorage("rb.rgPreamp", 0);
export const rgNoclipAtom = atomWithStorage("rb.rgNoclip", false);

export const cfModeAtom = atomWithStorage<CrossfeedMode>("rb.cfMode", CrossfeedMode.Off);
export const cfDirectAtom = atomWithStorage("rb.cfDirect", -1.5);

export const pbeAtom = atomWithStorage("rb.pbe", 0);
export const pbePrecutAtom = atomWithStorage("rb.pbePrecut", 0);

export const surDelayAtom = atomWithStorage("rb.surDelay", 0);
export const surBalanceAtom = atomWithStorage("rb.surBalance", 35);

export const compThreshAtom = atomWithStorage("rb.compThresh", 0);
export const compRatioAtom = atomWithStorage("rb.compRatio", 2);

export const channelAtom = atomWithStorage<ChannelMode>("rb.channel", ChannelMode.Stereo);
export const widthAtom = atomWithStorage("rb.width", 100);

export interface Settings {
  volume: number;
  eqEnabled: boolean;
  eqGains: number[];
  eqPrecut: number;
  bass: number;
  treble: number;
  rgMode: ReplayGainMode;
  rgPreamp: number;
  rgNoclip: boolean;
  cfMode: CrossfeedMode;
  cfDirect: number;
  pbe: number;
  pbePrecut: number;
  surDelay: number;
  surBalance: number;
  compThresh: number;
  compRatio: number;
  channel: ChannelMode;
  width: number;
}

/** Read the current value of every setting atom as a plain snapshot. */
export function useSettingsSnapshot(): Settings {
  return {
    volume: useAtomValue(volumeAtom),
    eqEnabled: useAtomValue(eqEnabledAtom),
    eqGains: useAtomValue(eqGainsAtom),
    eqPrecut: useAtomValue(eqPrecutAtom),
    bass: useAtomValue(bassAtom),
    treble: useAtomValue(trebleAtom),
    rgMode: useAtomValue(rgModeAtom),
    rgPreamp: useAtomValue(rgPreampAtom),
    rgNoclip: useAtomValue(rgNoclipAtom),
    cfMode: useAtomValue(cfModeAtom),
    cfDirect: useAtomValue(cfDirectAtom),
    pbe: useAtomValue(pbeAtom),
    pbePrecut: useAtomValue(pbePrecutAtom),
    surDelay: useAtomValue(surDelayAtom),
    surBalance: useAtomValue(surBalanceAtom),
    compThresh: useAtomValue(compThreshAtom),
    compRatio: useAtomValue(compRatioAtom),
    channel: useAtomValue(channelAtom),
    width: useAtomValue(widthAtom),
  };
}

/** Push every setting to the player (call once the engine is ready). */
export function applySettings(p: RockboxPlayer, s: Settings) {
  p.setVolume(s.volume / 100);
  p.setEqEnabled(s.eqEnabled);
  p.setEqPrecut(s.eqPrecut);
  s.eqGains.forEach((g, i) => p.setEqBand(i, CUTOFFS[i], 1.0, g));
  p.setTone(s.bass, s.treble);
  p.setReplaygain(s.rgMode, s.rgNoclip, s.rgPreamp);
  p.setCrossfeed(s.cfMode, Math.round(s.cfDirect * 10));
  p.setPbe(s.pbe, -Math.round(s.pbePrecut * 10));
  p.setSurround(s.surDelay, s.surBalance, 0, 0);
  p.setCompressor(s.compThresh, 0, s.compRatio, 0, 0, 0);
  p.setChannelMode(s.channel);
  p.setStereoWidth(s.width);
}
