# Language bindings

![Python](https://img.shields.io/badge/Python-cffi-3776AB?logo=python&logoColor=white)
![Go](https://img.shields.io/badge/Go-purego-00ADD8?logo=go&logoColor=white)
![TypeScript](https://img.shields.io/badge/TypeScript-Bun%20%7C%20Deno%20%7C%20Node-3178C6?logo=typescript&logoColor=white)
![Elixir](https://img.shields.io/badge/Elixir-erl__nif-4B275F?logo=elixir&logoColor=white)
![Gleam](https://img.shields.io/badge/Gleam-erl__nif-FFAFF3?logo=gleam&logoColor=white)
![Ruby](https://img.shields.io/badge/Ruby-fiddle-CC342D?logo=ruby&logoColor=white)
![Swift](https://img.shields.io/badge/Swift-dlopen-F05138?logo=swift&logoColor=white)
![Kotlin](https://img.shields.io/badge/Kotlin-Java%20FFM-7F52FF?logo=kotlin&logoColor=white)
![Clojure](https://img.shields.io/badge/Clojure-Java%20FFM-5881D8?logo=clojure&logoColor=white)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

Bindings for the Rockbox **DSP**, **metadata**, and **playback** engine in
several languages. All of them sit on top of one shared flat **C ABI**,
[`rockbox-ffi`](../crates/rockbox-ffi) (`crates/rockbox-ffi`), whose public
surface is declared in [`include/rockbox_ffi.h`](../include/rockbox_ffi.h).

```
                 crates/rockbox-ffi  (cdylib + staticlib)
             rb_dsp_* / rb_meta_* / rb_player_*   ← include/rockbox_ffi.h
                                   │
   ┌──────┬──────┬──────────┬────────┬───────┬───────┬───────┬────────┬────────┐
 Python  Go  TypeScript  Elixir   Gleam  Ruby   Swift  Kotlin  Clojure
 (cffi)(purego)(Bun/Deno/ (erl_nif)─(erl_nif)(fiddle)(dlopen)(Java FFM)(Java FFM)
                Node)        └─ shared rockbox_ffi_nif.{c,erl} ─┘
```

Build the shared library once from the repo root:

```sh
cargo build --release -p rockbox-ffi
#  target/release/librockbox_ffi.dylib   (macOS, dlopen)
#  target/release/librockbox_ffi.so      (Linux)
#  target/release/librockbox_ffi.a       (static, for NIFs / embedding)
```

| Language       | Directory                      | Mechanism                     | Verify                                            |
| -------------- | ------------------------------ | ----------------------------- | ------------------------------------------------- |
| Python         | [`python/`](python)            | `cffi` (dlopen)               | `uv run python examples/smoke.py`                 |
| Go             | [`go/`](go)                    | `purego` (dlopen, no cgo)     | `go run ./examples/smoke`                         |
| TypeScript     | [`typescript/`](typescript)    | Bun / Deno / Node.js FFI      | `bun run examples/smoke.bun.ts`                   |
| Elixir         | [`elixir/`](elixir)            | `erl_nif` (shared)            | `mix test`                                        |
| Gleam          | [`gleam/`](gleam)              | `erl_nif` (shared)            | `make && gleam test`                              |
| Ruby           | [`ruby/`](ruby)                | `fiddle` (dlopen)             | `ruby -Ilib examples/smoke.rb`                    |
| Swift          | [`swift/`](swift)              | `dlopen` (pure Swift)         | `swift run rockbox-ffi-smoke`                     |
| Kotlin         | [`kotlin/`](kotlin)            | Java FFM (Panama, JDK 22+)    | `mise exec -- gradle smoke`                       |
| Clojure        | [`clojure/`](clojure)          | Java FFM (Panama, JDK 22+)    | `mise exec -- clojure -M:smoke`                   |

Each binding exposes the same three surfaces with matching method names:

- **metadata** — `read(path)` (returns parsed tags / duration / ReplayGain /
  album-art & cuesheet locations) and `probe(filename)`.
- **Dsp** — `new(sample_rate)`, EQ / tone / surround / compressor /
  ReplayGain setters, and `process(samples)` over interleaved stereo int16.
- **Player** — queue + transport (`play` / `pause` / `next` / `seek` / …),
  crossfade, ReplayGain, and `status()`.

## Design notes

- **Rich values cross the boundary as JSON** (metadata, player status) — the
  lowest common denominator across every language. Simple values are plain
  scalars; sample buffers are raw int16.
- **Ownership**: every allocation the ABI hands out (`char*` JSON, `int16*`
  buffers) has a matching `rb_string_free` / `rb_buffer_free`, called inside
  each binding so users never manage memory. Handles have `*_new` / `*_free`
  (or GC destructors on the BEAM).
- **Two ReplayGain mode encodings**: the DSP uses `TRACK=0, ALBUM=1,
  SHUFFLE=2, OFF=3`; the player uses `OFF=0, TRACK=1, ALBUM=2`. Each binding
  documents both.
- The **Elixir and Gleam** bindings share the exact same
  `rockbox_ffi_nif.{c,erl}` NIF (vendored into each project).

## Smoke-test parity

Every binding's smoke test performs the same end-to-end check: read the tags
of a fixture, then run a 1 kHz sine through the DSP with a −6.02 dB track
gain and assert the output peak is ≈ 8000 (half of the 16000 input) — proving
the whole native pipeline works through that language's FFI.

## Prebuilt binaries & releasing

Every binding loads the shared library at runtime — Python / TypeScript / Ruby
/ Swift via `dlopen`, Kotlin / Clojure via the JVM's Foreign Function & Memory
API (`SymbolLookup.libraryLookup`). For **published** packages the matching
prebuilt binary is bundled so users install without a Rust toolchain; from a
repo checkout the loaders fall back to `target/release` (or `ROCKBOX_FFI_LIB`).

The two **JVM** bindings need a JDK with a stable FFM API (**JDK 22+**); both
ship a [`mise.toml`](https://mise.jdx.dev) pinning Temurin 25 (and, for
Clojure, the Clojure CLI), so `mise install` provisions the exact toolchain.
FFM's restricted native calls are granted with `--enable-native-access=ALL-UNNAMED`
(wired into the Gradle tasks / `deps.edn` aliases).

The [`bindings-release`](../.github/workflows/bindings-release.yml) workflow
(**manually triggerable** with a tag input, or on a `bindings-v*` tag) builds
`librockbox_ffi` for six targets, packages each ecosystem, and **attaches
every artifact to a GitHub Release** — gems, wheels + sdist, npm tarballs, and
the raw per-target shared libs. Uploads use the built-in `GITHUB_TOKEN`, so no
registry secrets are needed; download the assets and `gem push` / `twine
upload` / `npm publish` them manually when ready.

| Target        | Runner           | Python                     | Ruby (gem platform) | npm package                |
| ------------- | ---------------- | -------------------------- | ------------------- | -------------------------- |
| darwin-arm64  | macos-latest     | wheel `macosx_11_0_arm64`  | `arm64-darwin`      | `@rockbox-ffi/darwin-arm64`|
| darwin-x64    | macos-15-intel   | wheel `macosx_10_12_x86_64`| `x86_64-darwin`     | `@rockbox-ffi/darwin-x64`  |
| linux-x64     | ubuntu-latest    | manylinux (auditwheel)     | `x86_64-linux`      | `@rockbox-ffi/linux-x64`   |
| linux-arm64   | ubuntu-24.04-arm | manylinux (auditwheel)     | `aarch64-linux`     | `@rockbox-ffi/linux-arm64` |
| freebsd-x64   | vmactions VM     | wheel (best-effort)        | `x86_64-freebsd`    | `@rockbox-ffi/freebsd-x64` |
| netbsd-x64    | vmactions VM     | wheel (best-effort)        | `x86_64-netbsd`     | `@rockbox-ffi/netbsd-x64`  |

- **Python** bundles the lib in `rockbox_ffi/_lib/`; **Ruby** in the gem's
  `vendor/`; **npm** ships one `@rockbox-ffi/<platform>` package per target
  (declared as `optionalDependencies`, so npm installs only the matching one).
- **BSD** builds run inside `vmactions` VMs and are `continue-on-error`; every
  packaging step skips a target whose artifact is missing, so a flaky BSD build
  never blocks the macOS/Linux release. BSD Python wheels are best-effort
  (pip falls back to the sdist when the tag does not match).
- **Linux** needs the system **`libasound2`** (ALSA) package at runtime — the
  shared lib links it and it is not vendored (`auditwheel --exclude`), because
  a bundled libasound can't reliably load the system's ALSA plugins.

### Publishing

The CI already builds and tests every package (gems, wheels + sdist, npm
tarballs — each with the lib bundled) and attaches them to the release. To push
those exact artifacts to the registries, authenticate and run the per-ecosystem
scripts (each downloads from the release, then publishes):

```sh
# authenticate first: gem signin · twine (~/.pypirc) · npm login
bindings/scripts/publish-ruby.sh   --tag bindings-v0.1.0   # -> RubyGems
bindings/scripts/publish-python.sh --tag bindings-v0.1.1   # -> PyPI
bindings/scripts/publish-npm.sh    --tag bindings-v0.1.1   # -> npmjs
```

The two **JVM** bindings publish to different registries, under the
`io.github.tsirysndr` namespace (no `rockbox.org` domain needed — verified via
GitHub). Unlike the per-platform npm packages, each JVM artifact **bundles every
target in one jar** (`native/<target>/librockbox_ffi.<ext>`), so run
`fetch-libs.sh --all` first:

```sh
bindings/scripts/fetch-libs.sh --all                       # stage all targets

# Kotlin -> Maven Central (io.github.tsirysndr:rockbox-ffi)
(cd bindings/kotlin && mise exec -- gradle publishToMavenCentral)

# Clojure -> Clojars (io.github.tsirysndr/rockbox-clj-ffi)
(cd bindings/clojure && CLOJARS_USERNAME=… CLOJARS_PASSWORD=… \
   mise exec -- clojure -T:build deploy)
```

See [`kotlin/README.md`](kotlin/README.md) and
[`clojure/README.md`](clojure/README.md) for the one-time namespace/GPG setup.

Each accepts `--tag <tag>` (default: latest `bindings-v*`), `--repo
<owner/repo>` (default: the `origin` remote), and `--dry-run` (download + print
the push commands without running them). npm publishes the `@rockbox-ffi/*`
platform packages before the main package; PyPI uploads the macOS + manylinux
wheels and sdist, then the BSD wheels best-effort (Warehouse may reject their
platform tag — those users fall back to the sdist).

### Building locally instead

To build the packages yourself rather than publish CI's artifacts,
[`scripts/fetch-libs.sh`](scripts/fetch-libs.sh) stages the prebuilt libs from a
release into each binding's bundled location, ready to `gem build` /
`python -m build` / `npm pack`:

```sh
bindings/scripts/fetch-libs.sh                 # host lib -> all three bindings
bindings/scripts/fetch-libs.sh --all           # every target -> typescript/npm/*
bindings/scripts/fetch-libs.sh --target linux-x64 --tag bindings-v0.1.1
```
