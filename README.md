> [!IMPORTANT]
> **Do not try to flash this project onto your DAP (digital audio player).**
> This is **not** Rockbox firmware. Rockbox Daemon is a background daemon for
> **Linux and macOS** that runs on your computer — flashing it to a portable
> player will not work and is not supported.

<div>
  <img src="https://www.rockbox.org/rockbox400.png" />
</div>

# Rockbox Daemon 🎵 ⚡

[![GPL-2.0 licensed](https://img.shields.io/badge/License-GPL-blue.svg)](./LICENSE)
[![ci](https://github.com/tsirysndr/rockboxd/actions/workflows/ci.yml/badge.svg)](https://github.com/tsirysndr/rockboxd/actions/workflows/ci.yml)
[![Docker Pulls](https://img.shields.io/docker/pulls/tsiry/rockbox)](https://hub.docker.com/r/tsiry/rockbox)
![GitHub Downloads (all assets, all releases)](https://img.shields.io/github/downloads/tsirysndr/rockboxd/total)
[![discord](https://img.shields.io/discord/1292855167921815715?label=discord&logo=discord&color=5865F2)](https://discord.gg/tXPrgcPKSt)
[![storybook](https://raw.githubusercontent.com/storybooks/brand/master/badge/badge-storybook.svg)](https://master--670ceec25af685dcdc87c0df.chromatic.com/?path=/story/components-albums--default)

![Rockbox UI](./docs/gpui.png)

A modern take on the [Rockbox](https://www.rockbox.org) open source audio
player, extended with Rust and Zig. Rockbox Daemon exposes the full Rockbox audio
engine — gapless playback, DSP, 20+ codecs, tag database — through gRPC,
GraphQL, HTTP, and MPD APIs, in a single binary (`rockboxd`) and adds multi-room output via AirPlay, Snapcast,
and Squeezelite.

![Preview](./docs/preview.png)
![Desktop](./docs/desktop.png)
![macOS media controls](./docs/media-controls.png)
![macOS preview](./docs/preview-mac.png)

---

## 📑 Table of Contents

- [✨ Features](#-features)
- [🚀 Quick Start](#-quick-start)
- [🔌 Ports](#-ports)
- [⚙️ Audio Output Configuration](#️-audio-output-configuration)
  - [Built-in CPAL — default](#built-in-cpal--default)
  - [HLS + MPEG-DASH (CMAF)](#hls--mpeg-dash-cmaf)
  - [Snapcast](#snapcast)
  - [AirPlay (RAOP) — single or multi-room](#airplay-raop--single-or-multi-room)
  - [Squeezelite (Slim Protocol — multi-room)](#squeezelite-slim-protocol--multi-room)
  - [Chromecast](#chromecast)
  - [UPnP / DLNA](#upnp--dlna)
- [🚚 Installation](#-installation)
- [📦 Downloads](#-downloads)
- [🧙‍♂️ Systemd Service](#️-systemd-service)
- [🏗️ Compiling from Source](#️-compiling-from-source)
  - [🎛️ Console — one entry point for every build/dev/ops command](#️-console--one-entry-point-for-every-builddevops-command)
- [🧑‍🔬 Architecture](#-architecture)
- [📚 APIs](#-apis)
  - [GraphQL](#graphql)
  - [HTTP REST](#http-rest)
  - [gRPC](#grpc)
  - [S3-compatible API](#s3-compatible-api)
- [📖 Documentation](#-documentation)

---

## ✨ Features

### Audio output
- [x] Built-in [CPAL](https://github.com/RustAudio/cpal) audio
- [x] [HLS](https://developer.apple.com/streaming/) + [MPEG-DASH](https://dashif.org) (CMAF / fMP4) — plays directly in any browser, no extra client needed
- [x] [AirPlay](https://en.wikipedia.org/wiki/AirPlay) (RAOP) — single or multi-room fan-out to Apple TV, HomePod, Airport Express, shairport-sync
- [x] [Snapcast](https://github.com/snapcast/snapcast) — synchronised multi-room via snapserver (FIFO/pipe **and** direct TCP with mDNS auto-discovery)
- [x] [Squeezelite](https://github.com/ralph-irving/squeezelite) (Slim Protocol + HTTP broadcast) — synchronised multi-room
- [x] [Chromecast](https://developers.google.com/cast)
- [x] Gapless playback and crossfading
- [x] Supports 20+ codecs: MP3, OGG, FLAC, WAV, AAC, Opus, and more

### APIs & integrations
- [x] [gRPC API](https://buf.build/tsiry/rockboxapis/docs/main:rockbox.v1alpha1)
- [x] GraphQL API
- [x] HTTP REST API
- [x] [MPD](https://mpd.readthedocs.io/en/stable/protocol.html) server — compatible with all MPD clients
- [x] [MPRIS](https://specifications.freedesktop.org/mpris-spec/) — desktop media key and taskbar integration
- [x] Subsonic/Navidrome API — compatible with Cassette, Symfonium, DSub, Ultrasonic and more
- [x] Jellyfin-compatible API — works with native Jellyfin clients (Finamp, Findroid, Streamyfin, Amcfy Music, Symfonium); reuses the Subsonic credentials, opt-in by setting `jellyfin_port` (conventionally `8096`) in `settings.toml`, with mDNS + UDP 7359 discovery
- [x] S3-compatible API — upload / delete audio files from `awscli`, `mc`, `rclone`; the library DB stays in sync automatically
- [x] Fast search powered by [Typesense](https://typesense.org)
- [x] Navigate by folders or tag database
- [x] UPnP/DLNA
- [x] Android library
- [x] WebAssembly target

### Clients
- [x] Web client (React)
- [x] Desktop client (Native MacOS / GPUI / GTK4)
- [x] Mobile app (React Native)
- [x] Terminal client (TUI)
- [x] Rockbox REPL

### Planned
- [ ] Stream from YouTube / Spotify / Tidal
- [ ] TuneIn Radio
- [ ] Kodi output
- [ ] TypeScript ([Deno](https://deno.com)) plugin API
- [ ] Wasm extensions

---

## 🚀 Quick Start

### 🐳 Docker

The fastest way to get started — no install, no config file, no external client needed.
The default `audio_output` is **CMAF (HLS + DASH)**, so audio plays straight in
your browser:

```sh
docker run -v $HOME/Music:/root/Music \
  -p 6062:6062 \
  -p 7882:7882 \
  tsiry/rockbox
```

Open the web UI at [http://localhost:6062](http://localhost:6062) and start
playing — the page attaches to the HLS stream on port `7882` automatically.

Prefer the terminal? Any HLS-capable player can consume the stream directly:

```sh
ffplay http://localhost:7882/hls/master.m3u8
# or: vlc http://localhost:7882/hls/master.m3u8
# or: mpv  http://localhost:7882/hls/master.m3u8
```

<details>
<summary>Want Snapcast multi-room instead of HLS?</summary>

The image still ships a `snapserver` so you can opt into the FIFO/Snapcast path
without rebuilding. Expose its ports and edit
`~/.config/rockbox.org/settings.toml` inside the container to set
`audio_output = "fifo"`:

```sh
docker run -v $HOME/Music:/root/Music \
  -p 6062:6062 \
  -p 1704:1704 \
  -p 1705:1705 \
  -p 1780:1780 \
  tsiry/rockbox
```

Then connect a [Snapcast](https://github.com/snapcast/snapcast) client:

```sh
snapclient tcp://localhost
```

</details>

---

### Manual install

1. **Install** (see [Installation](#-installation) below).

2. **Create `~/.config/rockbox.org/settings.toml`**:

```toml
music_dir    = "/path/to/your/Music"
audio_output = "builtin"   # CPAL audio — see Audio Output for other options
playlist_shuffle = false
repeat_mode = 1
bass = 3
treble = 3
bass_cutoff = 0
treble_cutoff = 0
crossfade = 5
fade_on_stop = false
fade_in_delay = 2
fade_in_duration = 7
fade_out_delay = 4
fade_out_duration = 0
fade_out_mixmode = 2
balance = 0
stereo_width = 100
stereosw_mode = 0
surround_enabled = 0
surround_balance = 0
surround_fx1 = 0
surround_fx2 = 0
party_mode = true
channel_config = 0
player_name = ""
eq_enabled = true


[[eq_band_settings]]
cutoff = 60
q = 70
gain = 25

[[eq_band_settings]]
cutoff = 200
q = 70
gain = 55

[[eq_band_settings]]
cutoff = 500
q = 70
gain = 40

[[eq_band_settings]]
cutoff = 1000
q = 70
gain = -140

[[eq_band_settings]]
cutoff = 2000
q = 70
gain = -115

[[eq_band_settings]]
cutoff = 4000
q = 70
gain = -60

[[eq_band_settings]]
cutoff = 7000
q = 70
gain = 10

[[eq_band_settings]]
cutoff = 10000
q = 70
gain = 50

[[eq_band_settings]]
cutoff = 14000
q = 70
gain = 65

[[eq_band_settings]]
cutoff = 20000
q = 70
gain = 50

[replaygain_settings]
noclip = true
type = 0
preamp = 0

[compressor_settings]
threshold = -24
makeup_gain = 0
ratio = 4
knee = 1
release_time = 300
attack_time = 5
```

3. **Start Rockbox**:

```sh
rockbox
```

4. **Open the web UI** at [http://localhost:6062](http://localhost:6062) or connect any MPD client to `localhost:6600`.

---

## 🔌 Ports

| Service                              | Default port | Protocol        |
| ------------------------------------ | ------------ | --------------- |
| gRPC                                 | 6061         | gRPC / gRPC-Web |
| GraphQL + Web UI                     | 6062         | HTTP            |
| HTTP REST API                        | 6063         | HTTP            |
| MPD server                           | 6600         | MPD protocol    |
| Subsonic / Navidrome API             | 4533         | HTTP            |
| S3-compatible API                    | 9000         | HTTP            |
| CMAF (HLS + DASH)                    | 7882         | HTTP            |
| Slim Protocol (squeezelite)          | 3483         | TCP             |
| HTTP PCM stream (squeezelite)        | 9999         | HTTP            |
| Chromecast WAV stream                | 7881         | HTTP            |
| UPnP Media Server (ContentDirectory) | 7878         | HTTP / SSDP     |
| UPnP WAV broadcast (PCM sink)        | 7879         | HTTP            |
| UPnP MediaRenderer (AVTransport)     | 7880         | HTTP / SSDP     |

---

## ⚙️ Audio Output Configuration

Rockbox reads `~/.config/rockbox.org/settings.toml` at startup.
`music_dir` is always required. `audio_output` defaults to `"builtin"` if
omitted.

### Built-in CPAL — default

```toml
music_dir    = "/path/to/Music"
audio_output = "builtin"
```

Uses [CPAL](https://github.com/RustAudio/cpal) — plays through the OS default device. No extra setup needed.

### HLS + MPEG-DASH (CMAF)

```toml
music_dir      = "/path/to/Music"
audio_output   = "cmaf"           # also accepts "hls" or "dash"
cmaf_http_port = 7882             # optional, default 7882
cmaf_bitrate   = 128000           # optional, AAC-LC bitrate in bps
```

Encodes live audio as AAC-LC in fragmented MP4 (CMAF) and serves it as both
HLS and MPEG-DASH from the same in-memory ring buffer. Any HLS-capable client
can play the stream — including every modern browser — with no extra software:

```
http://<host>:7882/hls/master.m3u8     # HLS
http://<host>:7882/dash/manifest.mpd   # MPEG-DASH
```

This is the default for the Docker image: the web UI's `<audio>` element
attaches to the HLS stream automatically as soon as the active output is set
to `cmaf` / `hls` / `dash`. Try it from the terminal too:

```sh
ffplay http://localhost:7882/hls/master.m3u8
vlc    http://localhost:7882/hls/master.m3u8
mpv    http://localhost:7882/hls/master.m3u8
```

A 2-second sliding window of segments is kept in memory; clients always join at
the live edge. Optionally mirror segments + manifests to disk for an external
HTTP server (nginx, Caddy, a CDN origin) by adding:

```toml
cmaf_segment_dir = "/var/www/rockbox-cmaf"
```

### Snapcast

Rockbox supports two ways to feed [Snapcast](https://github.com/snapcast/snapcast)
for synchronised multi-room playback. Both write raw **S16LE stereo 44100 Hz**
PCM to snapserver.

#### TCP (recommended — auto-discovery)

```toml
music_dir         = "/path/to/Music"
audio_output      = "snapcast_tcp"
snapcast_tcp_host = "192.168.1.x"   # IP of the machine running snapserver
snapcast_tcp_port = 4953            # default snapserver TCP source port
```

Connects directly to snapserver's TCP source port. No named FIFO or filesystem
dependency needed.

```ini
# /etc/snapserver.conf  (or /usr/local/etc/snapserver.conf on macOS)
[stream]
source = tcp://0.0.0.0:4953?name=default&sampleformat=44100:16:2
```

> **Startup order**: start `snapserver` first so it is already listening when
> rockboxd begins playback. If the connection drops (e.g. snapserver restarts),
> it is re-established automatically on the next play call.

> **Auto-discovery**: rockboxd scans for `_snapcast._tcp.local.` via mDNS at
> startup. Discovered servers appear in the web UI and desktop app device
> picker — just click to connect, no config file editing needed.

#### FIFO / pipe

```toml
music_dir    = "/path/to/Music"
audio_output = "fifo"
fifo_path    = "/tmp/snapfifo"   # named FIFO for snapserver; use "-" for stdout
```

Writes to a named FIFO. Use this when you need stdout piping or prefer the
traditional pipe model.

```ini
# /etc/snapserver.conf  (or /usr/local/etc/snapserver.conf on macOS)
[stream]
source = pipe:///tmp/snapfifo?name=default&sampleformat=44100:16:2
```

> **Startup order**: start `rockboxd` before `snapserver`. Rockbox holds a
> permanent write reference on the FIFO so snapserver never sees a premature
> EOF between tracks.

Pipe to any PCM consumer with `fifo_path = "-"`:

```sh
rockboxd | ffplay -f s16le -ar 44100 -ac 2 -
```

See [SNAPCAST.md](./SNAPCAST.md) for a detailed comparison of both modes,
connection lifecycle, reconnect behaviour, and macOS quirks.

### AirPlay (RAOP) — single or multi-room

Single receiver:

```toml
music_dir    = "/path/to/Music"
audio_output = "airplay"
airplay_host = "192.168.1.50"   # IP of the AirPlay receiver
airplay_port = 5000             # optional, default 5000
```

Multi-room (fan-out to N receivers simultaneously):

```toml
music_dir    = "/path/to/Music"
audio_output = "airplay"

[[airplay_receivers]]
host = "192.168.1.50"   # living room
port = 5000             # optional, default 5000

[[airplay_receivers]]
host = "192.168.1.51"   # bedroom
# port defaults to 5000
```

Streams ALAC-encoded audio over RTP to any RAOP-compatible receiver — Apple
TV, HomePod, Airport Express, or
[shairport-sync](https://github.com/mikebrady/shairport-sync). All receivers
share the same `initial_rtptime`, so RTP-level playback synchronisation is
within one frame (~8 ms) across the LAN.

### Squeezelite (Slim Protocol — multi-room)

```toml
music_dir             = "/path/to/Music"
audio_output          = "squeezelite"
squeezelite_port      = 3483   # Slim Protocol TCP port, default 3483
squeezelite_http_port = 9999   # HTTP PCM broadcast port, default 9999
```

Rockbox acts as a minimal Logitech Media Server. Any number of
[squeezelite](https://github.com/ralph-irving/squeezelite) clients can connect
simultaneously; Rockbox sends a `sync` packet to every client once per second
so they all align to the same playback clock:

```sh
squeezelite -s localhost -n "Living Room"
squeezelite -s localhost -n "Kitchen"
squeezelite -s localhost -n "Bedroom"
```

Select a specific output device:

```sh
squeezelite -s localhost -l              # list available devices
squeezelite -s localhost -o ""           # system default
squeezelite -s localhost -o "Built-in Output"
```

### Chromecast

```toml
music_dir            = "/path/to/Music"
audio_output         = "chromecast"
chromecast_host      = "192.168.1.60"  # LAN IP of the target Chromecast
chromecast_port      = 8009            # optional, default 8009 (Cast protocol)
chromecast_http_port = 7881            # optional, default 7881 (WAV HTTP stream)
```

Rockbox streams audio to any Google Cast-compatible device — Google Home,
Chromecast Audio, Chromecast with Google TV, Nest Hub, or third-party Cast
receivers. It uses two channels simultaneously:

- **Cast protocol** (TCP 8009, TLS + Protobuf) — sends playback commands and
  tells the device where to fetch the audio stream.
- **WAV over HTTP** (port 7881) — serves a live `audio/wav` stream with a
  finite `Content-Length` so the Chromecast can show a progress bar and
  auto-advance the queue at track boundaries.

Track metadata (title, artist, album, duration) and album art are pushed to the
device on every track change. Chromecast devices on the LAN are also discovered
automatically via mDNS (`_googlecast._tcp.local.`) and appear in the UI device
picker; connecting through the picker starts the Cast session on demand without
requiring `audio_output = "chromecast"` in the config file.

> **Network requirement**: the Chromecast device must be able to reach port 7881
> on the machine running rockboxd. If rockboxd is inside a VM or container,
> forward that port to the host.

See [`crates/chromecast/README.md`](crates/chromecast/README.md) for a detailed
description of the architecture, protocols, and FFI surface.

### UPnP / DLNA

Rockbox has three independent UPnP/DLNA modes that can be combined freely.

#### PCM sink — stream live audio to a UPnP renderer (Kodi, VLC, …)

```toml
music_dir          = "/path/to/Music"
audio_output       = "upnp"

# AVTransport controlURL of the target renderer (required for metadata push)
upnp_renderer_url  = "http://192.168.1.x:7777/AVTransport/control"

# Port for the WAV HTTP broadcast server (default: 7879)
upnp_http_port     = 7879
```

Rockbox encodes live PCM as a continuous WAV-over-HTTP stream and commands the
renderer to play it via AVTransport SOAP. Track metadata (title, artist, album,
album art, duration) is sent as DIDL-Lite XML in `SetAVTransportURI` and
auto-refreshed on every track change so the renderer's "Now Playing" display
stays accurate.

> **Finding `upnp_renderer_url`**: start `rockboxd` with `RUST_LOG=info` — it
> scans the LAN on startup and logs `upnp scan: found renderer "…" av=http://…`
> for every discovered renderer.

#### Media Server — expose library to control points (BubbleUPnP, Kodi, …)

```toml
upnp_server_enabled = true
upnp_server_port    = 7878        # default
upnp_friendly_name  = "Rockbox"  # name shown in apps
```

Starts a ContentDirectory service so control points can browse artists, albums,
and tracks and pull audio directly from Rockbox.

#### MediaRenderer — let control points push media to Rockbox

```toml
upnp_renderer_enabled = true
upnp_renderer_port    = 7880        # default
upnp_friendly_name    = "Rockbox"
```

Rockbox registers as a `MediaRenderer:1`. Any DLNA control point (BubbleUPnP,
Foobar2000, etc.) can push a URI to Rockbox and control playback remotely.
Incoming DIDL-Lite metadata (title, artist, album, album art, duration) is
parsed and displayed.

#### All UPnP settings

| Key                     | Default     | Description                                   |
| ----------------------- | ----------- | --------------------------------------------- |
| `audio_output = "upnp"` | —           | Enable the PCM → WAV streaming sink           |
| `upnp_renderer_url`     | —           | AVTransport controlURL of the target renderer |
| `upnp_http_port`        | `7879`      | WAV broadcast HTTP port                       |
| `upnp_server_enabled`   | `false`     | Start the ContentDirectory media server       |
| `upnp_server_port`      | `7878`      | Media server HTTP port                        |
| `upnp_renderer_enabled` | `false`     | Start the MediaRenderer endpoint              |
| `upnp_renderer_port`    | `7880`      | MediaRenderer HTTP port                       |
| `upnp_friendly_name`    | `"Rockbox"` | Display name shown to control points          |

---

## 🚚 Installation

### Ubuntu / Debian

```sh
echo "deb [trusted=yes] https://apt.fury.io/tsiry/ /" | sudo tee /etc/apt/sources.list.d/fury.list
sudo apt-get update
sudo apt-get install rockbox
```

### Fedora

Add the following to `/etc/yum.repos.d/fury.repo`:

```ini
[fury]
name=Gemfury Private Repo
baseurl=https://yum.fury.io/tsiry/
enabled=1
gpgcheck=0
```

Then run:

```sh
dnf install rockbox
```

### Arch Linux

```sh
paru -S rockboxd-bin
```

### macOS/Linux (Homebrew)

```sh
brew install tsirysndr/tap/rockbox
```

### Universal (curl installer)

```sh
curl -fsSL https://raw.githubusercontent.com/tsirysndr/rockboxd/HEAD/install.sh | bash
```

### Nix (flake)

With [Determinate Nix](https://determinate.systems/nix) (flakes enabled) you
can run or install rockboxd straight from the repo:

```sh
# Run the daemon without installing
nix run "git+https://github.com/tsirysndr/rockboxd?submodules=1"

# Install into your profile (provides the `rockboxd` binary)
nix profile install "git+https://github.com/tsirysndr/rockboxd?submodules=1"
```

> **Use the `git+https://` form, not `github:`.** rockboxd vendors the `deno`
> and `rmpc` crates as git submodules. The `github:` fetcher uses GitHub's
> source tarball, which omits submodule contents (and silently ignores
> `?submodules=1`), so the build fails with `failed to read
> deno/cli/Cargo.toml`. The `git+https://…?submodules=1` fetcher does a real
> clone that pulls the submodules. (Quote the ref so your shell doesn't eat
> the `?`.)

**⚡ Speed up the build with Cachix.** A from-source build links the Rockbox C
firmware, every codec, and — for the `#rockbox` CLI — a full Deno/V8 stack, so
it is heavy. Enable the public **`rockbox`** binary cache to pull pre-built
artifacts instead of compiling locally:

```sh
nix profile install nixpkgs#cachix   # if you don't already have cachix
cachix use rockbox
```

Other flake outputs:

```sh
nix run    "git+https://github.com/tsirysndr/rockboxd?submodules=1#rockbox"  # the `rockbox` CLI client
nix build  "git+https://github.com/tsirysndr/rockboxd?submodules=1"          # → ./result/bin/rockboxd
nix develop "git+https://github.com/tsirysndr/rockboxd?submodules=1"         # dev shell: Zig, Rust, typesense…
```

---

## 📦 Downloads

Pre-built binaries for the latest release are available on the
[Releases page](https://github.com/tsirysndr/rockboxd/releases/latest).

| Platform | Architecture            | Package   |
| -------- | ----------------------- | --------- |
| Linux    | x86_64                  | `.tar.gz` |
| Linux    | aarch64                 | `.tar.gz` |
| macOS    | x86_64                  | `.pkg`    |
| macOS    | aarch64 (Apple Silicon) | `.pkg`    |

---

## 🧙‍♂️ Systemd Service

```sh
rockbox service install    # enable and start
rockbox service uninstall  # stop and disable
rockbox service status     # check status
```

![Systemd service screenshot](https://github.com/user-attachments/assets/1fbd2b58-0e29-4db4-9791-6e377de72728)

---

## 🏗️ Compiling from Source

### Dependencies

**Ubuntu / Debian**

```sh
sudo apt-get install libsdl2-dev libfreetype6-dev libdbus-1-dev libunwind-dev zip protobuf-compiler cmake libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

**Fedora**

```sh
sudo dnf install SDL2-devel freetype-devel libunwind-devel zip protobuf-compiler cmake libxkbcommon-devel libxkbcommon-x11-devel libxcb-devel
```

**macOS**

```sh
brew install sdl2 freetype cmake protobuf
```

You also need [Zig](https://ziglang.org/download/) ≥ 0.16 and a recent stable
Rust toolchain (`rustup update stable`).

### Build

```sh
# 1. Clone
git clone https://github.com/tsirysndr/rockboxd.git
cd rockboxd
git submodule update --init --recursive

# 2. Build the web UI
cd webui/rockbox
deno install
deno run build
cd ../..

# 3. Configure and build the C firmware (one-time setup)
mkdir -p build-lib && cd build-lib
../tools/configure --target=sdlapp --type=N --lcdwidth=320 --lcdheight=240 --prefix=/usr/local
cp ../autoconf/autoconf.h .
make lib
cd ..

# 4. Build Rust crates
cargo build --release -p rockbox-cli -p rockbox-server

# 5. Link everything with Zig
cd zig && zig build
```

The binary is at `zig/zig-out/bin/rockboxd`.

> **Rebuilding after changes**: after editing C code run `make lib` in
> `build-lib`; after editing Rust run `cargo build --release`. Then re-run
> `zig build`. Zig only re-links when the `.a` files are newer than the binary.

### 🎛️ Console — one entry point for every build/dev/ops command

Rather than remembering the exact `make`, `cargo`, `zig`, `bun`, and
`bash scripts/*.sh` incantations, the repo ships a REPL-driven console
([babashka](https://babashka.org) / Clojure) that wraps them all. The
`./console` launcher at the repo root forwards to it — no need to `cd` in:

```sh
./console                 # command tour (all available commands)
./console build:all       # firmware → crates → zig → rockboxd
./console run:debug       # RUST_LOG=debug rockboxd
./console wasm:build      # bash scripts/build-wasm.sh
./console verify:stale    # check the stale-binary pitfall
./console repl            # rich terminal REPL (clj -M:rebel)
./console nrepl           # nREPL for your editor (CIDER / Calva)
```

Any other argument is passed straight through to the underlying `bb <task>`.
Tool versions (Java / Clojure / babashka) are pinned in
[`tools/console/.mise.toml`](tools/console/.mise.toml); with
[mise](https://mise.jdx.dev) installed the launcher resolves them
automatically, otherwise install `bb` + `clj` yourself. See
[`tools/console/README.md`](tools/console/README.md) for the full command
reference and how to add your own commands.

### Build the GTK4 desktop app

```sh
sudo apt-get install flatpak
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.flatpak.Builder
flatpak install --user flathub org.gnome.Sdk/x86_64/47
flatpak install --user flathub org.gnome.Platform/x86_64/47
flatpak install --user org.freedesktop.Sdk.Extension.rust-stable
flatpak install --user org.freedesktop.Sdk.Extension.llvm18
cd gtk
flatpak run org.flatpak.Builder --user --disable-rofiles-fuse --repo=repo flatpak_app build-aux/io.github.tsirysndr.Rockbox.json --force-clean
flatpak run org.flatpak.Builder --run flatpak_app build-aux/io.github.tsirysndr.Rockbox.json rockbox-gtk
```

---

## 🧑‍🔬 Architecture

![Architecture diagram](./docs/rockbox-arch.png)

The Rockbox C firmware (audio engine, codecs, DSP) is compiled into
`libfirmware.a` and linked with two Rust static libraries
(`librockbox_cli.a`, `librockbox_server.a`) and [CPAL](https://github.com/RustAudio/cpal) by the Zig build script.
The result is a single `rockboxd` binary. Rust crates expose the firmware over
gRPC, GraphQL, HTTP, and MPD, and implement output sinks (AirPlay, Squeezelite,
Snapcast) and the Typesense search integration.

---

## 📚 APIs

### GraphQL

Open [http://localhost:6062/graphiql](http://localhost:6062/graphiql) in your browser.

<p style="margin-top: 20px; margin-bottom: 20px;">
 <img src="./docs/graphql.png" width="100%" />
</p>

### HTTP REST

Open [http://localhost:6063](http://localhost:6063) in your browser.

<p style="margin-top: 20px; margin-bottom: 20px;">
 <img src="./docs/http-api.png" width="100%" />
</p>

### gRPC

Docs: [buf.build/tsiry/rockboxapis](https://buf.build/tsiry/rockboxapis/docs/main:rockbox.v1alpha1)

Try it live with
[Buf Studio](https://buf.build/studio/tsiry/rockboxapis/rockbox.v1alpha1.LibraryService/GetAlbums?target=http%3A%2F%2Flocalhost%3A6061&selectedProtocol=grpc-web).

<p style="margin-top: 20px; margin-bottom: 20px;">
 <img src="./docs/grpc.png" width="100%" />
</p>

### S3-compatible API

Upload and delete audio files in `music_dir` using any AWS S3 client
(`awscli`, MinIO Client `mc`, `rclone`, AWS SDKs). Authenticated with AWS
Signature V4. The library DB stays in sync automatically — every PUT
triggers an add, every DELETE triggers a remove.

Enable in `~/.config/rockbox.org/settings.toml`:

```toml
s3_enabled = true
s3_port = 9000                      # optional, default 9000
s3_host = "0.0.0.0"                 # optional, default "0.0.0.0"
s3_access_key = "your-access-key"
s3_secret_key = "your-secret-key"
```

Region is fixed to `us-east-1` and the bucket name is fixed to `music`.

```sh
export AWS_ACCESS_KEY_ID="your-access-key"
export AWS_SECRET_ACCESS_KEY="your-secret-key"
export AWS_DEFAULT_REGION="us-east-1"
# Required for awscli v2.23+ — disables on-by-default integrity headers
# that confuse non-AWS endpoints.
export AWS_REQUEST_CHECKSUM_CALCULATION=when_required
export AWS_RESPONSE_CHECKSUM_VALIDATION=when_required

alias rbs3='aws --endpoint-url http://localhost:9000'

# Upload (single file or recursive)
rbs3 s3 cp song.flac s3://music/song.flac
rbs3 s3 sync ~/Staging s3://music/ --exclude "*" --include "*.flac" --include "*.mp3"

# List
rbs3 s3 ls s3://music/
rbs3 s3api list-objects-v2 --bucket music --prefix "Albums/"

# Delete
rbs3 s3 rm s3://music/song.flac
```

Only audio extensions are accepted on upload: `mp3, ogg, flac, m4a, aac,
mp4, alac, wav, wv, mpc, aiff, aif, ac3, opus, spx, sid, ape, wma`.

Single-shot uploads only — multipart upload and `STREAMING-AWS4-HMAC-SHA256-PAYLOAD`
are not yet implemented (cap per PUT: 2 GiB).

---

## 📖 Documentation

Full guides, configuration reference, audio-output setup, API reference, and
SDK docs are published at:

**[👉 View full documentation](https://rockboxd.tsiry-sandratraina.com)**

The Mintlify source lives in [`mintlify/`](./mintlify/). Topics covered:

- **Getting started** — install, quickstart, configuration
- **Audio output** — built-in [CPAL](https://github.com/RustAudio/cpal), [Snapcast](https://github.com/snapcast/snapcast), [AirPlay](https://en.wikipedia.org/wiki/AirPlay), [Squeezelite](https://github.com/ralph-irving/squeezelite), [Chromecast](https://developers.google.com/cast), [UPnP](https://en.wikipedia.org/wiki/Universal_Plug_and_Play)
- **Audio settings** — parametric EQ, DSP, ReplayGain, crossfade
- **Clients** — web UI, desktop apps, MPD, MPRIS
- **API reference** — HTTP REST (auto-generated from OpenAPI), GraphQL, gRPC, MPD
- **SDKs** — TypeScript, Python, Ruby, Elixir, Clojure, Gleam
- **Architecture** — build system, PCM sinks, cross-cutting concerns
- **Reference** — `rockbox` and `rockboxd` CLI, ports, `settings.toml`, troubleshooting, FAQ
