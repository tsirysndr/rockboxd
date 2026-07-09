# rockbox-ffi — Swift

Swift bindings for the Rockbox **DSP**, **metadata**, and **playback** engine,
over the shared [`rockbox-ffi`](../../crates/rockbox-ffi) C ABI.

Pure Swift, no C target or module map: the binding `dlopen`s `librockbox_ffi`
at runtime and `dlsym`s every function into a typed `@convention(c)` closure —
exactly like the Ruby / Python bindings. Keep
[`Loader.swift`](Sources/RockboxFFI/Loader.swift) in sync with
[`include/rockbox_ffi.h`](../../include/rockbox_ffi.h).

## Build & run

Build the shared library once from the repo root (the loader also honours
`ROCKBOX_FFI_LIB`, else walks up to `target/release`):

```sh
cargo build --release -p rockbox-ffi
```

```sh
swift run rockbox-ffi-smoke                 # end-to-end smoke test
swift run rockbox-ffi-play /path/to/audio   # play through the output device
```

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

// Player (queue + transport)
var cfg = Player.Config(); cfg.volume = 0.8
let player = try Player(config: cfg)
try player.setQueue(["/music/a.flac", "/music/b.mp3"])
player.play()
```

- Rich values (metadata, player status) come back as `[String: Any]`, parsed
  from the ABI's JSON with `JSONSerialization`.
- Native memory is freed inside the binding (`close()` / `deinit` for handles;
  every `char*` / `int16*` return is freed after copying).
- **Two ReplayGain encodings** — `DspReplayGainMode` (track 0, album 1,
  shuffle 2, off 3) for `Dsp`, `ReplayGainMode` (off 0, track 1, album 2) for
  `Player`.
