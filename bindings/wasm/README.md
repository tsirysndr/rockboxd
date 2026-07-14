# rockbox-wasm

The [Rockbox](https://www.rockbox.org) audio **decoders** and **DSP** compiled
to WebAssembly, wrapped in a small, batteries-included browser player.

![rockbox-wasm player — transport, seek, volume and 10-band EQ in the browser](https://raw.githubusercontent.com/tsirysndr/rockboxd/master/bindings/wasm/preview.png)

- 🎧 **Decodes** FLAC, MP3, Vorbis, Opus, ALAC, AAC, WavPack, WMA, APE, Musepack,
  WAV/AIFF and more — the real Rockbox codecs, not the browser's.
- 🎚️ **10-band parametric EQ**, tone controls, ReplayGain, channel mixing,
  stereo width, surround and a compressor — the Rockbox DSP chain.
- 📻 **Live internet radio** (Icecast/SHOUTcast) with ICY now-playing metadata.
- 📦 **The wasm is bundled in** — one `npm install`, nothing to download or
  serve separately.
- ✅ **No special server headers** — single-threaded, so no `SharedArrayBuffer`
  and no COOP/COEP configuration.
- 🧩 Framework-agnostic: a tiny `RockboxPlayer` class over Web Audio.

```js
import { RockboxPlayer } from "rockbox-wasm";

const player = new RockboxPlayer();
await player.init();                       // call from a click
player.setQueue(["https://example.com/song.flac"], true);
```

---

## Install

```sh
npm install rockbox-wasm
# or: pnpm add rockbox-wasm / yarn add rockbox-wasm / bun add rockbox-wasm
```

## Serving the assets

The package ships four files under `dist/` — the ESM facade, the wasm-embedded
core, the decoder worker and the audio worklet. The worker and worklet are
loaded at runtime, so they need to be **reachable by URL**. Two easy options:

### 1. `baseUrl` (works with any setup)

Copy `node_modules/rockbox-wasm/dist/` into a folder your server serves
(e.g. `public/rockbox/`) and point the player at it:

```js
const player = new RockboxPlayer({ baseUrl: "/rockbox" });
```

```json
// package.json — keep the copy in sync
"scripts": {
  "postinstall": "cp -r node_modules/rockbox-wasm/dist public/rockbox"
}
```

### 2. Let your bundler resolve them

With no `baseUrl`, the URLs resolve as siblings of the loaded module via
`import.meta.url`. Modern bundlers (Vite, webpack 5) emit the worker/worklet as
assets automatically. If a bundler doesn't, fall back to `baseUrl`.

### 3. From a CDN (no build step)

```html
<script type="module">
  import { RockboxPlayer } from "https://esm.sh/rockbox-wasm";
  const player = new RockboxPlayer({
    baseUrl: "https://esm.sh/rockbox-wasm/dist",
  });
</script>
```

## Quick start

```js
import { RockboxPlayer } from "rockbox-wasm";

const player = new RockboxPlayer({ baseUrl: "/rockbox" });

document.querySelector("#play").addEventListener("click", async () => {
  await player.init();                    // boots on the user gesture
  player.setQueue(["/audio/song.flac"], /* autoplay */ true);
});

player.on("progress", (p) => console.log(`${p.elapsed_ms} / ${p.duration_ms} ms`));
player.on("track", (t) => console.log("now playing:", t.metadata?.title ?? t.url));
```

> `init()` (and the first `play()`) must be reachable from a user gesture —
> browsers won't start an `AudioContext` otherwise.

There's a full **React + TypeScript + Vite + Tailwind** example in
[`example/`](./example) — all controls (transport, EQ, the whole DSP chain)
with settings persisted to localStorage via jotai.

## Live radio

Queue a stream URL — no `Content-Length` is detected as a live stream, decoded
continuously, and reported with `live: true` and (when the station exposes it
over CORS) the ICY `StreamTitle` as now-playing metadata.

```js
player.setQueue(["https://ice.example.com/stream.mp3"], true);
player.on("track", (t) => {
  if (t.live) console.log("📻", t.metadata?.artist, "–", t.metadata?.title);
});
```

## API

### Constructor

```ts
new RockboxPlayer({
  baseUrl?,                       // where the dist files are served from
  coreUrl?, workletUrl?, workerUrl?,   // or override each individually
})
```

### Lifecycle & transport

| Method                              | Description                                |
| ----------------------------------- | ------------------------------------------ |
| `await init()`                      | Boot the audio graph + worker (from a click) |
| `setQueue(urls, autoplay?)`         | Replace the queue                          |
| `enqueue(url)` / `clearQueue()`     | Append / empty the queue                   |
| `play()` `pause()` `toggle()` `stop()` | Transport                               |
| `next()` `prev()` `skipTo(i)` `seek(ms)` | Navigation                            |
| `setShuffle(bool)`                  | Toggle shuffle                             |
| `setRepeat(mode)`                   | `RepeatMode.Off` \| `.One` \| `.All`       |
| `setVolume(0..1)`                   | Output volume (Web Audio gain)             |

### DSP / equalizer

| Method                                              | Notes                              |
| --------------------------------------------------- | ---------------------------------- |
| `setEqEnabled(bool)`                                | 10-band parametric EQ              |
| `setEqBand(band, cutoffHz, q, gainDb)`              | band 0–9; gain/Q in **plain** units |
| `setEqPrecut(db)`                                   | headroom before the EQ             |
| `setTone(bassDb, trebleDb)`                         | shelving tone controls             |
| `setReplaygain(mode, noclip, preampDb)`             | `ReplayGainMode.Off\|.Track\|.Album\|.Shuffle` |
| `setChannelMode(mode)` / `setStereoWidth(pct)`      | `ChannelMode.Stereo\|.Mono\|…`     |
| `setSurround(delayMs, balance, fx1, fx2)`           | Haas surround                      |
| `setCompressor(thr, makeup, ratio, knee, rel, atk)` | dynamic-range compressor           |

Settings-value **enums** are exported (and each setter also accepts the raw
int):

```ts
import { RockboxPlayer, RepeatMode, ReplayGainMode, ChannelMode } from "rockbox-wasm";

player.setRepeat(RepeatMode.All);
player.setReplaygain(ReplayGainMode.Album, /* noclip */ true, /* preamp dB */ 0);
player.setChannelMode(ChannelMode.Mono);
```

`RockboxPlayer.EQ_BAND_CUTOFFS` gives the 10 default band centre frequencies.
DSP/EQ settings persist to `localStorage` and re-apply on the next `init()`.

### Events

`player.on(event, cb)` / `player.off(event, cb)`:

| Event      | Payload                                                          |
| ---------- | --------------------------------------------------------------- |
| `status`   | `{ state, index, queue_len, shuffle, repeat }`                  |
| `track`    | `{ index, url, live, metadata }`                                |
| `progress` | `{ state, index, live, elapsed_ms, duration_ms, metadata }`     |
| `queue`    | `{ urls, index }`                                               |
| `error`    | `{ message, index? }`                                           |

`state` is `"stopped" | "playing" | "paused"`; `repeat` is a `RepeatMode`. Full
types ship in `index.d.ts`.

## Supported formats

Everything in Rockbox's default codec set: **FLAC, MP3, Ogg Vorbis, Opus,
ALAC, AAC, WavPack, WMA / WMA Pro, Monkey's Audio (APE), True Audio (TTA),
Musepack, Speex, AC3, ADX, and the WAV / AIFF / AU / ADPCM family.**

Sources are `http(s)://` URLs (subject to CORS) or same-origin files.

## How it works

```
your code ── RockboxPlayer ──▶ AudioContext + GainNode (volume)
                  │  postMessage
                  ▼
          decoder Worker  ── rockbox-core.wasm (codecs + DSP + metadata)
                  │  PCM via a MessagePort
                  ▼
             AudioWorklet ──▶ speakers
```

The worker decodes **synchronously** off the main thread and streams PCM to the
AudioWorklet over a `MessagePort`. Because there are no wasm threads and no
`SharedArrayBuffer`, the page needs **no COOP/COEP headers**. The `.wasm` is
embedded in `rockbox-core.js` (Emscripten `SINGLE_FILE`), so there's no separate
binary to fetch or serve.

## Notes & limitations

- **Live streams** aren't seekable and report `duration_ms: 0`.
- Volume is a Web Audio `GainNode`, not a Rockbox DSP stage.
- ICY now-playing metadata only appears when the station allows it over CORS.

## License

GPL-2.0-or-later — the compiled sources are Rockbox firmware code.

Built from the [rockboxd](https://github.com/tsirysndr/rockboxd)
`bindings/wasm` package.
