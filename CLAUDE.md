# CLAUDE.md — Rockbox Daemon

## Markdown formatting

### Tables

Always align markdown tables so that column pipes line up in raw text. Every cell in a column must be padded with trailing spaces to the width of the widest cell in that column. The separator row must use the same number of dashes as the column width. Example:

```markdown
| Name    | Role      | Notes                        |
| ------- | --------- | ---------------------------- |
| Alice   | Engineer  | Owns the firmware layer      |
| Bob     | Designer  | Works on the mobile UI       |
| Charlie | QA        | Runs integration test suites |
```

When editing an existing table, re-align the whole table (not just the changed row). When adding a new table, align it before committing.

---

## Project overview

Rockbox Daemon is a modern wrapper around the [Rockbox](https://www.rockbox.org) open-source audio player firmware. It adds Rust/Zig services on top of the C firmware to expose gRPC, GraphQL, HTTP, and MPD APIs, a Typesense-backed search engine, Chromecast/AirPlay/Snapcast/Squeezelite output sinks, and a desktop/web UI.

The binary is called **`rockboxd`**. It is a single executable built by Zig that links:
- The Rockbox C firmware (compiled by Make into `build-lib/libfirmware.a` and friends)
- Rust crates (compiled by Cargo into `target/release/librockbox_cli.a` and `librockbox_server.a`)
- SDL2 for audio/event handling on the host platform

## Repository layout

```
firmware/          Rockbox C firmware (audio engine, codecs, DSP)
apps/              Rockbox application layer (playlist, database, plugins)
lib/               Codec libraries (rbcodec, fixedpoint, skin_parser, tlsf)
build-lib/         Out-of-tree Make build directory (generated; do not edit)
build-headless/    Headless (no SDL) Make build directory for the embedded lib
crates/            Rust workspace
  airplay/         ALAC encoder + RAOP/RTP sender (AirPlay 1 output)
  slim/            Slim Protocol + HTTP broadcast server (Squeezelite multi-room output)
  cli/             Entry point compiled to librockbox_cli.a (staticlib)
  embed/           Embeddable desktop library (daemon boot + gRPC client C ABI)
  server/          gRPC / HTTP server
  settings/        load_settings() — reads settings.toml, applies sinks
  sys/             FFI bindings to the C firmware (unsafe extern "C")
  library/         Audio file scanning and SQLite library management
  typesense/       Typesense client for fast music search
  netstream/       HTTP streaming (Range-request based fd multiplexing)
  chromecast/      Chromecast output
  rpc/             gRPC definitions / generated code
  graphql/         GraphQL schema and resolvers
  s3/              S3-compatible HTTP API (SigV4 auth, PUT/DELETE/GET/HEAD/List)
  mpd/             MPD protocol server
  mpris/           MPRIS D-Bus integration
  tracklist/       Playlist / tracklist management
  types/           Shared Rust types
  traits/          Shared Rust traits
zig/               Zig build script, main.zig (executable), lib.zig (embedded library)
include/           Public C header (rockboxd.h) for the embeddable library
gpui/              Desktop client (GPUI / Rust) — embeds daemon directly via librockboxd.a
expo/              React Native / Expo mobile app (see `expo/CLAUDE.md` rules below)
```

## Build system

### Step 1 — C firmware (Make)
```sh
cd build-lib
make lib          # builds libfirmware.a, librockbox.a, codec libs
```
The `build-lib/` directory was pre-configured via Rockbox's `tools/configure` for the `sdlapp` target. Do **not** run `configure` again unless you know what you're doing — it regenerates the Makefile and overwrites any local edits.

### Step 2 — Rust crates (Cargo)
```sh
cargo build --release -p rockbox-cli     # produces target/release/librockbox_cli.a
cargo build --release -p rockbox-server  # produces target/release/librockbox_server.a
```
Both crates have `crate-type = ["staticlib"]`. All transitive rlib dependencies are bundled into the `.a`.

### Step 3 — Zig linker
```sh
cd zig
zig build          # links everything into zig-out/bin/rockboxd
```

### Quick full rebuild
```sh
cd build-lib && make lib && cd ..
cargo build --release -p rockbox-cli -p rockbox-server
cd zig && zig build
```

### Embeddable static library (`librockboxd.a`)

`zig build lib` produces `zig/zig-out/lib/librockboxd.a` — a fat archive that
any desktop GUI (GPUI, Swift/AppKit, Qt, …) can link against to embed the full
Rockbox daemon in-process. Uses the **headless/cpal** firmware (no SDL).

Public C header: `include/rockboxd.h`

Build order:
```sh
# 1. Headless firmware (no SDL)
cd build-headless && make lib && cd ..

# 2. Rust embed crate (daemon boot + gRPC client) + server
cargo build --release -p rockbox-embed -p rockbox-server

# 3. Fat static library
cd zig && zig build lib
# → zig-out/lib/librockboxd.a
```

Consumers must also link these system libraries at final link time:

| Platform | Required flags                                                               |
| -------- | ---------------------------------------------------------------------------- |
| macOS    | `-framework CoreAudio -framework AudioUnit -framework AudioToolbox`          |
|          | `-framework CoreFoundation -framework Security -framework CoreServices`      |
| Linux    | `-lasound -lunwind -ldbus-1`                                                 |

The GPUI desktop client links `librockboxd.a` automatically via `gpui/build.rs`
and boots the daemon at startup — no external `rockboxd` process is needed.

### GPUI desktop client build
```sh
# After building librockboxd.a (see above):
cd gpui && cargo build --release
```

The app boots the embedded daemon on launch (shows "Starting Rockbox…" while
the gRPC server binds, then transitions to the full UI).

### Critical: stale binary pitfall
`zig build` only re-links if the `.a` files are newer than the binary. After changing C code, always run `make lib` first. After changing Rust code, run `cargo build --release`. If behavior doesn't match the code, check timestamps:
```sh
ls -la zig/zig-out/bin/rockboxd build-lib/libfirmware.a target/release/librockbox_cli.a
```

### Zig 0.16.0 build.zig API notes
- **Linker args (raw flags)**: **There is no `addLinkerArg` method** anywhere in Zig 0.16.0 — not on `Build.Step.Compile` and not on `Build.Module`. Passing raw flags like `--allow-multiple-definition` through `build.zig` is not possible. Solve linker conflicts at the source level instead (e.g. `objcopy --redefine-sym` in the build script).
- **Library/include paths**: `exe.root_module.addLibraryPath(...)`, `exe.root_module.addIncludePath(...)`.
- **System libraries**: `exe.root_module.linkSystemLibrary("name", .{})`.
- **Frameworks (macOS)**: `exe.root_module.linkFramework("Name", .{})`.
- **Object/archive files**: `exe.root_module.addObjectFile(b.path("..."))` — used for both `.o` and `.a`.
- **Conditional linking**: use `if (condition) { ... }` around the `addObjectFile` / `linkSystemLibrary` calls directly in `build()` — there is no per-target feature-flag mechanism.

## Runtime configuration

Settings file: `~/.config/rockbox.org/settings.toml`

```toml
music_dir = "/path/to/Music"

# Audio output — pick one:
audio_output = "builtin"      # SDL audio (default)

audio_output = "fifo"
fifo_path = "/tmp/snapfifo"   # named FIFO for Snapcast; use "-" for stdout

audio_output = "airplay"
airplay_host = "192.168.1.x"  # RAOP receiver IP
airplay_port = 5000            # optional, default 5000

audio_output = "squeezelite"
squeezelite_port = 3483        # optional, Slim Protocol port (default 3483)
squeezelite_http_port = 9999   # optional, HTTP PCM stream port (default 9999)

# S3-compatible API (upload / delete audio files in music_dir)
s3_enabled = true
s3_port = 9000                 # optional, default 9000
s3_host = "0.0.0.0"            # optional, default "0.0.0.0"
s3_access_key = "AKIA..."
s3_secret_key = "wJalrXUt..."
# region is fixed to "us-east-1"; bucket is fixed to "music"
```

Run one or more squeezelite clients pointing at rockboxd for multi-room:
```sh
squeezelite -s localhost -n "Living Room"
squeezelite -s localhost -n "Kitchen"
```

## S3-compatible API

`crates/s3/` exposes a single-bucket S3 server (constants: bucket `music`,
region `us-east-1`, port default `9000`). Authentication is AWS Signature
V4 header-form (`crates/s3/src/sigv4.rs`). Supported ops: `PutObject`,
`DeleteObject`, `GetObject`, `HeadObject`, `ListObjectsV2`, `ListBuckets`.

**DB sync is structural, not coded** — every PUT writes a file under
`music_dir`, every DELETE removes one. The library watcher
(`crates/library/src/watcher.rs`) picks up the `Create`/`Remove` event and
calls `save_audio_metadata()` / `repo::track::delete_by_path()`. Do NOT
add a parallel "tell the DB about this S3 op" code path — it would race
with the watcher and double-insert.

**Not implemented**: multipart upload, `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`
(awscli v2.23+ defaults that require `AWS_REQUEST_CHECKSUM_CALCULATION=when_required`
to fall back to single-shot signing), bucket CRUD, presigned URLs.
Single PUT cap is 2 GiB. Uploads are restricted to audio extensions
(same allowlist as the watcher's `AUDIO_EXTENSIONS`).

## PCM sink architecture

The audio output abstraction lives in `firmware/export/pcm_sink.h`. Each sink implements `struct pcm_sink_ops` (init / postinit / set_freq / lock / unlock / play / stop).

| Enum constant          | Value | Implementation file                        |
| ---------------------- | ----- | ------------------------------------------ |
| `PCM_SINK_BUILTIN`     | 0     | `firmware/target/hosted/sdl/pcm-sdl.c`     |
| `PCM_SINK_FIFO`        | 1     | `firmware/target/hosted/pcm-fifo.c`        |
| `PCM_SINK_AIRPLAY`     | 2     | `firmware/target/hosted/pcm-airplay.c`     |
| `PCM_SINK_SQUEEZELITE` | 3     | `firmware/target/hosted/pcm-squeezelite.c` |

`crates/settings/src/lib.rs:load_settings()` reads `audio_output` and calls `pcm::switch_sink()`.

Rust constants + helpers live in `crates/sys/src/sound/pcm.rs`.

### FIFO sink (Snapcast)
- Pre-creates the named FIFO with `O_RDWR|O_NONBLOCK` in `pcm_fifo_set_path()` then clears `O_NONBLOCK`, so a permanent writer reference is held — readers never see premature EOF between tracks.
- `sink_dma_stop()` does NOT close the fd; it stays open across track transitions.
- **Startup order matters**: rockboxd must start before snapserver. If snapserver opens the FIFO first it may get EOF and stop reading.
- On macOS, snapserver v0.35.0 ignores the `-s` sample-format CLI flag; use `/usr/local/etc/snapserver.conf`:
  ```ini
  [stream]
  source = pipe:///tmp/snapfifo?name=default&sampleformat=44100:16:2
  ```

### AirPlay sink (RAOP)
- `crates/airplay/` implements the full RAOP stack in pure Rust (no tokio needed).
  - `alac.rs` — ALAC escape/verbatim frame encoder: 352 stereo S16LE samples → 1411-byte bitstream
  - `rtp.rs` — RTP/UDP packet sender; RTCP NTP sync packets sent every ~44 frames
  - `rtsp.rs` — synchronous RTSP client: ANNOUNCE (SDP) → SETUP → RECORD
- `pcm_airplay_connect()` is called once per `sink_dma_start()` (idempotent if already connected).
- The `rockbox-airplay` rlib must be force-included in `librockbox_cli.a` via the `use rockbox_airplay::_link_airplay as _` shim in `crates/cli/src/lib.rs`.

### Squeezelite sink (Slim Protocol + HTTP broadcast)
- `crates/slim/` implements a Slim Protocol TCP server and an HTTP PCM broadcast server, both in pure Rust.
  - `slimproto.rs` — accepts squeezelite connections; sends `STRM 's'` pointing at the HTTP port; replies to every `STMt` heartbeat with `audg` to prevent squeezelite's 36-second watchdog from firing.
  - `http.rs` — concurrent HTTP server (one thread per client); each client gets an independent `BroadcastReceiver` cursor into the shared buffer, enabling true multi-room playback.
  - `lib.rs` — `BroadcastBuffer`: sequence-numbered chunks, per-reader cursors, 4 MB cap with oldest-first eviction; lagging readers skip forward rather than blocking the writer.
- `firmware/target/hosted/pcm-squeezelite.c` paces the DMA loop to real time using `CLOCK_MONOTONIC`. **Use `int64_t` for the nanosecond diff** — unsigned subtraction wraps catastrophically when `tv_nsec` rolls over.
- The `rockbox-slim` rlib must be force-included via `use rockbox_slim::_link_slim as _` in `crates/cli/src/lib.rs`.
- **Slim Protocol framing**: client→server is `opcode[4] + u32_t length BE + payload`; server→client is `u16_t length BE + opcode[4] + payload` (length does NOT include the 2-byte length field itself).
- **ASCII-encoded PCM fields in STRM**: squeezelite subtracts `'0'` from `pcm_sample_size`, `pcm_sample_rate`, `pcm_channels`, `pcm_endianness`. Correct values: `'1'` (16-bit), `'3'` (44100 Hz), `'2'` (stereo), `'1'` (little-endian).

## Key cross-cutting concerns

### macOS SDL audio
`SDL_InitSubSystem(SDL_INIT_AUDIO)` must be called explicitly on macOS because the SDL event thread (which normally does it) is `#ifndef __APPLE__`. This is done in `firmware/target/hosted/sdl/system-sdl.c` in the `#else` branch of the event-thread guard.

### SIGTERM handling
`system-hosted.c` installs a SIGTERM handler that loops forever (waits for SDL quit event). `crates/cli/src/lib.rs` overrides SIGTERM/SIGINT with a handler that kills the typesense child and calls `_exit(0)`.

### Typesense subprocess
Spawned in `crates/cli/src/lib.rs` with `Stdio::piped()`. stdout/stderr lines are forwarded to `tracing::debug!`/`tracing::warn!` in background threads, keeping the PCM stdout stream clean in FIFO mode.

### Logging — use `tracing`, never `eprintln!`/`println!`
All Rust logging must use the `tracing` crate (`tracing::debug!`, `tracing::info!`, `tracing::warn!`, `tracing::error!`). **Never use `eprintln!` or `println!` for diagnostic output** in Rust code — they bypass the structured log filter, pollute stdout (breaking FIFO/pipe mode), and can't be silenced at runtime.

Severity guide:
- `tracing::error!` — unrecoverable failures (connection refused, missing config)
- `tracing::warn!` — recoverable issues (non-fatal fallbacks, unexpected-but-handled states)
- `tracing::info!` — notable lifecycle events (session established, device paired)
- `tracing::debug!` — per-packet/per-frame detail, protocol negotiation steps

`tracing` is declared as a workspace dependency in the root `Cargo.toml`; add `tracing = { workspace = true }` to any crate that needs it. Control verbosity at runtime with `RUST_LOG`, e.g. `RUST_LOG=debug rockboxd` or `RUST_LOG=rockbox_airplay=debug,info`.

### HTTP streaming
HTTP fds are encoded as values `<= STREAM_HTTP_FD_BASE (-1000)`. `stream_open/read/lseek/close` in `crates/netstream/` dispatch between HTTP and POSIX based on fd value. The global `STREAMS` map holds `Arc<Mutex<StreamState>>` per handle so concurrent reads don't serialize on a single lock.

## Adding a new PCM sink

1. Create `firmware/target/hosted/pcm-<name>.c` — model on `pcm-fifo.c`.
2. Add `PCM_SINK_<NAME>` to the enum in `firmware/export/pcm_sink.h`.
3. Register `&<name>_pcm_sink` in the `sinks[]` array in `firmware/pcm.c`.
4. Add `target/hosted/pcm-<name>.c` inside the `#if PLATFORM_HOSTED` block in `firmware/SOURCES`.
5. Add Rust constant `PCM_SINK_<NAME>: i32` in `crates/sys/src/sound/pcm.rs`.
6. Add a `set_<name>_*` wrapper if configuration is needed.
7. Handle in `crates/settings/src/lib.rs:load_settings()`.
8. If the sink has a Rust implementation in a new crate: add a `_link_<name>()` dummy fn and reference it from `crates/cli/src/lib.rs` to force inclusion in the staticlib.

## Mobile app (`expo/`)

A React Native client (Expo Router + NativeWind) lives in `expo/`. It mirrors
the GPUI desktop layout (`gpui/src/ui/`) — same dark palette, same
Spotify/Tidal-inspired information architecture: bottom-tab shell with a
persistent miniplayer, full-screen player modal, queue modal, and detail
screens for album / artist / playlist / genre. Most state is mock-only today;
real data should plug into the rockboxd gRPC / GraphQL client (`crates/server/`).

### Stack
- **Expo SDK 54** + **expo-router** for file-based routing (`app/`).
- **NativeWind 4** with Tailwind 3 — class-based styling against a custom palette
  declared in both `expo/tailwind.config.js` and `expo/constants/theme.ts`
  (keep the two in sync).
- `expo-image`, `expo-blur`, `expo-linear-gradient`, `@expo/vector-icons`
  (Ionicons + MaterialCommunityIcons), `react-native-safe-area-context`.

### Layout
```
expo/
├── app/
│   ├── _layout.tsx                 root stack, fonts, PlayerProvider, modals
│   ├── (tabs)/_layout.tsx          custom tab bar with merged miniplayer dock
│   ├── (tabs)/{index,search,library}.tsx
│   ├── player.tsx, queue.tsx, settings.tsx
│   ├── album/[id].tsx, artist/[id].tsx, playlist/[id].tsx, genre/[id].tsx
│   └── playlist/new.tsx            create regular OR smart (?mode=smart)
├── components/                     mini-player, action-sheet, track-context-menu, …
├── lib/
│   ├── player-context.tsx          single source of truth for playback state
│   ├── mock-data.ts                ALBUMS / ARTISTS / PLAYLISTS / GENRES + helpers
│   └── nativewind-setup.ts         cssInterop registrations (must be imported)
├── constants/theme.ts              `Colors` palette consumed by inline styles
├── tailwind.config.js              `Colors` palette mirrored as Tailwind tokens
├── babel.config.js                 babel-preset-expo + nativewind/babel
└── metro.config.js                 withNativeWind({ input: './global.css' })
```

### Styling rules — NativeWind only

- **Always use `className` for styling.** Inline `style={{...}}` is reserved for
  values className genuinely cannot express: `Animated.Value` bindings,
  runtime-computed widths (`` `${pct * 100}%` ``), per-instance shadow tokens,
  or colors derived from data (e.g. `genre.color`).
- **Never combine a function `style={(state) => ({...})}` with `className` on
  the same element.** The function `style` overrides NativeWind's class output
  and silently drops every utility on that element. Use arbitrary-value classes
  (`w-[48.5%]`, `h-[100px]`, `aspect-square`) and the `active:` variant for
  press feedback instead.
- Static `style={{...}}` objects (no callbacks) merge fine with `className`.
- Color tokens live in `tailwind.config.js` under `bg.*`, `accent.*`, `text.*`,
  `border`, `divider`, `slider.*`, `danger`. Reach for these (`bg-bg-card`,
  `text-text-secondary`, `bg-accent`) instead of hard-coded hex values.
- `expo-image`, `expo-blur`, `expo-linear-gradient`, and the safe-area
  `SafeAreaView` are wired up via `cssInterop` in `lib/nativewind-setup.ts` —
  any other third-party component needs to be registered there before it can
  accept `className`.
- Fonts: `font-sans` → SpaceGrotesk (UI), `font-mono` → JetBrainsMono
  (durations / numerics). The TTFs are bundled via the `expo-font` plugin in
  `app.json` and copied from `gpui/assets/fonts/`.

### Player state
`lib/player-context.tsx` holds queue, currentIdx, position (1 Hz tick),
isPlaying, shuffle, repeat, liked, userPlaylists, and the global track / entity
context-menu state. The mock advances `position` and auto-advances tracks; the
real implementation should replace the action handlers with rockboxd RPC calls
while keeping the same shape so the UI doesn't need to change.

### Useful commands
```sh
cd expo
bun install                        # or npm/yarn
bun run start                      # iOS / Android / web via expo-router
bunx tsc --noEmit                  # type check
bunx expo lint                     # lint
bunx expo export --platform web    # smoke-test the bundle (catches NativeWind transform issues)
```

### Native gRPC client — `crates/expo/` + `expo/modules/rockbox-rpc/`

The mobile app talks to rockboxd through a native module that wraps a real
tonic gRPC client written in Rust. It is split in two halves:

**`crates/expo/`** — `rockbox-expo` Rust crate, `staticlib + cdylib`.
- Generates client-only proto bindings in `build.rs` from the shared
  `crates/rpc/proto` tree (linked in via the `proto -> ../rpc/proto` symlink
  inside the crate so we don't duplicate `.proto` files).
- Owns a single multi-thread Tokio runtime via `once_cell`.
- Exposes a flat C ABI (`rb_set_server_url`, `rb_ping`, `rb_play`, `rb_pause`,
  `rb_play_pause`, `rb_next`, `rb_prev`, `rb_seek`, `rb_status_json`,
  `rb_current_track_json`, `rb_like_track`, `rb_unlike_track`,
  `rb_free_string`). Complex responses are returned as heap-allocated JSON
  C strings — caller MUST free via `rb_free_string`. Simple ops return `i32`
  status codes (0 = ok, <0 = error).
- Deliberately does NOT depend on `rockbox-rpc` to avoid pulling sqlx /
  typesense / library transitive deps that fight cross-compilation.

**`expo/modules/rockbox-rpc/`** — Expo SDK 54 native module.
- `expo-module.config.json` declares iOS + Android module classes; the module
  is autolinked into the app via `expo/package.json` (`"rockbox-rpc": "file:./modules/rockbox-rpc"`).
- iOS: `ios/RockboxRpcModule.swift` declares each `rb_*` symbol with
  `@_silgen_name(...)` and exposes them through `Function` / `AsyncFunction`.
  The static library is delivered as `ios/RockboxExpo.xcframework` (built by
  `scripts/build-ios.sh`); the `.podspec` `vendored_frameworks` it.
- Android: `android/src/main/java/expo/modules/rockboxrpc/RockboxRpcModule.kt`
  uses `System.loadLibrary("rockbox_expo")` + JNI `external fun` declarations.
  The `.so` per ABI is dropped into `android/src/main/jniLibs/<abi>/` by
  `scripts/build-android.sh` (uses `cargo-ndk`).
- TS facade: `expo/modules/rockbox-rpc/src/index.ts` declares the JS surface;
  `expo/lib/rockbox-client.ts` is the in-app helper with an `isAvailable`
  flag so callers can fall back to the mock `PlayerProvider` on web or when
  the libs haven't been built yet.

#### Building the native libs

```sh
# iOS — produces expo/modules/rockbox-rpc/ios/RockboxExpo.xcframework
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
cd expo/modules/rockbox-rpc
bun run build:ios

# Android — produces expo/modules/rockbox-rpc/android/src/main/jniLibs/<abi>/librockbox_expo.so
cargo install cargo-ndk
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
export ANDROID_NDK_HOME=...   # NDK r25+
bun run build:android
```

After the native libs are in place, run `bunx expo prebuild` and then
`bunx expo run:ios` / `run:android` to bundle them into the app.

#### Adding new RPCs

1. Add a thin wrapper in `crates/expo/src/lib.rs` (`rb_<name>` returning
   `c_int` for unit ops or `*mut c_char` for JSON-bearing reads).
2. Add the matching extern declaration in both
   `expo/modules/rockbox-rpc/ios/RockboxRpcModule.swift` and
   `expo/modules/rockbox-rpc/android/src/main/java/.../RockboxRpcModule.kt`,
   plus an `AsyncFunction` binding.
3. Add the typed method to `expo/modules/rockbox-rpc/src/index.ts` and the
   forwarding helper in `expo/lib/rockbox-client.ts`.
4. Rebuild the native libs (`build:ios` / `build:android`) — `metro` doesn't
   pick up native changes automatically.

#### Streaming subscriptions

Server-streaming RPCs (`StreamStatus`, `StreamCurrentTrack`, `StreamPlaylist`)
are exposed as JS events — not async iterators — to play nicely with React's
render loop. The pipeline is:

```
tonic stream
  → tokio mpsc<String>     (one queue per subscription, in crates/expo)
    → rb_poll_event(id, timeout_ms) -> *mut c_char
       → Swift dispatch_async / Kotlin Dispatchers.IO loop
          → sendEvent("rockbox.<topic>", payload)  (Expo Modules EventEmitter)
             → RockboxRpc.addListener("rockbox.<topic>", cb)
```

Each `subscribe*` returns an opaque numeric subscription id; the JS facade in
`expo/lib/rockbox-client.ts` wraps that with an `() => void` unsubscribe
helper that removes both the event listener and the native subscription:

```ts
const unsubscribe = RockboxClient.subscribeStatus(
  (s) => console.log("status", s.status),
  (e) => console.warn("stream error", e.error),
);
// later: unsubscribe();
```

Topics today: `rockbox.status`, `rockbox.currentTrack`, `rockbox.playlist`,
`rockbox.library`, `rockbox.discovery` (LAN mDNS / Bonjour scan via the
`rockbox-discovery` crate — emits one `DiscoveredService` per resolved peer),
plus `rockbox.error` for stream failures (carries `subId`, `stream`, `error`).

The `subscribeDiscovery` helper defaults to the `_rockbox._tcp.local.`
service; pass any other Bonjour service name (e.g. `_googlecast._tcp.local.`)
to scan for Chromecast / etc. Constants are also surfaced on the JS side via
`RockboxClient.rockboxServiceName()` and `RockboxClient.chromecastServiceName()`.

To add a new streamed RPC: add a `rb_subscribe_<name>` in `crates/expo/src/lib.rs`
that follows the `spawn_stream(...)` pattern, declare the matching event topic
in the iOS / Android `Events(...)` lists, register a `Function("subscribe<Name>")`
in both modules, and add the typed `subscribe<Name>(cb, onError?)` helper to
`expo/lib/rockbox-client.ts`.

#### Embedded daemon — Android cdylib (`embedded-daemon` feature)

The Android build of `librockbox_expo.so` can host a **full in-process
rockboxd**: C firmware + codecs + Rust gRPC/HTTP/GraphQL/MPD servers + AAudio
sink + mDNS advertising. The phone becomes a symmetric peer of any LAN
rockboxd, while keeping the existing tonic gRPC client to control other peers.

Enable with `--features embedded-daemon` (the `expo/modules/rockbox-rpc/scripts/build-android.sh`
script does this by default). Without the feature the .so is the thin
~6 MB remote-only client; with it, ~48 MB.

```sh
PROFILE=release bash expo/modules/rockbox-rpc/scripts/build-android.sh
```

**Architecture (cdylib):**
- Static-linked codecs (BINFMT_STATIC) — `lib/rbcodec/codecs/codecs.make` runs
  `objcopy --redefine-sym` per codec to make `__header`, `codec_main`, `codec_run`,
  `codec_start` distinct symbols. Codec lookup goes through `lc_static_table[]`
  in `firmware/target/hosted/android/cdylib/lc-android.c` instead of `dlopen`.
- Per-target headless config: `firmware/export/config/androidcdylib.h` defines
  `CONFIG_BINFMT BINFMT_STATIC`, `CONFIG_PLATFORM (PLATFORM_HOSTED|PLATFORM_ANDROID)`,
  `ROCKBOX_SERVER`, `CONFIG_SERVER`, plus a `DEBUGF debugf` override so firmware
  diagnostics surface in logcat (debug-android.c routes `debugf` →
  `__android_log_print`).
- New cdylib-only sources under `firmware/target/hosted/android/cdylib/`:
  `system-android.c` (boot + stdout/stderr→logcat shim), `pcm-aaudio.c`
  (AAudio sink), `lc-android.c` (codec table loader), `rb_zig_compat.c`
  (C compat layer for the 18 `rb_*` symbols `crates/sys` expects from
  the Zig wrapper), plus stubs `lcd-noop.c`, `button-noop.c`, etc.
- `crates/expo/src/daemon.rs` wraps the firmware boot:
  `rb_daemon_start(configDir, musicDir, deviceName)` spawns a pthread that
  calls `main_c()`, then waits up to 30s for `crates/server::start_servers()`
  to bind gRPC :6061. Auto-runs an audio scan after gRPC binds (skipped if
  the library DB already has tracks; force with `RockboxClient.rescanLibrary()`
  or `ROCKBOX_UPDATE_LIBRARY=1`).
- The daemon module is referenced from the Expo module's `OnCreate` lifecycle
  hook in `RockboxRpcModule.kt`, so the daemon boots at app launch and the
  process stays alive via the foreground `NowPlayingService`.

**Permissions / paths:**
- `MANAGE_EXTERNAL_STORAGE` declared in `expo/android/app/src/main/AndroidManifest.xml`
  — required so the filesystem-based scanner can read `/storage/emulated/0/Music`
  on API 33+. `READ_MEDIA_AUDIO` doesn't help (it only grants MediaStore queries).
  The `useAllFilesAccessPrompt()` hook in `expo/app/_layout.tsx` opens system
  Settings → "All files access" the first time the user runs the app.
- The daemon sets `ROCKBOX_LIBRARY=<musicDir>` env var (canonical, read by
  `crates/{settings,server,graphql}`); previous builds set the misnomer
  `ROCKBOX_MUSIC_DIR` which nothing read.
- `firmware/target/hosted/android/debug-android.c` routes firmware
  `printf`/`fprintf` and DEBUGF to logcat under tag `Rockbox`.
  `system-android.c::redirect_stdio_to_logcat` adds a pthread that pipes
  stdout/stderr fds to `__android_log_write` so even raw `printf` calls
  (the `[metadata]`/`[streamfd]` chatter Rockbox emits) are visible.

**JS-callable controls (in addition to the remote-only surface):**
- `RockboxClient.rescanLibrary()` — force a full audio scan
- `RockboxClient.hasAllFilesAccess()` / `requestAllFilesAccess()` — Android
  permission gating
- `RockboxNowPlaying.start()` — early foreground-service promotion (so the
  process survives backgrounding while the daemon is running)

**Common pitfalls (see auto-memory):**
- `pcm_sink::set_freq` receives an INDEX into `hw_freq_sampr[]`, not Hz —
  AAudio gets opened at "4 Hz", silently falls back to 48 kHz, 44.1 kHz
  audio plays ~9 % fast (chipmunk effect). Look up the rate first.
- `apps/codecs.c::ci` (struct) collides with each codec's
  `codec_crt0.c::ci` (pointer) at link time. Firmware-side rename to
  `firmware_ci` keeps the type/size invariants distinct.
- `apps/main.c` gates `server_init()` on `ROCKBOX_SERVER` but `apps/SOURCES`
  gates the .c COMPILATION on `CONFIG_SERVER` — define BOTH.
- Android 14+ blocks `startForegroundService` from background process state
  even with `mediaPlayback` type. `startServiceCompat` checks importance
  before promoting; `refreshNotification` does the same before
  `startForeground`.

See `crates/expo/README.md` for the full architecture writeup.

## WASM browser build

The WASM target is **lightweight**: it compiles only the extracted core crates
— `rockbox-codecs` (decode), `rockbox-dsp` (EQ/DSP), `rockbox-metadata` (tags)
— through the `rockbox-ffi` flat C ABI, into `web/rockbox-core.{js,wasm}`
(~1.2 MB). There is **no firmware, no netstream, no playlist engine, no
servers**; the player (queue, transport, output) is plain JavaScript in `web/`.
See `WEBASSEMBLY.md` for the full writeup.

Build: `bash scripts/build-wasm.sh`. Serve: `node scripts/wasm-dev-server.mjs`
(COOP/COEP required for SharedArrayBuffer; COEP is `credentialless` so
cross-origin webfonts still load).

The build is two steps: `cargo rustc -p rockbox-ffi --no-default-features
--crate-type staticlib` for `wasm32-unknown-emscripten` (`--no-default-features`
drops the `player` feature → no `rockbox-playback`/`cpal`, which need a real
output device), then an `emcc` link. Emscripten pthreads need
`-Wl,--no-check-features` (prebuilt Rust std objects lack the `atomics`
feature).

### Browser architecture (three JS files)

- `web/rockbox.js` — main-thread `RockboxPlayer` facade: `AudioContext`, a
  `GainNode` for volume (not a rockbox-dsp stage), events, and `postMessage`
  to the Worker.
- `web/rockbox-decoder-worker.js` — owns the WASM module; runs the whole
  player loop (fetch → MEMFS → `rb_decoder_next_chunk` → `rb_dsp_process` →
  resample) and writes PCM into a shared ring. Decode **must** be off the main
  thread: `rockbox-codecs::Decoder` blocks on a `Condvar` (`Atomics.wait`),
  which throws on the main browser thread.
- `web/rockbox-audio-worklet.js` — `AudioWorkletProcessor` reading the shared
  `SharedArrayBuffer` ring → speakers.

### Exposing new functions to JS

1. Add the `rb_*` function to `rockbox-ffi` (`crates/rockbox-ffi/src/`).
2. Add `"_rb_<name>"` to `EXPORTED_FUNCTIONS` in `scripts/build-wasm.sh` —
   missing entries are silently dead-stripped (`Module._rb_foo === undefined`).
3. Call it from the decoder Worker / add a `RockboxPlayer` method.
4. Rebuild — the emcc link step must re-run to pick up the new export.

### DSP / EQ value units

The **`rockbox-dsp` FFI takes plain units** — `rb_dsp_set_eq_band(dsp, band,
cutoff_hz, q, gain_db)` uses a plain `f32` Q and plain-dB gain; `rb_dsp_set_eq_precut(db)`
is plain dB. The crate converts to Rockbox's internal tenths for you, so do
**not** pre-multiply by 10 (unlike the old firmware `rb_set_eq_band`, which
took `dB × 10`).

## Useful commands

```sh
# Run the daemon
./zig/zig-out/bin/rockboxd

# Run with AirPlay debug logging
RUST_LOG=debug ./zig/zig-out/bin/rockboxd

# Test FIFO → stdout pipe
./zig/zig-out/bin/rockboxd | ffplay -f s16le -ar 44100 -ac 2 -

# Check binary vs library timestamps
ls -la zig/zig-out/bin/rockboxd build-lib/libfirmware.a target/release/librockbox_cli.a

# Verify AirPlay symbols are present
nm zig/zig-out/bin/rockboxd | grep pcm_airplay

# Verify squeezelite symbols are present
nm zig/zig-out/bin/rockboxd | grep pcm_squeezelite

# Verify a crate is in the staticlib
ar t target/release/librockbox_cli.a | grep airplay
ar t target/release/librockbox_cli.a | grep slim

# Multi-room squeezelite test
squeezelite -s localhost -n "Room 1"
squeezelite -s localhost -n "Room 2"
```
