# Rockbox WASM — web player

A tiny browser player built on the Rockbox **decode + DSP core** compiled to
WebAssembly. The WASM module only *decodes*, runs the *DSP/EQ*, and reads
*metadata*; the player itself (queue, transport, scheduling, output) is plain
JavaScript.

See the top-level [`WEBASSEMBLY.md`](../WEBASSEMBLY.md) for the full build and
architecture writeup. This file is the quick reference for the `web/` example.

## Directory contents

| File                          | Role                                                             |
| ----------------------------- | --------------------------------------------------------------- |
| `index.html`                  | Demo UI (Outfit + Lexend + JetBrains Mono); EQ + DSP panel      |
| `rockbox.js`                  | `RockboxPlayer` — main-thread facade (audio graph, events)      |
| `rockbox-decoder-worker.js`   | Decoder Worker — owns the WASM module, decode/DSP, queue        |
| `rockbox-audio-worklet.js`    | `AudioWorkletProcessor` — plays the shared PCM ring             |
| `rockbox-core.js` / `.wasm`   | Emscripten build of `rockbox-ffi` (generated — do not edit)     |

## Architecture

```
 main thread      rockbox.js         UI facade · AudioContext · GainNode · events
      │  cmd ▼                              ▲ event │
 decoder Worker   rockbox-decoder-worker.js  owns rockbox-core.wasm
      │  PCM ▼  (SharedArrayBuffer ring)
 AudioWorklet     rockbox-audio-worklet.js   ring → speakers
```

The decoder runs its codec on a pthread and blocks on a `Condvar`, which is
illegal on the main thread — so all decode work is in the Worker, and only PCM
crosses into the audio thread through a lock-free `SharedArrayBuffer` ring.
That needs a **cross-origin-isolated** page (COOP/COEP).

## Quick start

```sh
# From the repo root, after building the core (see below):
node scripts/wasm-dev-server.mjs      # → http://localhost:8090
```

Open the URL, click **Play** (the first click boots the `AudioContext`), paste
an audio URL, and go. The dev server sets the required COOP/COEP headers.

## Building the core

```sh
source /path/to/emsdk/emsdk_env.sh
bash scripts/build-wasm.sh            # → web/rockbox-core.{js,wasm}
```

Prerequisites: the Emscripten SDK and `rustup target add
wasm32-unknown-emscripten`.

## Using `RockboxPlayer`

```js
import { RockboxPlayer } from './rockbox.js';

const player = new RockboxPlayer();
await player.init();                       // from a user gesture

player.setQueue(['song.flac'], true);      // autoplay
player.on('progress', (p) => { /* p.elapsed_ms, p.duration_ms */ });
```

### Transport

`setQueue(urls, autoplay?)`, `enqueue(url)`, `clearQueue()`, `play()`,
`pause()`, `toggle()`, `stop()`, `next()`, `prev()`, `skipTo(i)`, `seek(ms)`,
`setShuffle(bool)`, `setRepeat(0|1|2)`, `setVolume(0..1)`.

### DSP / EQ

Forwarded to `rockbox-dsp` in the Worker. Settings persist to `localStorage`
and are re-applied on the next `init()`.

| Method                                              | Notes                              |
| --------------------------------------------------- | ---------------------------------- |
| `setEqEnabled(bool)`                                | 10-band parametric EQ              |
| `setEqBand(band, cutoffHz, q, gainDb)`              | band 0–9                           |
| `setEqPrecut(db)`                                   | headroom before the EQ            |
| `setTone(bassDb, trebleDb)`                         | shelving tone controls            |
| `setReplaygain(mode, noclip, preampDb)`             | 0 track · 1 album · 2 shuffle · 3 off |
| `setChannelMode(mode)` / `setStereoWidth(pct)`      | channel mixing                    |
| `setSurround(delayMs, balance, fx1, fx2)`           | Haas surround                     |
| `setCompressor(thr, makeup, ratio, knee, rel, atk)` | dynamic-range compressor          |

> `EqBand` cutoffs default to Rockbox's 10 bands
> (`RockboxPlayer.EQ_BAND_CUTOFFS`). The demo uses a fixed Q of 1.0 per band.

### Events

`player.on(event, cb)` / `player.off(event, cb)` for: `status`
(`{state, index, queue_len, shuffle, repeat}`), `track`
(`{index, url, metadata}`), `progress` (`{elapsed_ms, duration_ms, metadata}`),
`queue` (`{urls, index}`), and `error` (`{message}`).

## Troubleshooting

**Nothing happens / no audio.** The `AudioContext` starts suspended until a
user gesture — `init()` and `play()` must be reachable from a click. The demo
boots on the first button press.

**`crossOriginIsolated is false`.** The page isn't cross-origin isolated.
Serve it with the COOP/COEP headers (`scripts/wasm-dev-server.mjs` does this).
Opening `index.html` from `file://` won't work.

**A URL won't load.** Remote files need permissive CORS. The Worker `fetch()`es
the whole file into memory before decoding; check the browser console for the
`error` event message.

**EQ has no effect.** Enable it (`setEqEnabled(true)`) — moving a band slider
in the demo auto-enables it.

**Fonts look like the system default.** Some browsers block the cross-origin
Google Fonts even under `credentialless`; the CSS falls back to system fonts.
