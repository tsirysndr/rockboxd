# rockbox (Go)

![Go](https://img.shields.io/badge/Go-1.22%2B-00ADD8?logo=go&logoColor=white)
![FFI](https://img.shields.io/badge/FFI-purego-00ADD8)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

Go bindings for the Rockbox **DSP**, **metadata**, and **playback** engine, via
[`purego`](https://github.com/ebitengine/purego) over the prebuilt
`librockbox_ffi` shared library. **No cgo and no C toolchain** — the package
`dlopen`s the shared library at process start.

## Setup

Build the shared library once (from the repo root):

```sh
cargo build --release -p rockbox-ffi
```

Then add the module and run the smoke test:

```sh
go get github.com/tsirysndr/rockboxd/bindings/go

# from a checkout:
cd bindings/go
go run ./examples/smoke   # end-to-end check
go test ./...
```

The library is located automatically. Precedence:

1. `ROCKBOX_FFI_LIB` environment variable (explicit path override).
2. a `lib/librockbox_ffi.{dylib,so,dll}` bundled next to the package source.
3. `target/release/librockbox_ffi.*`, by walking up from the package source,
   the executable, and the current directory (repo checkout / dev).

Because Go modules are consumed from source (`go get`), the usual story for a
standalone binary is to point `ROCKBOX_FFI_LIB` at the shared library, or drop
it next to the executable's `target/release`. CI attaches the per-target
`librockbox_ffi.*` to each `bindings-v*` release.

## Usage

```go
package main

import (
	"fmt"

	rockbox "github.com/tsirysndr/rockboxd/bindings/go"
)

func main() {
	// --- metadata -----------------------------------------------------
	meta, _ := rockbox.Metadata.Read("song.flac")
	fmt.Printf("%s — %s (%d ms)\n", meta.Artist, meta.Title, meta.DurationMs)
	label, _ := rockbox.Metadata.Probe("track.opus") // => "Opus"
	_ = label

	// --- DSP (interleaved stereo int16) -------------------------------
	dsp, _ := rockbox.NewDsp(44100)
	defer dsp.Close()
	dsp.EqEnable(true)
	dsp.SetEqBand(0, 60, 0.7, 3.0)
	dsp.SetReplaygain(rockbox.DspReplayGainTrack, true, 0.0)
	dsp.SetReplaygainGains(rockbox.Opt(-6.02), nil, nil, nil) // halves amplitude
	processed, _ := dsp.Process(samples)                      // []int16
	_ = processed

	// --- playback (needs an output device) ----------------------------
	cfg := rockbox.DefaultConfig()
	cfg.Volume = 0.8
	player, _ := rockbox.NewPlayer(cfg)
	defer player.Close()
	player.SetReplaygain(rockbox.ReplayGainTrack, 0.0, true)
	player.SetCrossfade(rockbox.CrossfadeAlways, 0, 2000, 0, 2000, rockbox.MixCrossfade)
	player.SetQueue([]string{"a.flac", "b.mp3", "c.opus"})
	player.Play()
	st, _ := player.Status() // => &Status{State: "playing", ...}
	_ = st
}
```

`Dsp` and `Player` own native resources — call `Close` (a `defer` is idiomatic)
when done.

## API

| Symbol                          | Contents                                                           |
| ------------------------------- | ------------------------------------------------------------------ |
| `rockbox.Metadata.Read/Probe`   | `Read(path) (*Meta, error)`, `Probe(name) (string, bool)`          |
| `rockbox.NewDsp` → `*Dsp`       | EQ / tone / surround / compressor / ReplayGain, `Process([]int16)` |
| `rockbox.NewPlayer` → `*Player` | queue + transport + crossfade + ReplayGain, `Status()`             |
| `rockbox.*` consts              | `DspReplayGain*`, `ReplayGain*`, `Crossfade*`, `Mix*`, `Channel*`  |

Rich values (metadata, player status) cross the FFI boundary as JSON and are
decoded into typed structs (`Meta`, `Status`). Sample buffers are plain
`[]int16` of interleaved-stereo signed 16-bit samples. Optional metadata fields
(track number, per-track ReplayGain, album art, cuesheet) are pointer fields
that are `nil` when the tag is absent.

### Two ReplayGain encodings

The DSP and player use *different* mode integers (a quirk of the C ABI):

- `Dsp.SetReplaygain` → `DspReplayGainMode` (`Track=0, Album=1, Shuffle=2, Off=3`)
- `Player.SetReplaygain` / `Config.ReplayGainMode` → `ReplayGainMode`
  (`Off=0, Track=1, Album=2`)

Use the named constants and you won't have to remember which is which.

## Examples

```sh
go run ./examples/smoke   # metadata + DSP + player checks
```

## Test

```sh
go test ./...
```
