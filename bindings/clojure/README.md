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

## Bundled native libraries

The published jar bundles the prebuilt `librockbox_ffi` for every OS/arch under
`native/<target>/` — `rockbox.ffi/extract-bundled` picks the one matching the
running JVM, extracts it to a temp file, and loads it. So a consumer just adds
the dependency; no Rust toolchain, no separate `.dylib`/`.so`. `ROCKBOX_FFI_LIB`
still overrides, and a repo checkout falls back to `target/release`.

## Publishing (Clojars, `io.github.tsirysndr`)

Coordinates: `io.github.tsirysndr/rockbox-ffi`, always deployed as a
**`-SNAPSHOT`** (Clojars release versions are immutable). One-time setup:
register the group on [clojars.org](https://clojars.org) under **Verified
Groups → GitHub** (or use `org.clojars.tsirysndr`, auto-granted), and create a
**deploy token**.

```sh
# 1. stage the prebuilt libs for every platform into the jar resources
bindings/scripts/fetch-libs.sh --all

# 2. build the jar locally to sanity-check (no credentials needed)
mise exec -- clojure -T:build jar

# 3. deploy to Clojars
CLOJARS_USERNAME=tsirysndr CLOJARS_PASSWORD=<deploy-token> \
  mise exec -- clojure -T:build deploy       # ROCKBOX_VERSION=0.2.0 to bump
```

Consume it with:

```clojure
io.github.tsirysndr/rockbox-ffi {:mvn/version "0.1.0-SNAPSHOT"}
```
