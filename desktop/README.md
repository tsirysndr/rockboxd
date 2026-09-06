# rockbox-desktop — Slint client for rockboxd

A modern, skinnable desktop app for [rockboxd](../README.md), built with
[Slint](https://slint.dev). Inspired by jetAudio (VFD readout), Mixxx (LED
level meters, LateNight palette), Cambridge Audio (lunar-grey hi-fi restraint),
FL Studio / VST synth UIs (neon accents).

## Features

- **Embedded daemon** — if nothing is listening on `127.0.0.1:6061`, the app
  boots a full in-process rockboxd via `librockboxd.a` (same archive the GPUI
  app links). If a daemon is already running it just connects.
- **Library browser** — Albums grid (with cover art), Artists, Tracks, Liked;
  album detail page (art, year, label, track count, total duration, per-track
  play, shuffle).
- **Transport** — play/pause, prev/next, seek, shuffle toggle, repeat cycle
  (off → all → one), and a rotary volume knob with percent readout, all
  synced with the daemon over gRPC streams.
- **VSCode-style panel toggles** — two buttons at the top right (after
  search) show/hide the left navigation sidebar and the right queue panel;
  the icon is filled while its panel is visible.
- **Queue drawer** — Play Queue / History tabs, pinned Now Playing card, Up
  Next list; click any row to jump (`PlaylistService.Start`).
- **Command palette** — press `/` (or click the search box) for a
  Raycast-style overlay searching tracks, albums and artists; `↑`/`↓` +
  `enter` to play. `?` shows the keyboard-shortcut help.
- **Audio settings** — press `e` (or the sliders icon in the player bar):
  10-band equalizer (enable switch, per-band vertical sliders, precut knob),
  Mixxx-style rotary knobs for bass / treble / balance / replaygain pre-amp /
  crossfade fade timings (drag vertically, scroll to nudge, double-click to
  reset), dropdowns for replaygain and crossfade modes, dithering toggle.
  Changes are coalesced and applied live via `SettingsService.SaveSettings`.
- **VFD display** — jetAudio-style readout with elapsed time, codec,
  bitrate, sample rate and animated LED VU meters.
- **Server switcher** — click the address at the bottom of the sidebar for a
  Raycast-style list: the embedded daemon, LAN rockboxd peers (mDNS,
  `_rockbox._tcp`), saved Subsonic/Jellyfin servers, or type `host:port` to
  connect to any remote rockboxd; switching re-points every gRPC stream live.
- **Playlists** — create (name + description), edit, delete; add tracks via a
  Raycast-style picker; per-track remove; playlists appear in the `/` palette.
- **Remote servers** — Subsonic/Navidrome and Jellyfin browsing/streaming via
  the daemon's `navidrome://` / `jellyfin://` browse schemes (Servers tab).
- **Skins** — five bundled (`Synthwave` default, `Late Night`, `Neutron`,
  `Lunar`, `Porcelain`); press `s` or click the SKIN entry in the sidebar to cycle. The
  choice persists in `~/.config/rockbox.org/desktop-skin`.

## Build

```sh
# Optional but recommended — embedded daemon support:
cd build-headless && make lib && cd ..
cargo build --release -p rockbox-embed -p rockbox-server
cd zig && zig build lib && cd ..          # → zig/zig-out/lib/librockboxd.a

# The app itself:
cd desktop && cargo build --release       # → target/release/rockbox-desktop
```

If `librockboxd.a` is absent the build still succeeds as a **remote-only
client** (build.rs prints a warning); it then requires an externally started
`rockboxd`.

Environment overrides: `ROCKBOX_HOST`, `ROCKBOX_GRPC_PORT` (6061),
`ROCKBOX_GRAPHQL_PORT` (6062, used for `/covers/` album art).

## Skins

A skin is a TOML file of design tokens — colors, radii, font families — that
Rust loads into the Slint `Theme` global at runtime (`src/skin.rs`). Drop
extra `.toml` files into `~/.config/rockbox.org/skins/` and they join the
cycle; copy `skins/synthwave.toml` as a template. Malformed color values
render loud magenta so they're easy to spot.

| File                   | Vibe                                                  |
| ---------------------- | ----------------------------------------------------- |
| `skins/synthwave.toml` | Neon magenta/cyan on deep violet (Serum / synthwave)  |
| `skins/late-night.toml`| Mixxx LateNight PaleMoon — teal VFD, burnt amber      |
| `skins/neutron.toml`   | FL Studio graphite + signal orange, lime channel glow |
| `skins/lunar.toml`     | Cambridge Audio lunar grey + warm lamp amber          |
| `skins/porcelain.toml` | Light jetAudio silver deck, cool blue display         |

## Architecture

```
ui/app.slint         window, sidebar, views, player bar, overlays (palette,
                     help, queue drawer, album detail)
ui/components.slint  IconButton, PlayPauseButton, SlideBar, MeterStrip,
                     VfdDisplay, AlbumCard, TrackRow/TrackListHeader, …
ui/theme.slint       Theme global — every visual token, overwritten per skin
ui/icons.slint       Icons global — Lucide-style SVGs shared with gpui/assets
src/main.rs          UI-thread state (thread_local), callbacks, skin cycling
src/rpc.rs           tokio worker: tonic clients, StreamCurrentTrack /
                     StreamStatus / StreamPlaylist followers, command loop
src/skin.rs          skin TOML loading + Theme application + persistence
src/daemon.rs        embedded rockboxd boot (mirrors gpui/src/startup.rs)
build.rs             proto codegen (client-only), slint compile, conditional
                     librockboxd.a link (cfg: embedded_daemon)
```

Threading model: the tokio worker owns all gRPC I/O and pushes plain data to
the UI thread with `Weak::upgrade_in_event_loop`; library state lives in a
`thread_local` on the UI thread. UI callbacks send `Cmd` values over an
unbounded channel back to the worker.

Icon rule: **no emoji / Unicode glyphs as icons** — only the SVGs referenced
by `ui/icons.slint` (tinted via `Image.colorize`, so they follow every skin).
