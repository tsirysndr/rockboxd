import { useState, type ReactNode } from "react";
import { RockboxPlayer, ReplayGainMode, ChannelMode, CrossfeedMode } from "rockbox-wasm";

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
  min: number; max: number; step?: number; value: number;
  onChange: (v: number) => void;
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
        const raw = e.target.value;
        const opt = props.options.find(([v]) => String(v) === raw);
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

// ── the panel ────────────────────────────────────────────────────────────────
export function DspPanel({ apply }: { apply: Apply }) {
  // ReplayGain
  const [rgMode, setRgMode] = useState<ReplayGainMode>(ReplayGainMode.Off);
  const [rgPreamp, setRgPreamp] = useState(0);
  const [rgNoclip, setRgNoclip] = useState(false);
  const rg = (mode = rgMode, preamp = rgPreamp, noclip = rgNoclip) =>
    apply((p) => p.setReplaygain(mode, noclip, preamp));

  // Tone (bass + treble go together)
  const [bass, setBass] = useState(0);
  const [treble, setTreble] = useState(0);
  const tone = (b = bass, t = treble) => apply((p) => p.setTone(b, t));

  // EQ precut
  const [precut, setPrecut] = useState(0);

  // Crossfeed
  const [cfMode, setCfMode] = useState<CrossfeedMode>(CrossfeedMode.Off);
  const [cfDirect, setCfDirect] = useState(-1.5); // dB
  const cf = (mode = cfMode, direct = cfDirect) =>
    apply((p) => p.setCrossfeed(mode, Math.round(direct * 10)));

  // PBE
  const [pbe, setPbe] = useState(0);
  const [pbePrecut, setPbePrecut] = useState(0); // dB of headroom
  const applyPbe = (s = pbe, pc = pbePrecut) =>
    apply((p) => p.setPbe(s, -Math.round(pc * 10)));

  // Haas surround
  const [surDelay, setSurDelay] = useState(0);
  const [surBalance, setSurBalance] = useState(35);
  const sur = (d = surDelay, b = surBalance) => apply((p) => p.setSurround(d, b, 0, 0));

  // Compressor
  const [compThresh, setCompThresh] = useState(0); // 0 = off
  const [compRatio, setCompRatio] = useState(2);
  const comp = (thr = compThresh, ratio = compRatio) =>
    apply((p) => p.setCompressor(thr, 0, ratio, 0, 0, 0));

  // Channel / width
  const [channel, setChannel] = useState<ChannelMode>(ChannelMode.Stereo);
  const [width, setWidth] = useState(100);

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
    </div>
  );
}
