import { type ReactNode } from "react";
import { useAtom } from "jotai";
import {
  RockboxPlayer,
  ReplayGainMode,
  ChannelMode,
  CrossfeedMode,
  CrossfadeMode,
  CrossfadeMixMode,
} from "rockbox-wasm";
import * as S from "./settings";

/** Runs `fn` with the (booted) player — boots on first use. */
type Apply = (fn: (p: RockboxPlayer) => void) => void | Promise<void>;

// ── little Tailwind primitives ──────────────────────────────────────────────
function Field({ label, value, children }: { label: string; value?: ReactNode; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="flex justify-between text-xs text-zinc-400">
        <span>{label}</span>
        {value != null && <span className="font-mono text-zinc-200">{value}</span>}
      </span>
      {children}
    </label>
  );
}

function Slider(props: {
  min: number; max: number; step?: number; value: number; onChange: (v: number) => void;
}) {
  return (
    <input
      type="range"
      min={props.min}
      max={props.max}
      step={props.step ?? 1}
      value={props.value}
      onChange={(e) => props.onChange(Number(e.target.value))}
      className="accent-indigo-500"
    />
  );
}

function Select<T extends string | number>(props: {
  value: T; options: [T, string][]; onChange: (v: T) => void;
}) {
  return (
    <select
      value={String(props.value)}
      onChange={(e) => {
        const opt = props.options.find(([v]) => String(v) === e.target.value);
        if (opt) props.onChange(opt[0]);
      }}
      className="rounded-lg border border-zinc-700 bg-zinc-800 px-2 py-1.5 text-sm text-zinc-100"
    >
      {props.options.map(([v, label]) => (
        <option key={String(v)} value={String(v)}>{label}</option>
      ))}
    </select>
  );
}

const Card = ({ title, children }: { title: string; children: ReactNode }) => (
  <div className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4">
    <h3 className="mb-3 text-sm font-semibold text-zinc-200">{title}</h3>
    <div className="flex flex-col gap-3">{children}</div>
  </div>
);

