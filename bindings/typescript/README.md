# rockbox-ffi (TypeScript)

[![npm](https://img.shields.io/npm/v/rockbox-ffi?logo=npm&logoColor=white)](https://www.npmjs.com/package/rockbox-ffi)
![TypeScript](https://img.shields.io/badge/TypeScript-5.7-3178C6?logo=typescript&logoColor=white)
![Bun](https://img.shields.io/badge/Bun-ready-000000?logo=bun&logoColor=white)
![Deno](https://img.shields.io/badge/Deno-ready-000000?logo=deno&logoColor=white)
![Node.js](https://img.shields.io/badge/Node.js-ready-5FA04E?logo=nodedotjs&logoColor=white)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

TypeScript bindings for the [Rockbox](https://www.rockbox.org) audio engine —
**metadata** parsing (40+ formats), the **DSP** pipeline (EQ, tone, surround,
compressor, ReplayGain, resampler), and a queue-based **player** with
crossfade. One typed API, three runtimes: **Bun**, **Deno**, and **Node.js**.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

```ts
import { metadata, Dsp } from "rockbox-ffi/bun";

const meta = metadata.read("song.flac");
console.log(`${meta.artist} — ${meta.title} (${meta.duration_ms} ms)`);
```

## Install

```sh
npm  install rockbox-ffi        # Node.js  (also pulls in koffi)
bun  add     rockbox-ffi        # Bun
deno add npm:rockbox-ffi        # Deno   (or import npm:rockbox-ffi/deno directly)
```

Then import the entry point for your runtime — all three expose the identical
API:

| Runtime | Import                                 | Notes                                            |
| ------- | -------------------------------------- | ------------------------------------------------ |
| Bun     | `import … from "rockbox-ffi/bun"`      | uses built-in `bun:ffi`                          |
| Deno    | `import … from "npm:rockbox-ffi/deno"` | run with `--allow-ffi --allow-read --allow-env`  |
| Node.js | `import … from "rockbox-ffi/node"`     | uses [`koffi`](https://koffi.dev) (a dependency) |

> Prefer a single import? `import { load } from "rockbox-ffi"` returns a
> promise that resolves the correct backend at runtime (see
> [Runtime auto-detection](#runtime-auto-detection)).

### The native library

These bindings call into the Rockbox engine through a native shared library
(`librockbox_ffi.dylib` / `.so`). Point the package at it with the
`ROCKBOX_FFI_LIB` environment variable:

```sh
export ROCKBOX_FFI_LIB=/path/to/librockbox_ffi.dylib
```

If unset, the loader searches upward from the package for a
`target/release/librockbox_ffi.{dylib,so}` (handy when working inside the
Rockbox repo). See [Building the native library](#building-the-native-library).

## Quick start

```ts
import {
  metadata,
  Dsp,
  Player,
  sineStereo,
  DspReplayGainMode,
  ReplayGainMode,
  CrossfadeMode,
} from "rockbox-ffi/bun"; // or /node, or npm:rockbox-ffi/deno

// --- metadata --------------------------------------------------------
const meta = metadata.read("song.flac");
console.log(meta.artist, "—", meta.title, `${meta.duration_ms} ms`);
console.log(metadata.probe("track.opus")); // "Opus"  (extension guess, no I/O)

// --- DSP: process interleaved stereo Int16 ---------------------------
const dsp = new Dsp(44_100);
dsp.eqEnable(true);
dsp.setEqBand(0, /*cutoffHz*/ 60, /*q*/ 0.7, /*gainDb*/ 3.0);
dsp.setReplaygain(DspReplayGainMode.TRACK, /*noclip*/ true, /*preampDb*/ 0.0);
dsp.setReplaygainGains(/*trackGainDb*/ -6.02); // −6 dB ≈ half amplitude

const input = sineStereo(1_000, 1.0, 44_100); // 1 s of a 1 kHz test tone
const output: Int16Array = dsp.process(input);
dsp.close();

// --- playback (needs an audio output device) -------------------------
const player = new Player({ volume: 0.8, crossfadeMode: CrossfadeMode.ALWAYS });
player.setReplaygain(ReplayGainMode.TRACK, 0.0, true);
player.setQueue(["a.flac", "b.mp3", "c.opus"]);
player.play();
console.log(player.status()); // { state: "playing", index: 0, ... }
```

## API

Everything is fully typed — the tables below are a map; your editor has the
details.

### `metadata`

| Function                   | Returns          | Description                                               |
| -------------------------- | ---------------- | -------------------------------------------------------- |
| `metadata.read(path)`      | `Metadata`       | Parse tags, duration, ReplayGain, album-art/cue offsets  |
| `metadata.probe(filename)` | `string \| null` | Codec label from the extension, without opening the file |

### `Dsp`

Interleaved-S16LE-stereo processor. Construct with a sample rate, feed it
`Int16Array`s, and `close()` (or `using`) when done.

```ts
const dsp = new Dsp(sampleRate: number);
```

| Method                                                           | Description                                                  |
| ---------------------------------------------------------------- | ------------------------------------------------------------ |
| `process(samples: Int16Array): Int16Array`                       | Run stereo S16 frames through the pipeline                   |
| `setInputFrequency(hz)`                                          | Change input rate (engages the resampler)                    |
| `eqEnable(on)` / `setEqBand(band, cutoffHz, q, gainDb)`          | 10-band EQ (band 0 low-shelf, 9 high-shelf)                  |
| `setEqPrecut(db)`                                                | Negative pre-gain to avoid EQ clipping                       |
| `setTone(bassDb, trebleDb)` / `setToneCutoffs(bHz, tHz)`         | Bass/treble shelves                                          |
| `setSurround(delayMs, balance, fx1, fx2)`                        | Haas surround (`delayMs > 0` enables)                        |
| `setChannelConfig(mode)` / `setStereoWidth(pct)`                 | Channel mode / custom width                                  |
| `setCompressor(threshold, makeup, ratio, knee, rel, atk)`        | Dynamic-range compressor (`threshold 0` = off)               |
| `setReplaygain(mode, noclip, preampDb)`                          | ReplayGain mode (see [encodings](#two-replaygain-encodings)) |
| `setReplaygainGains(trackDb?, albumDb?, trackPeak?, albumPeak?)` | Per-track gains in dB (omit = absent)                        |
| `setReplaygainGainsRaw(tg, ag, tp, ap: bigint)`                  | Native Q7.24 gains (the `raw_*` metadata fields)             |
| `flush()` / `close()`                                            | Drop buffered samples / free the handle                      |

### `Player`

Queue-based player backed by a live audio device and a background thread.

```ts
const player = new Player(config?: PlayerConfig);
```

| Method                                                         | Description                              |
| -------------------------------------------------------------- | ---------------------------------------- |
| `setQueue(paths)` / `enqueue(path)`                            | Replace / append to the queue            |
| `play()` `pause()` `toggle()` `stop()`                         | Transport                                |
| `next()` `previous()` `skipTo(index)` `seekMs(ms)`             | Navigation                               |
| `setVolume(v)` / `volume()`                                    | Volume, `0.0`–`1.0`                      |
| `sampleRate()`                                                 | Output rate everything resamples to      |
| `setCrossfade(mode, foDelay?, foDur?, fiDelay?, fiDur?, mix?)` | Configure crossfade                      |
| `setReplaygain(mode, preampDb, preventClipping)`               | ReplayGain (player encoding)             |
| `status(): PlayerStatus`                                       | Snapshot: state, index, position, queue… |
| `close()`                                                      | Stop playback and free the handle        |

`PlayerConfig` (all optional): `sampleRate` (`0` = device default),
`bufferSeconds`, `volume`, `replaygainMode`, `replaygainPreampDb`,
`replaygainPreventClipping`, `crossfadeMode`, `fadeOutDelayMs`,
`fadeOutDurationMs`, `fadeInDelayMs`, `fadeInDurationMs`, `mixMode`.

### Enums & helpers

`DspReplayGainMode`, `ReplayGainMode`, `CrossfadeMode`, `MixMode`,
`ChannelConfig`, and `sineStereo(freqHz, seconds, rate, amplitude?)` for
generating test tones. `abiVersion()` returns the native ABI version.

### Types

`Metadata`, `PlayerStatus`, `ReplayGain`, `AlbumArt`, `Cuesheet`,
`PlayerConfig` are exported for annotations. JSON field names are snake_case
(e.g. `duration_ms`, `sample_rate`, `replaygain.raw_track_gain`).

## Resource management

`Dsp` and `Player` hold native handles. Free them with `close()`, or let
`using` do it automatically (both implement `Symbol.dispose`):

```ts
using dsp = new Dsp(44_100);
// dsp.close() runs at end of scope
```

Everything else that crosses the FFI boundary (JSON strings, sample buffers)
is freed inside the wrappers — you never call a `*_free` yourself.

> **Note:** the `Dsp` wraps a process-wide singleton, so keep only one alive
> at a time and use it from a single thread.

## Two ReplayGain encodings

The DSP and player take *different* mode integers (a quirk of the underlying
C ABI) — use the named enums and you won't have to remember which:

- `Dsp.setReplaygain` → `DspReplayGainMode` (`TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3`)
- `Player.setReplaygain` → `ReplayGainMode` (`OFF=0, TRACK=1, ALBUM=2`)

## Runtime auto-detection

If you don't want to hard-code the backend, import `load` from the package
root; it dynamically imports the right module (so the other runtimes' FFI code
is never parsed):

```ts
import { load } from "rockbox-ffi";

const { metadata, Dsp, Player } = await load();
```

---

## Developing

The rest is only relevant when hacking on the bindings themselves inside the
[Rockbox repo](https://github.com/tsirysndr/rockboxd).

### Building the native library

```sh
cargo build --release -p rockbox-ffi
#  target/release/librockbox_ffi.dylib   (macOS)
#  target/release/librockbox_ffi.so      (Linux)
```

The loader finds this automatically when the package sits inside the repo;
elsewhere, set `ROCKBOX_FFI_LIB`.

### Running the smoke tests

```sh
bun  run examples/smoke.bun.ts
deno run --allow-ffi --allow-read --allow-env examples/smoke.deno.ts
npm  install && npx tsx examples/smoke.node.ts
bunx tsc --noEmit                                # type check
```

### Building & publishing

The package is authored in TypeScript and shipped as bundled ESM + `.d.ts`
under `dist/`:

```sh
bun install
bun run build      # Bun.build + tsc  ->  dist/*.js + dist/*.d.ts
npm publish        # prepublishOnly runs the build automatically
```

`build` bundles the four entry points (`index` / `bun` / `deno` / `node`),
keeping `bun:ffi` and `koffi` external, so consumers get the right backend via
the package `exports` map (`.`, `./bun`, `./deno`, `./node`).
