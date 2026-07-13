# rockbox-ffi — Swift

Swift bindings for the Rockbox **DSP**, **metadata**, **codecs**, and
**playback** engine, over the shared [`rockbox-ffi`](../../crates/rockbox-ffi)
C ABI.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

The public API (`Dsp`, `Player`, `Metadata`, enums) and the `dlsym` loader live
in the `RockboxFFICore` module and are shared by **two products** — pick one:

| Product              | Native code        | Runtime lookup | Use when                                       |
| -------------------- | ------------------ | -------------- | ---------------------------------------------- |
| `RockboxFFI`         | statically linked  | none           | you want a single self-contained binary        |
| `RockboxFFIDynamic`  | `dlopen`ed         | file lookup    | you'd rather ship the library beside your app  |

Both drive the same `@convention(c)` closures; the loader decides at runtime
whether to resolve the `rb_*` symbols from the process image (static) or from a
`dlopen`ed library (dynamic). Keep
[`Loader.swift`](Sources/RockboxFFICore/Loader.swift) — and the `ffiSymbols`
list in [`Package.swift`](Package.swift) — in sync with
[`include/rockbox_ffi.h`](../../include/rockbox_ffi.h).

## Build & run

Build the Rust artifacts once from the repo root — this produces **both**
`librockbox_ffi.a` (bundled by `RockboxFFI`) and `librockbox_ffi.dylib`
(loaded by `RockboxFFIDynamic`):

```sh
cargo build --release -p rockbox-ffi
```

```sh
swift run rockbox-ffi-smoke                 # static product, end-to-end smoke test
swift run rockbox-ffi-play /path/to/audio   # dynamic product, plays via output device
```

`rockbox-ffi-smoke` links `RockboxFFI`, so it needs no library at runtime — the
engine is baked into the binary. `rockbox-ffi-play` links `RockboxFFIDynamic`,
which honours `ROCKBOX_FFI_LIB`, else walks up to `target/release`.

### `RockboxFFI` (static)

`RockboxFFI` pulls the `rb_*` entry points out of `librockbox_ffi.a` (with a
`-u <symbol>` per function, since the loader reaches them via `dlsym` and the
linker would otherwise strip them) and links the system libraries the engine
needs. Consumers get a binary with **no** dependency on `librockbox_ffi.dylib`
and no `ROCKBOX_FFI_LIB` / path lookup at runtime. Required system libraries:

| Platform | Linked automatically by the package                            |
| -------- | -------------------------------------------------------------- |
| macOS    | `AudioUnit`, `CoreAudio`, `CoreFoundation`, `iconv`            |
| Linux    | `asound` (ALSA, backs cpal)                                    |

The archive is resolved absolutely from the manifest location: first
`../../target/release/librockbox_ffi.a` (in-repo cargo output), else a vendored
`Libs/librockbox_ffi.a` next to `Package.swift` (distributable tarball).

## Distribution

The `bindings-release` GitHub workflow publishes reproducible, prebuilt Swift
artifacts to the release (sorted entries, fixed timestamps, `gzip -n` — byte
stable across runs):

| Asset                              | Contents                                                                     |
| ---------------------------------- | ---------------------------------------------------------------------------- |
| `RockboxFFI.xcframework.zip`       | Static lib + header for macOS (universal), iOS (arm64), iOS-sim (arm64+x64)   |
| `rockbox-ffi-swift-macos.tar.gz`   | This SwiftPM package with the universal macOS archive vendored under `Libs/`  |
| `librockbox_ffi-<target>.a`        | Raw per-target static archives (macOS, Linux, BSD)                            |

**Apple apps (macOS + iOS):** drop `RockboxFFI.xcframework` into Xcode, or
reference it as a `binaryTarget` in a `Package.swift`. Because it's a static
xcframework, an iOS/macOS app that calls the C ABI directly retains the symbols;
if you consume it through this package's `dlsym` loader instead, keep the
`-u`-per-symbol linker settings so `-dead_strip` doesn't drop them. iOS apps
must additionally link `AudioToolbox` / `AVFoundation` and configure an
`AVAudioSession` before using `Player`.

**macOS off the monorepo:** unpack `rockbox-ffi-swift-macos.tar.gz` and
`swift build` — `Package.swift` picks up the vendored `Libs/librockbox_ffi.a`.

## Usage

```swift
import RockboxFFI

// metadata
let meta = try Metadata.read("/music/song.flac")   // [String: Any]
Metadata.probe("song.flac")                         // "FLAC"

// DSP (interleaved stereo int16)
let dsp = try Dsp(sampleRate: 44_100)
defer { dsp.close() }
dsp.eqEnable(true)
dsp.setEqBand(0, cutoffHz: 100, q: 0.7, gainDb: 3.0)
let out = try dsp.process(samples)

// Codecs (decode a file to PCM, one chunk at a time)
let dec = try Decoder(path: "/music/song.flac")
defer { dec.close() }
let tags = try dec.metadata()                        // [String: Any]
while let (pcm, sampleRate) = dec.nextChunk() {      // interleaved-stereo int16
    _ = (pcm, sampleRate)
}
let (done, code) = dec.finished()                    // (true, 0)  (0 = clean end)

// Player (queue + transport)
var cfg = Player.Config(); cfg.volume = 0.8
let player = try Player(config: cfg)
// Queue entries may be local files, http(s):// URLs to remote media,
// or live-radio / streaming URLs — mix and match freely.
try player.setQueue([
    "/music/a.flac",
    "https://example.com/b.mp3",
    "http://radio.example/stream",
])
player.play()

// Queue editing
player.enqueue("/music/c.flac")   // append one track
player.remove(0)                  // drop a track by 0-based index
player.clearQueue()               // empty the queue + stop playback
```

- Rich values (metadata, player status) come back as `[String: Any]`, parsed
  from the ABI's JSON with `JSONSerialization`.
- Native memory is freed inside the binding (`close()` / `deinit` for handles;
  every `char*` / `int16*` return is freed after copying).
- The `Decoder` codec engine is process-wide — only one may decode at a time;
  constructing a second one blocks until the first is closed.
- **Two ReplayGain encodings** — `DspReplayGainMode` (track 0, album 1,
  shuffle 2, off 3) for `Dsp`, `ReplayGainMode` (off 0, track 1, album 2) for
  `Player`.