// ── the panel — all state is persisted via jotai atomWithStorage ─────────────
export function DspPanel({ apply }: { apply: Apply }) {
  const [rgMode, setRgMode] = useAtom(S.rgModeAtom);
  const [rgPreamp, setRgPreamp] = useAtom(S.rgPreampAtom);
  const [rgNoclip, setRgNoclip] = useAtom(S.rgNoclipAtom);
  const rg = (mode = rgMode, preamp = rgPreamp, noclip = rgNoclip) =>
    apply((p) => p.setReplaygain(mode, noclip, preamp));

  const [bass, setBass] = useAtom(S.bassAtom);
  const [treble, setTreble] = useAtom(S.trebleAtom);
  const tone = (b = bass, t = treble) => apply((p) => p.setTone(b, t));

  const [precut, setPrecut] = useAtom(S.eqPrecutAtom);

  const [cfMode, setCfMode] = useAtom(S.cfModeAtom);
  const [cfDirect, setCfDirect] = useAtom(S.cfDirectAtom);
  const cf = (mode = cfMode, direct = cfDirect) =>
    apply((p) => p.setCrossfeed(mode, Math.round(direct * 10)));

  const [pbe, setPbe] = useAtom(S.pbeAtom);
  const [pbePrecut, setPbePrecut] = useAtom(S.pbePrecutAtom);
  const applyPbe = (s = pbe, pc = pbePrecut) => apply((p) => p.setPbe(s, -Math.round(pc * 10)));

  const [surDelay, setSurDelay] = useAtom(S.surDelayAtom);
  const [surBalance, setSurBalance] = useAtom(S.surBalanceAtom);
  const sur = (d = surDelay, b = surBalance) => apply((p) => p.setSurround(d, b, 0, 0));

  const [compThresh, setCompThresh] = useAtom(S.compThreshAtom);
  const [compRatio, setCompRatio] = useAtom(S.compRatioAtom);
  const comp = (thr = compThresh, ratio = compRatio) =>
    apply((p) => p.setCompressor(thr, 0, ratio, 0, 0, 0));

  const [channel, setChannel] = useAtom(S.channelAtom);
  const [width, setWidth] = useAtom(S.widthAtom);

  // Crossfade (Rockbox pcmbuf)
  const [xfMode, setXfMode] = useAtom(S.xfModeAtom);
  const [xfFoDelay, setXfFoDelay] = useAtom(S.xfFoDelayAtom);
  const [xfFoDur, setXfFoDur] = useAtom(S.xfFoDurAtom);
  const [xfFiDelay, setXfFiDelay] = useAtom(S.xfFiDelayAtom);
  const [xfFiDur, setXfFiDur] = useAtom(S.xfFiDurAtom);
  const [xfMix, setXfMix] = useAtom(S.xfMixAtom);
  const xf = (over: Partial<{
    mode: CrossfadeMode; foDelay: number; foDur: number;
    fiDelay: number; fiDur: number; mix: CrossfadeMixMode;
  }> = {}) =>
    apply((p) =>
      p.setCrossfade(over.mode ?? xfMode, {
        fadeOutDelay: over.foDelay ?? xfFoDelay,
        fadeOutDuration: over.foDur ?? xfFoDur,
        fadeInDelay: over.fiDelay ?? xfFiDelay,
        fadeInDuration: over.fiDur ?? xfFiDur,
        mixMode: over.mix ?? xfMix,
      }),
    );

  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <Card title="ReplayGain">
        <Field label="Mode">
          <Select<ReplayGainMode>
            value={rgMode}
            options={[
              [ReplayGainMode.Off, "Off"],
              [ReplayGainMode.Track, "Track"],
              [ReplayGainMode.Album, "Album"],
              [ReplayGainMode.Shuffle, "Shuffle"],
            ]}
            onChange={(v) => { setRgMode(v); rg(v); }}
          />
        </Field>
        <Field label="Pre-amp" value={`${rgPreamp.toFixed(1)} dB`}>
          <Slider min={-12} max={12} step={0.5} value={rgPreamp}
            onChange={(v) => { setRgPreamp(v); rg(rgMode, v); }} />
        </Field>
        <label className="flex items-center gap-2 text-sm text-zinc-400">
          <input type="checkbox" checked={rgNoclip} className="accent-indigo-500"
            onChange={(e) => { setRgNoclip(e.target.checked); rg(rgMode, rgPreamp, e.target.checked); }} />
          Prevent clipping
        </label>
      </Card>

      <Card title="Tone">
        <Field label="Bass" value={`${bass} dB`}>
          <Slider min={-24} max={24} value={bass} onChange={(v) => { setBass(v); tone(v, treble); }} />
        </Field>
        <Field label="Treble" value={`${treble} dB`}>
          <Slider min={-24} max={24} value={treble} onChange={(v) => { setTreble(v); tone(bass, v); }} />
        </Field>
        <Field label="EQ pre-cut" value={`${precut} dB`}>
          <Slider min={0} max={24} value={precut}
            onChange={(v) => { setPrecut(v); apply((p) => p.setEqPrecut(v)); }} />
        </Field>
      </Card>

      <Card title="Crossfeed (headphones)">
        <Field label="Mode">
          <Select<CrossfeedMode>
            value={cfMode}
            options={[
              [CrossfeedMode.Off, "Off"],
              [CrossfeedMode.Meier, "Meier"],
              [CrossfeedMode.Custom, "Custom"],
            ]}
            onChange={(v) => { setCfMode(v); cf(v); }}
          />
        </Field>
        <Field label="Direct gain" value={`${cfDirect.toFixed(1)} dB`}>
          <Slider min={-6} max={0} step={0.5} value={cfDirect}
            onChange={(v) => { setCfDirect(v); cf(cfMode, v); }} />
        </Field>
      </Card>

      <Card title="Perceptual Bass (PBE)">
        <Field label="Strength" value={`${pbe}%`}>
          <Slider min={0} max={100} value={pbe} onChange={(v) => { setPbe(v); applyPbe(v); }} />
        </Field>
        <Field label="Pre-cut" value={`-${pbePrecut} dB`}>
          <Slider min={0} max={24} value={pbePrecut}
            onChange={(v) => { setPbePrecut(v); applyPbe(pbe, v); }} />
        </Field>
      </Card>

      <Card title="Haas surround">
        <Field label="Delay (0 = off)" value={`${surDelay} ms`}>
          <Slider min={0} max={30} value={surDelay} onChange={(v) => { setSurDelay(v); sur(v, surBalance); }} />
        </Field>
        <Field label="Balance" value={`${surBalance}%`}>
          <Slider min={0} max={100} value={surBalance} onChange={(v) => { setSurBalance(v); sur(surDelay, v); }} />
        </Field>
      </Card>

      <Card title="Compressor">
        <Field label="Threshold (0 = off)" value={`${compThresh} dB`}>
          <Slider min={-30} max={0} value={compThresh} onChange={(v) => { setCompThresh(v); comp(v, compRatio); }} />
        </Field>
        <Field label="Ratio">
          <Select<number>
            value={compRatio}
            options={[[2, "2:1"], [4, "4:1"], [6, "6:1"], [10, "10:1"]]}
            onChange={(v) => { setCompRatio(v); comp(compThresh, v); }}
          />
        </Field>
      </Card>

      <Card title="Channel">
        <Field label="Mode">
          <Select<ChannelMode>
            value={channel}
            options={[
              [ChannelMode.Stereo, "Stereo"],
              [ChannelMode.Mono, "Mono"],
              [ChannelMode.Custom, "Custom"],
              [ChannelMode.MonoLeft, "Mono left"],
              [ChannelMode.MonoRight, "Mono right"],
              [ChannelMode.Karaoke, "Karaoke"],
              [ChannelMode.Swap, "Swap L/R"],
            ]}
            onChange={(v) => { setChannel(v); apply((p) => p.setChannelMode(v)); }}
          />
        </Field>
        <Field label="Stereo width" value={`${width}%`}>
          <Slider min={0} max={255} value={width}
            onChange={(v) => { setWidth(v); apply((p) => p.setStereoWidth(v)); }} />
        </Field>
      </Card>

      <Card title="Crossfade">
        <Field label="Mode">
          <Select<CrossfadeMode>
            value={xfMode}
            options={[
              [CrossfadeMode.Off, "Off"],
              [CrossfadeMode.AutoSkip, "Auto track change"],
              [CrossfadeMode.ManualSkip, "Manual skip"],
              [CrossfadeMode.Shuffle, "Shuffle"],
              [CrossfadeMode.ShuffleOrManualSkip, "Shuffle or manual skip"],
              [CrossfadeMode.Always, "Always"],
            ]}
            onChange={(v) => { setXfMode(v); xf({ mode: v }); }}
          />
        </Field>
        <Field label="Fade-out delay" value={`${xfFoDelay} s`}>
          <Slider min={0} max={7} value={xfFoDelay}
            onChange={(v) => { setXfFoDelay(v); xf({ foDelay: v }); }} />
        </Field>
        <Field label="Fade-out duration" value={`${xfFoDur} s`}>
          <Slider min={0} max={15} value={xfFoDur}
            onChange={(v) => { setXfFoDur(v); xf({ foDur: v }); }} />
        </Field>
        <Field label="Fade-in delay" value={`${xfFiDelay} s`}>
          <Slider min={0} max={7} value={xfFiDelay}
            onChange={(v) => { setXfFiDelay(v); xf({ fiDelay: v }); }} />
        </Field>
        <Field label="Fade-in duration" value={`${xfFiDur} s`}>
          <Slider min={0} max={15} value={xfFiDur}
            onChange={(v) => { setXfFiDur(v); xf({ fiDur: v }); }} />
        </Field>
        <Field label="Fade-out mode">
          <Select<CrossfadeMixMode>
            value={xfMix}
            options={[
              [CrossfadeMixMode.Crossfade, "Crossfade (both fade)"],
              [CrossfadeMixMode.Mix, "Mix (outgoing stays full)"],
            ]}
            onChange={(v) => { setXfMix(v); xf({ mix: v }); }}
          />
        </Field>
      </Card>
    </div>
  );
}
