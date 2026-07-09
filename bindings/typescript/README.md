# rockbox-ffi (TypeScript)

[![npm](https://img.shields.io/npm/v/rockbox-ffi?logo=npm&logoColor=white)](https://www.npmjs.com/package/rockbox-ffi)
![TypeScript](https://img.shields.io/badge/TypeScript-5.7-3178C6?logo=typescript&logoColor=white)
![Bun](https://img.shields.io/badge/Bun-ready-000000?logo=bun&logoColor=white)
![Deno](https://img.shields.io/badge/Deno-ready-000000?logo=deno&logoColor=white)
![Node.js](https://img.shields.io/badge/Node.js-ready-5FA04E?logo=nodedotjs&logoColor=white)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

TypeScript bindings for the Rockbox **DSP**, **metadata**, and **playback**
engine, using native FFI. Works under **Bun** (`bun:ffi`), **Deno**
(`Deno.dlopen`), and **Node.js** (via [`koffi`](https://koffi.dev)) — one
shared high-level API over a thin per-runtime backend.

## Setup

Build the shared library once (from the repo root):

```sh
cargo build --release -p rockbox-ffi
```

The library is located automatically by walking up to
`target/release/librockbox_ffi.{dylib,so}`; override with `ROCKBOX_FFI_LIB`.

## Usage

Import the module for your runtime (all export the same API):

```ts
// Bun
import { Dsp, Player, metadata, sineStereo } from "rockbox-ffi/bun";
// Deno
import { Dsp, Player, metadata, sineStereo } from "./src/deno.ts";
// Node.js  (requires `npm install koffi`)
import { Dsp, Player, metadata, sineStereo } from "rockbox-ffi/node";

import { DspReplayGainMode, ReplayGainMode, CrossfadeMode } from "rockbox-ffi/bun";

// --- metadata --------------------------------------------------------
const meta = metadata.read("song.flac");
console.log(meta.artist, "—", meta.title, meta.duration_ms, "ms");
console.log(metadata.probe("track.opus")); // -> "Opus"

// --- DSP (interleaved stereo Int16) ----------------------------------
const dsp = new Dsp(44100);
dsp.eqEnable(true);
dsp.setEqBand(0, 60, 0.7, 3.0);
dsp.setReplaygain(DspReplayGainMode.TRACK, true, 0.0);
dsp.setReplaygainGains(-6.02); // halves amplitude
const processed = dsp.process(samples); // Int16Array
dsp.close();

// --- playback (needs an output device) -------------------------------
const player = new Player({ volume: 0.8, crossfadeMode: CrossfadeMode.ALWAYS });
player.setReplaygain(ReplayGainMode.TRACK, 0.0, true);
player.setQueue(["a.flac", "b.mp3", "c.opus"]);
player.play();
console.log(player.status()); // { state: "playing", index: 0, ... }
```

Both `Dsp` and `Player` implement `Symbol.dispose`, so you can also write:

```ts
using dsp = new Dsp(44100);
```

## Runtime auto-detection

`import { load } from "rockbox-ffi"` returns a promise that resolves to the
right backend (via dynamic import, so the other runtime's FFI module is never
parsed):

```ts
const { Dsp, metadata } = await load();
```

## Two ReplayGain encodings

The DSP and player use *different* mode integers (a quirk of the C ABI):

- `Dsp.setReplaygain` → `DspReplayGainMode` (`TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3`)
- `Player.setReplaygain` → `ReplayGainMode` (`OFF=0, TRACK=1, ALBUM=2`)

## Verify

```sh
bun run examples/smoke.bun.ts
deno run --allow-ffi --allow-read --allow-env examples/smoke.deno.ts
npm install && npx tsx examples/smoke.node.ts   # Node.js (installs koffi)
bunx tsc --noEmit                               # type check
```

## Build & publish to npm

The package is authored in TypeScript and published as bundled ESM + type
declarations under `dist/`:

```sh
bun install
bun run build          # -> dist/*.js + dist/*.d.ts  (Bun.build + tsc)
npm publish            # runs `prepublishOnly` -> build automatically
```

`bun run build` bundles the four runtime entry points (`index` / `bun` /
`deno` / `node`) with Bun's bundler, keeping `bun:ffi` and `koffi` external,
then emits `.d.ts` with `tsc`. Consumers get the right backend automatically
through the package's `exports` map (`.`, `./bun`, `./deno`, `./node`).

## Memory

Heap allocations crossing the FFI boundary (JSON strings, sample buffers)
are freed inside the wrappers — you never call a `*_free` yourself.
