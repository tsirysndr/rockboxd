# rockbox-ffi — Kotlin

Kotlin/JVM bindings for the Rockbox **DSP**, **metadata**, and **playback**
engine, over the shared [`rockbox-ffi`](../../crates/rockbox-ffi) C ABI.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

No JNI, no native glue to compile: the binding uses the Java **Foreign Function
& Memory API** (JEP 454, stable since JDK 22) to locate `librockbox_ffi` at
runtime and bind every function to a `MethodHandle` downcall. Keep
[`Native.kt`](src/main/kotlin/org/rockbox/ffi/Native.kt) in sync with
[`include/rockbox_ffi.h`](../../include/rockbox_ffi.h).

## Toolchain

Requires a JDK with a stable FFM API (**JDK 22+**). The pinned toolchain is
declared in [`mise.toml`](mise.toml) (Temurin 25):

```sh
mise install          # provisions Temurin 25
```

Build the shared library once from the repo root (the loader also honours
`ROCKBOX_FFI_LIB`, else walks up to `target/release`):

```sh
cargo build --release -p rockbox-ffi
```

## Run

```sh
mise exec -- gradle smoke                       # end-to-end smoke test
mise exec -- gradle play -Pfile=/path/to/audio  # play through the output device
```

## Usage

```kotlin
import org.rockbox.ffi.*

// metadata
val meta = Metadata.read("/music/song.flac")   // Map<String, Any?>
println(meta["title"])
Metadata.probe("song.flac")                     // "FLAC"

// DSP (interleaved stereo int16)
Dsp(44_100).use { dsp ->
    dsp.eqEnable(true)
    dsp.setEqBand(band = 0, cutoffHz = 100, q = 0.7f, gainDb = 3.0f)
    val out: ShortArray = dsp.process(samples)
}

// Player (queue + transport)
Player(Player.Config().apply { volume = 0.8f }).use { player ->
    player.setQueue(listOf("/music/a.flac", "/music/b.mp3"))
    player.play()
    println(player.status()["state"])
}
```

- Rich values (metadata, player status) come back as `Map<String, Any?>`,
  parsed from the ABI's JSON with `org.json`.
- Native memory is freed automatically: handles are `AutoCloseable` (`use { }`),
  and every `char*` / `int16*` the ABI returns is freed inside the binding.
- **Two ReplayGain encodings** — `DspReplayGainMode` (TRACK=0, ALBUM=1,
  SHUFFLE=2, OFF=3) for `Dsp`, `ReplayGainMode` (OFF=0, TRACK=1, ALBUM=2) for
  `Player`.

## Bundled native libraries

The published jar bundles the prebuilt `librockbox_ffi` for every OS/arch under
`native/<target>/` — `Native.extractBundled()` picks the one matching the
running JVM, extracts it to a temp file, and loads it. So a consumer just adds
the dependency; no Rust toolchain, no separate `.dylib`/`.so`. `ROCKBOX_FFI_LIB`
still overrides, and a repo checkout falls back to `target/release`.

## Publishing (Maven Central, `io.github.tsirysndr`)

Coordinates: `io.github.tsirysndr:rockbox-ffi`. One-time setup: verify the
`io.github.tsirysndr` namespace on [central.sonatype.com](https://central.sonatype.com)
(create a public repo named after the verification code), and have a GPG key.

```sh
# 1. stage the prebuilt libs for every platform into the jar resources
bindings/scripts/fetch-libs.sh --all

# 2. credentials in ~/.gradle/gradle.properties (or ORG_GRADLE_PROJECT_* env):
#   mavenCentralUsername=<central-portal-token-user>
#   mavenCentralPassword=<central-portal-token>
#   signingInMemoryKey=<ASCII-armored GPG private key>
#   signingInMemoryKeyPassword=<key passphrase>

# 3. build (with the bundled libs) + upload + auto-release
mise exec -- gradle publishToMavenCentral        # -PlibVersion=0.2.0 to override

# validate locally without credentials first:
mise exec -- gradle publishToMavenLocal
```
