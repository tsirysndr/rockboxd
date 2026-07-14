# Rockbox WASM — Browser Build

A **lightweight, single-threaded** WebAssembly build of Rockbox for the browser.
It compiles only the extracted core crates — the **decoders**, the **DSP**, and
the **metadata parser** — and lets JavaScript be the player. It ships as the
[`rockbox-wasm`](bindings/wasm) npm package.

```
rockbox-codecs    FLAC, MP3, Vorbis, Opus, ALAC, WavPack, AAC, WMA, APE, …
rockbox-dsp       parametric EQ, tone, ReplayGain, resampler, compressor, …
rockbox-metadata  tags + ReplayGain for 40+ formats
        │  (flat C ABI: rockbox-ffi, `player` feature off)
        ▼
   rockbox-core.wasm   ← decode + DSP + metadata only
```

No firmware, no netstream, no playlist engine, no servers, no SDL. Queue,
transport, scheduling and audio output all live in ~three small JS files.

## Layout

```
bindings/wasm/
  src/          rockbox.js (facade) · rockbox-decoder-worker.js · rockbox-audio-worklet.js
  dist/         built package (gitignored): rockbox-core.js [+ the 3 src files]
  example/      Vite + React + TypeScript demo (depends on the package via file:..)
  index.d.ts    TypeScript declarations
  scripts/      build.sh · publish.sh
scripts/build-wasm.sh   low-level core builder (emits rockbox-core.{js,wasm})
```

## Architecture — single-threaded, no SharedArrayBuffer

```
 main thread      src/rockbox.js         facade · AudioContext · GainNode · events
      │  cmd ▼                                  ▲ event │
 decoder Worker   src/rockbox-decoder-worker.js  owns rockbox-core.wasm
      │  PCM ▼  (MessagePort — postMessage, transferable ArrayBuffers)
 AudioWorklet     src/rockbox-audio-worklet.js   queues + plays PCM → speakers
```

Decoding is fully **synchronous** (`rb_decode_packet` / `rb_decode_file`) — no
wasm threads — so the module needs no `SharedArrayBuffer`, and the page needs
**no COOP/COEP headers**. The worker decodes off the main thread and streams
S16 PCM to the worklet over a `MessagePort`; the worklet queues it and reports
back how much it has consumed/queued so the worker can pace decoding and show
elapsed time.

- **Finite file** (has `Content-Length`) — buffered whole into MEMFS; tags via
  `rb_meta_read_json`, decoded via `rb_decode_file`. Full metadata, duration,
  **seeking**.
- **Live stream** (no `Content-Length`, e.g. Icecast/SHOUTcast) — read the
  network in ~32 KB segments, decode each with `rb_decode_packet`, stream the
  PCM. ICY `StreamTitle` is demuxed for now-playing metadata. Not seekable.

## Build

```sh
source /path/to/emsdk/emsdk_env.sh          # Emscripten SDK
rustup target add wasm32-unknown-emscripten

bash bindings/wasm/scripts/build.sh          # → bindings/wasm/dist/ (wasm embedded)
# or the low-level core only:
bash scripts/build-wasm.sh                    # → bindings/wasm/dist/rockbox-core.{js,wasm}
```

Two steps: `cargo rustc -p rockbox-ffi --no-default-features --crate-type
staticlib` for `wasm32-unknown-emscripten` (the codec/DSP/metadata C sources
compile to wasm via the `cc` crate), then an `emcc` link. The package build adds
`SINGLE_FILE=1` so the `.wasm` is embedded as base64 in `rockbox-core.js` — no
separate binary to serve.

Console (babashka): `bb wasm:build`, `bb wasm:example`, `bb wasm:dev`,
`bb wasm:publish`.

## Using it

```js
import { RockboxPlayer } from "rockbox-wasm";
const player = new RockboxPlayer({ baseUrl: "/rockbox" });
await player.init();                         // from a user gesture
player.setQueue(["/song.flac"], true);
```

See `bindings/wasm/README.md` for the full API and `bindings/wasm/example/` for
the React app.

## C-ABI exports

The exports are the `rockbox-ffi` decode + DSP + metadata functions (declared in
`include/rockbox_ffi.h`). Every symbol JS calls is listed in `EXPORTED_FUNCTIONS`
in `scripts/build-wasm.sh` — a missing entry is silently dead-stripped.

- **Decode (sync)**: `rb_decode_file`, `rb_decode_packet`
- **Decode (incremental)**: `rb_decoder_open`, `rb_decoder_next_chunk`,
  `rb_decoder_seek_ms`, `rb_decoder_metadata_json`, `rb_decoder_free`
  — spawns a codec thread, so unused by the single-threaded browser build
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
3. Call it from the worker / add a `RockboxPlayer` method.
4. Rebuild — the emcc link step must re-run to pick up the new export.

## Known limitations

- **Dither / pitch** aren't exposed: the WASM DSP surface is what
  `rockbox-ffi`'s `dsp.rs` exposes. (Crossfeed and PBE are; crossfade is the
  Rockbox pcmbuf algorithm ported to JS in the decoder worker.)
- **Live streams** aren't seekable and report `duration_ms: 0`.
- Large finite files are decoded whole before playback (the incremental,
  seek-while-streaming decoder needs a codec thread, which the single-threaded
  build omits).
