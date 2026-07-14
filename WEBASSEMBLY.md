# Rockbox WASM — Browser Build

A **lightweight** WebAssembly build of Rockbox for the browser. It compiles
only the extracted core crates — the **decoders**, the **DSP**, and the
**metadata parser** — and lets JavaScript be the player.

```
rockbox-codecs    FLAC, MP3, Vorbis, Opus, ALAC, WavPack, AAC, WMA, APE, …
rockbox-dsp       parametric EQ, tone, ReplayGain, resampler, compressor, …
rockbox-metadata  tags + ReplayGain for 40+ formats
        │  (flat C ABI: rockbox-ffi, `player` feature off)
        ▼
   rockbox-core.wasm   ← ~1.2 MB, decode + DSP + metadata only
```

There is **no firmware, no netstream, no playlist engine, no gRPC/HTTP
server, no SDL** — none of the daemon. Queue, transport, scheduling and audio
output all live in ~three small JS files under `web/`.

> This replaces the previous build, which compiled the *entire* Rockbox
> firmware to WASM. The old firmware WASM target
> (`firmware/target/hosted/wasm/`, `tools/configure` target 207) is no longer
> used and can be removed.

## Architecture overview

```
 main thread            web/rockbox.js
   RockboxPlayer         UI facade · AudioContext · GainNode(volume) · events
        │  postMessage(cmd)                         ▲ postMessage(event)
        ▼                                           │
 decoder Worker         web/rockbox-decoder-worker.js
   owns rockbox-core.wasm (RockboxModule)
   queue · transport · fetch → MEMFS → decode → DSP → resample
        │  writes S16LE PCM into a shared ring (SharedArrayBuffer)
        ▼
 AudioWorklet          web/rockbox-audio-worklet.js
   reads the ring → speakers
```

### Why a Worker + SharedArrayBuffer

`rockbox-codecs::Decoder` runs each codec on its **own pthread** and blocks on
a `Condvar` (which lowers to `Atomics.wait`) while waiting for the next chunk.
`Atomics.wait` throws on the main browser thread, so all decode work must
happen off it — hence the dedicated **decoder Worker**.

Because the module is built with `-pthread`, its WebAssembly memory is a
`SharedArrayBuffer`. That requires the page to be **cross-origin isolated**
(COOP/COEP headers — see `scripts/wasm-dev-server.mjs`).

### Audio pipeline

1. JS `fetch()`es the track (in the Worker) and writes the bytes to the
   Emscripten in-memory filesystem (MEMFS).
2. `rb_decoder_open()` opens it; `rb_decoder_next_chunk()` yields interleaved
   S16LE stereo PCM at the codec's native rate.
3. `rb_dsp_process()` applies the EQ/DSP chain **and resamples** to the
   `AudioContext` output rate (`rb_dsp_set_input_frequency` engages the
   resampler when the codec rate differs).
4. The resulting PCM is written into a lock-free ring buffer.
5. The `AudioWorkletProcessor` reads the ring and outputs to the speakers.
   Volume is a Web Audio `GainNode` (not a rockbox-dsp stage).

The ring keeps ~2–3 s of look-ahead; the Worker throttles decode with
`setTimeout(0)` between chunks so it stays responsive to commands.

## Prerequisites

- **Emscripten SDK** (tested with 5.0.x) — `source /path/to/emsdk/emsdk_env.sh`
- **Rust wasm target** — `rustup target add wasm32-unknown-emscripten`

## Build

```sh
source /path/to/emsdk/emsdk_env.sh
bash scripts/build-wasm.sh            # release
bash scripts/build-wasm.sh --debug    # -O0 -g
```

Output:

| File                    | What                                                         |
| ----------------------- | ------------------------------------------------------------ |
| `web/rockbox-core.js`   | Emscripten loader (`MODULARIZE`, `EXPORT_NAME=RockboxModule`) |
| `web/rockbox-core.wasm` | decode + DSP + metadata (~1.2 MB)                             |

The build is just two steps:

1. `cargo rustc -p rockbox-ffi --no-default-features --crate-type staticlib`
   for `wasm32-unknown-emscripten`. `--no-default-features` drops the
   `player` feature (and with it `rockbox-playback` + `cpal`, which need a
   real output device). The codec/DSP/metadata **C** sources are compiled to
   wasm by emcc via the `cc` crate.
2. `emcc` links the staticlib into `rockbox-core.{js,wasm}`.

## Running locally

Serve `web/` with COOP/COEP headers:

```sh
node scripts/wasm-dev-server.mjs      # → http://localhost:8090
```

The dev server sends `Cross-Origin-Opener-Policy: same-origin` and
`Cross-Origin-Embedder-Policy: credentialless`. `credentialless` (rather than
`require-corp`) keeps the page cross-origin isolated while still allowing the
cross-origin webfonts the example uses (Outfit / Lexend / JetBrains Mono).

## JS integration

```js
import { RockboxPlayer } from './rockbox.js';

const player = new RockboxPlayer();
await player.init();                 // call from a user gesture (click)

player.setQueue(['https://example.com/song.flac'], /* autoplay */ true);

player.on('status',   (s) => console.log(s.state, s.index, s.queue_len));
player.on('track',    (t) => console.log(t.metadata.title));
player.on('progress', (p) => console.log(p.elapsed_ms, '/', p.duration_ms));
```

