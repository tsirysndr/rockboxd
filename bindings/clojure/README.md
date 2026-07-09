# rockbox-clj-ffi — Clojure

[![Clojars Project](https://img.shields.io/clojars/v/io.github.tsirysndr/rockbox-clj-ffi.svg)](https://clojars.org/io.github.tsirysndr/rockbox-clj-ffi)

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
(require '[rockbox.ffi.metadata :as metadata]
         '[rockbox.ffi.dsp :as dsp]
         '[rockbox.ffi.player :as player])

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

Coordinates: `io.github.tsirysndr/rockbox-clj-ffi`. The build mirrors the sibling
`org.clojars.tsiry/rockbox-clj` SDK — a plain `<scm>` url plus a version-derived
`<tag>` (`clojure-ffi-v<version>`), which builds cleanly on cljdoc.org. One-time
setup: register the group on [clojars.org](https://clojars.org) under **Verified
Groups → GitHub** (or use `org.clojars.tsirysndr`, auto-granted), and create a
**deploy token**.

```sh
# 1. stage the prebuilt libs for every platform into the jar resources
bindings/scripts/fetch-libs.sh --all

# 2. one-shot release: stamp cljdoc config, commit + tag clojure-ffi-v0.1.2,
#    deploy to Clojars, push the tag, and request a cljdoc build
VERSION=0.1.2 CLOJARS_USERNAME=tsirysndr CLOJARS_PASSWORD=<deploy-token> \
  mise exec -- clojure -T:build release
```

`release` requires a clean working tree. Lower-level tasks are available too:
`clojure -T:build jar` (build only), `deploy` (Clojars only — assumes you have
already stamped + tagged), `stamp-cljdoc`, and `request-cljdoc`. The
`<scm><tag>` must be a **real git tag** so cljdoc can check out the exact
sources — `release` creates and pushes it. Consume it with:

```clojure
io.github.tsirysndr/rockbox-clj-ffi {:mvn/version "0.1.2"}
```

### Coexisting with the `rockbox-clj` SDK

This repo also publishes a separate gRPC SDK, `org.clojars.tsiry/rockbox-clj`
(tagged `clojure-v*`). Nothing here collides with it:

- **Clojars coordinate** — different group *and* artifact
  (`io.github.tsirysndr/rockbox-clj-ffi` vs `org.clojars.tsiry/rockbox-clj`), so the
  two are independent projects on Clojars.
- **Clojure namespaces** — this binding lives under `rockbox.ffi.*`; the SDK
  owns the bare `rockbox.*` (`rockbox.core`, `rockbox.playback`, …). A project
  can depend on both with no namespace clash.
- **cljdoc README** — cljdoc renders a single repo-root `doc/cljdoc.edn` per
  checked-out tag (it has no per-subdirectory config). Since each artifact
  builds from its **own** release tag, `release` stamps that repo-root file to
  point at this binding's README before tagging `clojure-ffi-v*`; the SDK's
  `release` re-stamps `sdk/clojure/README.md` for `clojure-v*`. Each cljdoc
  build reads the config at its own tag, so neither shows the other's README.
- **Release tags** — the SDK uses `clojure-v*`; this binding uses
  `clojure-ffi-v*` (matches neither the SDK's nor the shared `bindings-v*`
  tag-triggered workflows, so it triggers none of them). Its prebuilt native
  libs still ship from the `bindings-v*` GitHub release via `fetch-libs.sh`.
