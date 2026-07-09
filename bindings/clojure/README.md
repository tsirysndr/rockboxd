# rockbox-ffi — Clojure

Clojure bindings for the Rockbox **DSP**, **metadata**, and **playback**
engine, over the shared [`rockbox-ffi`](../../crates/rockbox-ffi) C ABI.

No JNI, no native glue to compile: the binding calls the Java **Foreign Function
& Memory API** (JEP 454, stable since JDK 22) through interop to locate
`librockbox_ffi` at runtime and bind every function to a `MethodHandle`
downcall. Keep [`src/rockbox/ffi.clj`](src/rockbox/ffi.clj) in sync with
[`include/rockbox_ffi.h`](../../include/rockbox_ffi.h).

## Toolchain

Requires a JDK with a stable FFM API (**JDK 22+**) and the Clojure CLI. Both are
pinned in [`mise.toml`](mise.toml) (Temurin 25 + Clojure 1.12):

```sh
mise install          # provisions Temurin 25 + the Clojure CLI
```

Build the shared library once from the repo root (the loader also honours
`ROCKBOX_FFI_LIB`, else walks up to `target/release`):

```sh
cargo build --release -p rockbox-ffi
```

## Run

```sh
mise exec -- clojure -M:smoke                   # end-to-end smoke test
mise exec -- clojure -M:play /path/to/audio     # play through the output device
```

## Usage

```clojure
(require '[rockbox.metadata :as metadata]
         '[rockbox.dsp :as dsp]
         '[rockbox.player :as player])

;; metadata -> map with keyword keys
(metadata/read "/music/song.flac")   ; => {:title "…" :codec "FLAC" …}
(metadata/probe "song.flac")         ; => "FLAC"

;; DSP (interleaved stereo int16 short-array)
(dsp/with-dsp [d 44100]
  (dsp/eq-enable d true)
  (dsp/set-eq-band d 0 100 0.7 3.0)
  (dsp/process d samples))           ; => short-array

;; Player (queue + transport)
(player/with-player [p {:volume 0.8}]
  (player/set-queue p ["/music/a.flac" "/music/b.mp3"])
  (player/play p)
  (:state (player/status p)))
```

- Rich values (metadata, player status) come back as maps with keyword keys,
  parsed from the ABI's JSON with `clojure.data.json`.
- Native memory is freed automatically: `with-dsp` / `with-player` free their
  handle, and every `char*` / `int16*` the ABI returns is freed inside the
  binding.
- Enum arguments accept either a keyword or the raw int (see
  [`src/rockbox/enums.clj`](src/rockbox/enums.clj)). **Two ReplayGain
  encodings**: `dsp-replaygain-mode` (`:track` 0, `:album` 1, `:shuffle` 2,
  `:off` 3) for `rockbox.dsp`, `replaygain-mode` (`:off` 0, `:track` 1,
  `:album` 2) for `rockbox.player`.