### `RockboxPlayer` API (`web/rockbox.js`)

| Method                                              | Effect                                     |
| --------------------------------------------------- | ------------------------------------------ |
| `init()`                                            | Boot audio graph + decoder Worker (async)  |
| `setQueue(urls, autoplay?)`                         | Replace the queue                          |
| `enqueue(url)` / `clearQueue()`                     | Append / empty the queue                   |
| `play()` / `pause()` / `toggle()` / `stop()`        | Transport                                  |
| `next()` / `prev()` / `skipTo(i)` / `seek(ms)`      | Navigation                                 |
| `setShuffle(bool)` / `setRepeat(0\|1\|2)`           | off / one / all                            |
| `setVolume(0..1)`                                   | Web Audio `GainNode`                       |
| `setEqEnabled(bool)`                                | Toggle the 10-band EQ                      |
| `setEqBand(band, cutoffHz, q, gainDb)`              | One EQ band (0–9)                          |
| `setEqPrecut(db)`                                   | EQ pre-cut headroom                        |
| `setTone(bassDb, trebleDb)`                         | Tone controls                              |
| `setReplaygain(mode, noclip, preampDb)`             | 0 track, 1 album, 2 shuffle, 3 off         |
| `setChannelMode(mode)` / `setStereoWidth(pct)`      | Channel mixing                             |
| `setSurround(delayMs, balance, fx1, fx2)`           | Haas surround                              |
| `setCompressor(thr, makeup, ratio, knee, rel, atk)` | Dynamic-range compressor                   |

Events: `status`, `track`, `progress`, `queue`, `error`.

## C-ABI exports (raw WASM surface)

The exports are the `rockbox-ffi` decode + DSP + metadata functions (declared
in `include/rockbox_ffi.h`). Every symbol JS calls is listed in
`EXPORTED_FUNCTIONS` in `scripts/build-wasm.sh` — a missing entry is silently
dead-stripped and `Module._rb_foo` becomes `undefined` at runtime.

- **Decode (file)**: `rb_decoder_open`, `rb_decoder_next_chunk`,
  `rb_decoder_try_next_chunk` (non-blocking — used by the pump so the module's
  main thread never parks), `rb_decoder_seek_ms`, `rb_decoder_metadata_json`,
  `rb_decoder_finished`, `rb_decoder_free`
- **Decode (live stream)**: `rb_stream_new`, `rb_stream_feed`,
  `rb_stream_close`, `rb_stream_available`, `rb_stream_free`,
  `rb_decoder_open_stream`
- **DSP**: `rb_dsp_new`, `rb_dsp_set_input_frequency`, `rb_dsp_process`,
  `rb_dsp_eq_enable`, `rb_dsp_set_eq_band`, `rb_dsp_set_eq_precut`,
  `rb_dsp_set_tone`, `rb_dsp_set_surround`, `rb_dsp_set_channel_config`,
  `rb_dsp_set_stereo_width`, `rb_dsp_set_compressor`, `rb_dsp_set_replaygain`,
  `rb_dsp_set_replaygain_gains`, `rb_dsp_flush`, `rb_dsp_free`
- **Metadata**: `rb_meta_read_json`, `rb_meta_probe`
- **Memory**: `rb_string_free`, `rb_buffer_free`, `malloc`, `free`

### Adding a new export

1. Add the `rb_*` function to `rockbox-ffi` (`crates/rockbox-ffi/src/`).
2. Add `"_rb_<name>"` to `EXPORTED_FUNCTIONS` in `scripts/build-wasm.sh`.
3. Call it from the decoder Worker (or expose a `RockboxPlayer` method).
4. Rebuild — the emcc link step must re-run to pick up the new export.

## Media support

All codecs in `rockbox-codecs` (its default feature set): FLAC, MP3, Vorbis,
Opus, ALAC, WavPack, AAC, WMA/WMA Pro, APE, TTA, Musepack, Speex, AC3, WAV /
AIFF / AU family, and more. Sources are `http(s)://` URLs (subject to CORS) or
files served from `web/`.

**Finite vs. live.** The Worker branches on the response's **`Content-Length`**
(and `icy-*`) headers:

- *Finite file* (has `Content-Length`) — buffered whole into MEMFS, then
  `rb_decoder_open`. Full metadata, duration and **seeking**.
- *Live / infinite stream* (no `Content-Length`, e.g. Icecast/SHOUTcast radio)
  — the response body is streamed and pushed chunk-by-chunk into a blocking
  reader via `rb_stream_feed`; `rb_decoder_open_stream` decodes it forever. An
  empty-but-open buffer *parks* the codec (a network stall plays out to silence
  and resumes — the stream is never dropped), and the input buffer is bounded
  (backpressure above ~8 MB), so it never grows unbounded. No seeking; the UI
  shows a **LIVE** badge and unknown duration.

## Known limitations

- **Crossfade / crossfeed / PBE / dither / pitch** are not exposed: the WASM
  DSP surface is what `rockbox-ffi`'s `dsp.rs` exposes (EQ, tone, ReplayGain,
  channel/width, surround, compressor). They can be added to `dsp.rs` later.
- **No gapless**: a track fully decodes and the ring drains before the next
  begins — a small gap between tracks, not a crossfade.
- **Elapsed time** is derived from frames actually output (accurate to what
  you hear), reset per track.
- Requires a cross-origin-isolated context (`SharedArrayBuffer`).
