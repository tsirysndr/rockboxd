# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [2026.07.28]

### Added
- `wasm`: lightweight single-threaded **browser core** — the WASM target now compiles only the extracted `rockbox-codecs` (decode), `rockbox-dsp` (EQ/DSP), and `rockbox-metadata` (tags) crates through the `rockbox-ffi` flat C ABI, shipped as the **`rockbox-wasm`** npm package (`bindings/wasm/`); there is no firmware, no servers, and no pthreads — decoding is fully synchronous so the page needs no COOP/COEP or `SharedArrayBuffer` — and it ships with a React example featuring a live queue UI, insertion modes, `.m3u8` playlists, Tabler icons, and jotai-persisted settings
- `wasm`: **live / infinite radio** streaming with frame-aligned segment decode and an MP3-reservoir primer for gapless segments, plus **ICY metadata** parsing; **gapless prefetch** of the next whole-file track (re-prefetched on queue insert), **progressive M4A/AAC** playback via ADTS re-framing, a JS-side Rockbox `pcmbuf` **crossfade** port with a settings panel, and the full **DSP surface** (parametric EQ + crossfeed / PBE) exposed to JS
- `playback`: **queue `remove(index)` and `clear_queue`** APIs, surfaced through the FFI and every binding
- `rocksky`: the daemon's remote-control WebSocket client now advertises a configurable **device name** from `settings.toml` (`device_name`, falling back to `player_name`, then `"Rockbox"`) so the Rocksky miniplayers label this player with a user-chosen name; presence announcements (`device_registered` / `device_unregistered` / `primary_changed`) about other devices are ignored
- `bindings`: prebuilt **Android native libraries** are now built and bundled for the Kotlin binding in CI, and the codec `Decoder` API + HTTP(S) stream playback are documented across every binding README (Python, TypeScript, Swift, Go, Kotlin, Gleam, Elixir, Clojure, Ruby, Erlang)

### Fixed
- `rocksky`: **resume restored playback** when the audio engine is stopped on the first play from a miniplayer — a fresh daemon start leaves the engine STOPPED (not paused), so a plain `resume()` was a no-op; the client now queries engine status and calls `PlaylistService.resume_track()` (restore the playlist from the control file + seek to the saved position) when stopped, mirroring the GPUI play/pause handler
- `rocksky`: don't clobber this device's own id from `device_registered` broadcasts about *other* devices — capturing another device's id would tag our now-playing pushes with the wrong id and make every miniplayer mislabel the source
- `rockbox-codecs`: route raw ADTS / HE-AAC `"aac"` streams (e.g. `audio/aacp` radio) to `aac_bsf` instead of the MP4-container aac codec, and guard an `aac_bsf` divide-by-zero on container-less ADTS
- `rockbox-playback`: send a `User-Agent` header on HTTP streams
- `wasm`: a run of playback-stability fixes — audio no longer dies at ~1 s (frames were counted after the buffer transfer detached them), pause/stop lag (control messages were posted to the wrong port), a boot deadlock that disabled the Play button forever, and worker-main-thread blocking that crashed live streams

## [2026.07.12]

### Added
- `bindings`: multi-language FFI bindings for **9 languages** — Python, TypeScript, Elixir, Gleam, Ruby, Swift, Kotlin, Clojure, and Go — all layered on a single shared `rockbox-ffi` C ABI (`cdylib` + `staticlib`); Python via cffi, TS via Bun/Deno/Node koffi, Ruby via fiddle, Go via purego, Elixir + Gleam through a shared `erl_nif` NIF, Swift/Kotlin/Clojure via their native FFIs; each ships an interactive console (Python IPython, Ruby IRB) and a playback example, and queue-insertion / resume / m3u8 are exposed uniformly across all nine
- `bindings`: per-platform prebuilt native libraries bundled into each package with a dedicated `bindings-v*` release CI pipeline that uploads artifacts to the GitHub Release; Swift gains a static-linked product plus a macOS/iOS `xcframework`, Kotlin publishes to Maven Central and Clojure to Clojars (`clojure-ffi-v*` tag), and Python wheels are tagged `py3-none-<platform>` per-arch
- `dsp`: new standalone **`rockbox-dsp`** crate (v0.2.0) — the Rockbox DSP chain (parametric EQ, tone controls, surround, channel mode, compressor, and replaygain) extracted as a reusable Rust library that compiles `lib/rbcodec/dsp` via `cc`; honours `settings.toml` EQ values (already in tenths) with local-settings precedence, and builds standalone from vendored C sources
- `metadata`: new standalone **`rockbox-metadata`** crate — Rockbox tag/metadata extraction exposed as a library (phase 1 of the rbcodec extraction), with a flat `rbmeta_tags` bridge over the firmware parsers
- `codecs`: new standalone **`rockbox-codecs`** crate (v0.1.1) — Rockbox decoders as a Rust library (phase 2), using a warble-style shim and `-D` symbol renames; HE-AAC (SBR + PS) decoding included
- `playback`: Rockbox-style **queue insertion** API (insert / insert-next / insert-last), **resume** with playback-position restore, and first-class **`.m3u8` playlist** handling — all surfaced through the FFI and bindings
- `playback`: **HTTP(S) remote media** playback via a `MediaSource` abstraction (byte-range requests + MIME sniffing), **lazy range-buffered** remote files, **live-radio** streaming, and **ICY metadata** parsing (`StreamTitle`, station name, bitrate)
- `player`: the full **Rockbox DSP chain + EQ presets** (e.g. BassBoost), **crossfeed / PBE / AFR** and tone cutoffs, and **shuffle / repeat** modes are now driveable through the high-level player API, with builder and pipe/fluent DX ergonomics
- `build`/`ci`: **FreeBSD / NetBSD / OpenBSD** support — headless-host firmware build, a direct `libasound` sink on FreeBSD/NetBSD and a new `libsndio` sink on OpenBSD, `statvfs`-based filesystem code, FTS5 search on the BSDs, and a dedicated `bsd-release` CI job that builds `rockboxd` in FreeBSD/NetBSD VMs via `cross-platform-actions`
- `nix`: Nix flake packaging — a hermetic `.#rockbox` build with bundled Typesense and prefetched V8, Rust staticlibs split into a separately-cached derivation, FlakeHub publishing, a Cachix binary cache, and a `nix-consume` smoke-test workflow; `nix run` starts the daemon
- `tools`: `tools/console` — a Clojure/babashka REPL (with a `./console` launcher) that centralizes the monorepo's build/dev/ops commands
- `cli`: FreeBSD/NetBSD `rc.d` service examples plus a service README

### Fixed
- `ci`: macOS gpui and embeddable-library builds no longer fail intermittently at link time with `Undefined symbols for architecture arm64: _rb_daemon_start` — the Cargo/Zig caches in `release-gpui`, `macos-build`, and `release-embed-lib` are now keyed on `${{ matrix.arch }}` so a same-OS/different-arch runner (or a drifting `*-latest` label that silently switches CPU arch) can never restore an incompatible `target/` or `zig/.zig-cache`; the embed step also force-removes `librockbox_embed.a` before `cargo build` and asserts `rb_daemon_start` is present in both `librockbox_embed.a` and the flattened `librockboxd.a` via `nm`, so a stale or mislinked archive aborts the job at its source instead of deep inside the rust-lld output
- `playback`: fixed a shutdown deadlock exposed by the new queue-insertion path
- firmware: synced with upstream Rockbox master — brings the Sansa Clip Zip LCD flip / display-init fixes, RTL on-screen-keyboard corrections, numerous `FS#` bug fixes, and a large sweep of `rbutil` / tools build-warning cleanups

## [2026.07.02]

### Added
- `jellyfin`: Playlists API — full CRUD from the OpenAPI spec so third-party clients (Findroid, Streamyfin, Moonfin, Amcfy) can create, edit, and delete playlists through the standard surface without falling back to the Subsonic bridge; `POST /Playlists` returns `PlaylistCreationResult`, `GET/POST /Playlists/{id}`, `GET/POST/DELETE /Playlists/{id}/Items`, `POST /Playlists/{id}/Items/{itemId}/Move/{newIndex}`, `GET /Playlists/{id}/Users`, and `DELETE /Items/{id}` (which only accepts playlists — tracks/albums/artists return 403); backed by the existing `rockbox-playlists::PlaylistStore` so the Subsonic and Jellyfin surfaces share state; a new virtual "Playlists" `CollectionFolder` shows up in `/Users/{uid}/Views` alongside Music so clients render a top-level tile; `PlaylistItemId` values are synthesized deterministically from `(playlist_id, position)` so entry-id round-trips through remove/move without a mapping table
- `jellyfin`: Favorites API — new `jf_favorites (kind, native_id, favorited_at)` migration covers tracks, albums, artists, and playlists; writes for tracks/albums mirror to `rockbox-library`'s existing `favourites` table so smart-playlist `is_liked` rules and the Subsonic bridge stay in sync, and reads take the union so likes added elsewhere still surface here; `POST/DELETE /UserFavoriteItems/{id}` (10.9+) and legacy `POST/DELETE /Users/{uid}/FavoriteItems/{id}` both return the freshly-updated `UserItemDataDto`; `?IsFavorite=true` and `?Filters=IsFavorite` on `/Items` route through a `list_favorites` orchestrator that honours `IncludeItemTypes` across all four kinds
- `jellyfin`: UserData API — new `jf_user_data (kind, native_id, played, play_count, playback_position_ticks, last_played_at, likes, rating, updated_at)` composite-PK cache backs the spec's per-item user-data fields; for tracks the store merges with `rockbox-playlists::track_stats` on read (whichever `play_count` is higher wins, engine's `last_played` fills a null) so audio-engine counters surface on the Jellyfin side without a sync step; `GET /UserItems/{id}/UserData` (10.9+) and legacy `GET /Users/{uid}/Items/{id}/UserData` roll up `IsFavorite` + playback + rating + likes; the matching `POST` accepts a partial `UpdateUserItemDataDto` where unset fields preserve stored state per spec, and `IsFavorite` is forwarded to the favorites store so both surfaces stay coherent; all four `*_to_dto` helpers now emit real user-data fields instead of hardcoded zeros
- `jellyfin`: InstantMix API — seed-based mix generator covers `GET /Items/{id}/InstantMix` (generic dispatcher) plus the legacy `/Songs`, `/Albums`, `/Artists`, `/Artists/InstantMix?id=`, `/Playlists`, and `/MusicGenres/{name}` per-kind aliases; algorithm anchors the seed (or its own tracks), fills with same-`artist_id` matches, then same-`genre_id`, then a random tail from the whole library; track seeds keep position 0 anchored through the shuffle so "play me first" behaviour is preserved; results dedup by native id and truncate to `Limit` (default 50)
- `jellyfin`: Lyrics API — reads a `.lrc` (synced) or `.txt` (plain) sidecar next to the audio file; `GET /Audio/{id}/Lyrics` returns a `LyricDto` (with `LyricLine.Start` in 100-ns ticks and a `LyricMetadata` block populated from `ar/al/ti/by/offset/length/re/ve/au` header tags), `POST` accepts raw LRC / plain text OR a JSON `LyricDto` body (distinguished by `Content-Type` — JSON re-serializes as LRC so external players can read the file), `DELETE` removes sidecars idempotently, `GET /Audio/{id}/RemoteSearch/Lyrics` and `GET /Providers/Lyrics` return empty arrays since no remote providers ship; LRC parser handles header tags, multi-timestamp expansion, and offset application; unit tests cover synced/multi-timestamp/plain parsing
- `jellyfin`: Similar API backed by Last.fm and MusicBrainz plugins — `GET /Items/{id}/Similar` plus legacy `/Artists/{id}/Similar` and `/Albums/{id}/Similar` route through a plugin orchestrator that seeds `artist.getsimilar` / `track.getsimilar` from Last.fm and canonicalizes MBIDs through MusicBrainz before matching results back to the local library; artist seed → local artist by exact/case-insensitive name; album seed → same-artist expansion (Last.fm has no album endpoint) → their local albums; track seed → local `(title, artist)` lookup with the returned artist MBID cross-referenced through MB; plugins are gated on `lastfm_api_key` / `musicbrainz_user_agent` — no key means no fetches, and `SimilarResult` short-circuits to empty rather than falling back to random suggestions; both plugin activations log at startup
- `jellyfin`: RemoteImage API backed by Cover Art Archive — `GET /Items/{id}/RemoteImages` returns candidate cover art (honours `type`, `startIndex`, `limit`), `GET /Items/{id}/RemoteImages/Providers` lists active providers, and `POST /Items/{id}/RemoteImages/Download?imageUrl=&type=` fetches the URL, saves under `~/.config/rockbox.org/covers/` using the same `md5(album_id)` filename scheme the audio scanner uses, and updates `album.album_art` in one shot; MusicBrainz search resolves `(artist, album)` to an MBID via a Lucene-escaped `release-group` query; CAA client handles both historical (`small`/`large`) and size-suffixed (`250`/`500`/`1200`) thumbnail shapes; track seeds fall back to their parent album so the "change cover art" action from a now-playing view works; gated on the same `musicbrainz_user_agent` as the Similar plugin
- `jellyfin`: Genre Browsing API — new `KIND_GENRE` + `remember_genre` so genres get stable GUIDs; `genre_to_dto` populates `SongCount` / `AlbumCount` for chip UIs; `GET /Genres` and `/MusicGenres` (sorted list, honours `searchTerm` / `nameStartsWith` / range / pagination); `GET /Genres/{name}`, `/MusicGenres/{name}`, and legacy `/Users/{uid}/Genres/{name}` with case-insensitive fallback; `items_impl` gained a `parentId=<genre_guid>` branch (dispatches on `IncludeItemTypes` to return tracks / albums / artists under the genre) and an `IncludeItemTypes=MusicGenre` shorthand for the flat list
- `jellyfin`: Filters API — `GET /Items/Filters` returns `QueryFiltersLegacy` (flat genre name list), `GET /Items/Filters2` returns `QueryFilters` (`NameGuidPair` list so chips round-trip through `?genreIds=<guid>` without a name→guid lookup), plus the pre-10.9 `GET /Users/{uid}/Items/Filters` alias; years come from distinct non-zero `album.year`; tags and official ratings stay as empty arrays since rockbox doesn't track them
- `jellyfin`: Item Counts endpoint — `GET /Items/Counts` returns `ItemCounts` with real `SongCount` / `AlbumCount` / `ArtistCount` from `repo::*::count_filtered`; unsupported kinds (movies, series, episodes, trailers, boxsets, books) stay at 0 per spec; `ItemCount` sums the three we surface
- `jellyfin`: Last.fm artist enrichment — new `jf_artist_enrichment (artist_id PK, mbid, bio, tags, image_url, fetched_at)` cache backs `Overview` and `Genres` on `MusicArtist` DTOs; `LastFm::artist_info` calls `artist.getInfo`, cleans the trailing "Read more on Last.fm" link, extracts tags and the largest image URL; `enrichment::get_artist` is a cache-only read used by every `artist_to_dto` (SQLite lookup — list requests stay fast), `enrichment::refresh_artist` enforces a 7-day TTL and upserts on demand from the detail handlers; serves stale rows on network errors so a blip doesn't blank the bio
- `jellyfin`: Last.fm album enrichment — mirrors the artist flow with a new `jf_album_enrichment (album_id PK, mbid, description, tags, image_url, fetched_at)` cache; `LastFm::album_info` calls `album.getInfo` with MBID priority over `(artist, album)`, parses the wiki summary/content; `album_to_dto` reads the cache on every call so list paths stay fast; the album detail branches in `item_by_id` / `user_item_by_id` call `refresh_album` first so the returned DTO carries a hot description
- `jellyfin`: home-rail routes replace the earlier `empty_items` stubs so Findroid / Streamyfin / official web actually populate their home screens — `/Items/Resume` + `/Users/{uid}/Items/Resume` return tracks with non-zero `PlaybackPositionTicks` ordered by `updated_at DESC`, `/UserItems/Resume` and `/UserItems/Latest` cover the legacy plain-array shape Findroid still uses, `/Items/Suggestions` + `/Users/{uid}/Items/Suggestions` return random tracks by default (honours `IncludeItemTypes=MusicAlbum` / `MusicArtist` and `mediaType=Audio`) via SQL `RANDOM()` so the rail refreshes each session; the generic `/Items` handler now recognizes `sortBy=PlayCount` and `sortBy=DatePlayed` (plus `DateLastPlayed`) with `sortOrder=Descending|Ascending`, joining `track` LEFT JOIN `track_stats` so the "Most Played" and "Recently Played" home rails work through the standard `/Items` endpoint without dedicated handlers
- `settings`: new optional `lastfm_api_key: Option<String>` and `musicbrainz_user_agent: Option<String>` fields on `NewGlobalSettings` — the Jellyfin Similar / RemoteImage / enrichment plugins short-circuit to empty when their respective field is absent, so the plugins only activate when the corresponding credentials are present in `settings.toml`

## [2026.06.29]

### Added
- `jellyfin`: Jellyfin-compatible HTTP API server on its own port — new `crates/jellyfin/src/server/` module gated behind the `server` cargo feature, spawned alongside Navidrome from `crates/server/src/lib.rs`; opt-in by setting `jellyfin_port` in `settings.toml` (conventionally `8096`) — omitting the key disables the server entirely; spoofs `ProductName: "Jellyfin Server"` and `Version: 10.11.11` so the SDK-generated clients (Finamp, Findroid, Streamyfin, Amcfy Music, Symfonium) treat it as a real Jellyfin server; reuses `subsonic_username` / `subsonic_password` from `settings.toml` as the Jellyfin credentials (additionally gated on the password being non-empty); tokens are persisted in a new `jellyfin_tokens` table (migration applied at startup) and accepted via `X-Emby-Token`, `Authorization: MediaBrowser Token=…`, or `?api_key=` query param on streaming URLs; item IDs are deterministic dashed-UUIDs derived from the native `Artist/Album/Track` ids and round-tripped via a new `jf_guids` lookup table; full route surface covers `/System/Info{,/Public}`, `/Users/AuthenticateByName` (PascalCase + lowercase variants for Amcfy), `/Users/Public`, `/Users/{id}/Views`, `/UserViews`, `/Library/MediaFolders`, `/Items` (with `parentId`, `includeItemTypes`, `searchTerm`, `albumArtistIds`/`artistIds`, `ids`, pagination — accepts both camelCase and PascalCase plus repeated keys), `/Items/{id}` + `/Items/{id}/{Images/Primary,File,PlaybackInfo}`, `/Audio/{id}/{stream,stream.{ext},universal}` with HTTP Range support, `/Search/Hints` backed by the same `repo::{artist,album,track}::filter` LIKE queries, `/Sessions{,/Capabilities/Full,/Playing,/Playing/Progress,/Playing/Stopped}` acks, `/ScheduledTasks/Running/{id}` + `/Library/Refresh` (return 204 — actual scans are owned by `audio_scan::start_watcher`), `/Shows/*` + `/UserItems/*` + `/Items/{Suggestions,Resume,Latest}` stubs for client home rails, and `/Items/{id}/Images/{kind}` (both PascalCase and lowercase `/items/...` paths for Findroid); discovery runs in two tokio tasks — `_jellyfin._tcp.local.` mDNS broadcast plus a UDP listener on port `7359` that answers the literal `"Who is JellyfinServer?"` probe with `{"Address":"http://<lan-ip>:<port>","Id":…,"Name":…}`
- `settings`: new optional `jellyfin_port: Option<u16>` field on `NewGlobalSettings` (defaults to `8096`); also added to the `Into<NewGlobalSettings>` impl in `crates/rpc/src/lib.rs` so gRPC settings round-trips don't drop the field

### Fixed
- `jellyfin`: album art now resolves correctly — `rockbox-library` stores `album_art` as one of three forms (bare filename living under `~/.config/rockbox.org/covers/`, absolute filesystem path, or `http(s)://` URL for Rocksky-sourced artist images), but the initial Jellyfin image handler joined bare filenames against `music_dir` and got `NotFound`; replaced with a `serve_art_value` that mirrors Navidrome's resolver (proxy `http(s)://` via `reqwest`, read absolute paths as-is, prefix bare filenames with `~/.config/rockbox.org/covers/`), and added the "if the album row has no art, search its tracks" fallback Navidrome already uses

## [2026.06.26]

### Added
- `library`: periodic library rescan + delete reconciliation as a backstop for filesystem watcher events that get dropped silently — Linux inotify is a no-op on NFS/SMB/FUSE mounts, and the kqueue backend on BSDs (NetBSD) coalesces multi-file drops into a single `Vnode::Write` event and only surfaces one new file per coalesce; `start_watcher` now spawns a background tokio task that ticks every `ROCKBOX_RESCAN_INTERVAL_SECS` seconds (default `120`, set `0` to disable), runs `scan_audio_files` then the new `audio_scan::reconcile_deletions` which walks `repo::track::all` (already filters `is_remote = 0`) and calls `repo::track::delete_by_path` for any track whose path no longer exists on disk; overlapping ticks are skipped via `tokio::sync::Mutex::try_lock` so the periodic pass can never pile up behind a still-running initial scan on a large library
- `ci`: Debian and Fedora package builds, published to GitHub Releases and Gemfury — new `.github/workflows/linux-x86_64-build.yml` builds `rockbox`, `rockboxd`, `librockboxd.a`, and `rockbox-gpui` (via `gpui/package.sh`, hard-fails if the Linux GPUI build breaks), downloads `typesense-server` v30.1 from `dl.typesense.org`, and packages a `.deb` and `.rpm` that ship all four binaries plus the `rockbox-gpui` desktop entry and PNG icon; the existing `linux-aarch64-build.yml` is extended with a `.deb` containing `rockbox` + `rockboxd` + `typesense-server` (no GPUI); `linux-armhf-build.yml` gains a `.deb` with just `rockboxd`; each workflow checks for `FURY_TOKEN` and `FURY_ACCOUNT` secrets and pushes its own packages to `push.fury.io`, mirroring the smolsonic release pipeline

### Fixed
- `build`: flatten `librockboxd.a` on Linux so consumers (notably the GPUI desktop app linked via `gpui/build.rs`) can resolve symbols — `zig build lib` packs each input static archive as a nested `.a` member of the output, and on Linux GNU/rust-lld doesn't unpack nested members, so symbols like `rb_daemon_start` from `librockbox_embed.a` end up undefined and the final `cc` link fails with `rust-lld: error: undefined symbol`; new `scripts/flatten-archive.sh` walks each archive with `ar t` and pulls every member individually via `ar xN <occurrence>` (the per-occurrence extract is required because `apps/SOURCES` puts two distinct objects both named `list.o` into `librockbox.a` — one from `apps/gui/list.c` defining `gui_synclist_*` / `simplelist_*`, one from `apps/gui/bitmap/list.c` — and a bulk `ar x` lets the second overwrite the first), renames each .o with its archive-lineage prefix to also dodge collisions across nested archives, drops `.so` refs (linked via `-l` flags at the consumer side), then the linux branch in `zig/build.zig`'s `lib` step calls it in-place, mirroring the existing macOS `libtool -static` repack
- `firmware/headless`: gate the CPU stubs in `firmware/target/hosted/headless/cpuinfo-noop.c` behind `#if !(defined(__linux__) && !defined(__ANDROID__))` so they don't collide with the real `cpuinfo-linux.c` (always compiled on Linux non-Android per `firmware/SOURCES:17-22`) — the duplication was previously masked by lazy archive pull-in when zig linked the rockboxd executable directly from `libfirmware.a`, but the flattened `librockboxd.a` consumed by GPUI drags both `.o` files in (battery stubs in noop force its pull-in, real CPU funcs in linux force its) and rust-lld rejects `current_scaling_governor` / `cpuusage_linux` / etc. as duplicate symbols; battery stubs (`_battery_level` / `_battery_voltage` / `_battery_time`) stay because they're not provided by `cpuinfo-linux.c`

## [2026.06.25]

### Added
- Embedded S3 admin web UI — new React + Vite SPA under `crates/s3/s3webui/` (TanStack Router/Query + Jotai + FlyonUI/Tailwind) embedded into `rockbox-s3` via `rust-embed` and served at `/admin/` on the S3 port; the dashboard signs requests with SigV4 directly via AWS SDK v3 in the browser, login validates against the configured `s3_access_key` / `s3_secret_key`, and the standard buckets / objects / upload / settings views are wired in; `HEAD /{bucket}` is now implemented so `HeadBucket` succeeds; the `Dockerfile` gains an in-container Bun builder stage and every CI workflow that compiles `rockbox-s3` runs the SPA build first so the binary always ships with an up-to-date UI

### Changed
- Renamed the user-facing product from "Rockbox Zig" to "Rockbox Daemon" and updated every reference to the GitHub repo URL, AUR package (`rockbox-zig-bin` → `rockboxd-bin`), macOS pkg identifier, Electron `appId`, and Gleam SDK metadata; the published npm scope `@rockbox-zig/sdk` and the `rockboxzig.mintlify.app` docs URL are intentionally unchanged

### Fixed
- `ci(gpui)`: force relink of `librockboxd.a` in the `release-gpui` workflow — the cache key only hashes `Cargo.lock`, so edits to `crates/embed` sources could leave a stale archive missing newer exports (e.g. `rb_daemon_start`), failing the gpui link step

## [2026.06.23]

### Added
- S3-compatible HTTP API — new `crates/s3` actix-web server listens on `s3_port` (default `9000`) and exposes `music_dir` as a single fixed bucket (`music`, region `us-east-1`); supports `PutObject`, `DeleteObject`, `GetObject`, `HeadObject`, `ListObjectsV2`, and `ListBuckets` with AWS Signature V4 header-form authentication; the new `s3_enabled`, `s3_host`, `s3_port`, `s3_access_key`, `s3_secret_key` keys in `settings.toml` gate startup (server stays off when disabled or credentials are empty); per-PUT cap 2 GiB, uploads restricted to the same audio-extension allowlist as the library scanner; `STREAMING-AWS4-HMAC-SHA256-PAYLOAD` is not supported (clients should use `UNSIGNED-PAYLOAD` or sign the full body — awscli v2.23+ needs `AWS_REQUEST_CHECKSUM_CALCULATION=when_required` to opt out of the new default-on chunked signing); README, CLAUDE.md and the mintlify reference document client recipes for awscli, rclone, and MinIO Client
- `library`: filesystem watcher (`crates/library/src/watcher.rs`) — recursive `notify`-based watch on `music_dir` keeps the SQLite tag database in sync with on-disk changes; `Create` and data-modify events call `save_audio_metadata`, `Remove` events call the new `repo::track::delete_by_path` (cascades through `album_tracks`, `artist_tracks`, `playlist_tracks`, `favourites`), and rename events split into add/remove pairs; non-audio files are ignored via the same 18-extension allowlist the scanner uses; the watcher boots after the initial scan in `crates/cli/src/lib.rs::run_indexing()` and is also the sync engine behind the new S3 API — no separate DB code path

### Fixed
- `build`: link `CoreServices.framework` on macOS so the `notify` crate's FSEvents backend resolves at link time — macOS-Intel CI was failing the Zig link step with undefined symbols `_FSEventStream{Create,Invalidate,Release,ScheduleWithRunLoop,Start,Stop}`; added to both the `rockboxd` executable and the embeddable `librockboxd.a` link blocks in `zig/build.zig`, and to the embedder-facing framework list in CLAUDE.md

## [2026.06.18]

### Fixed
- `server`: hard-exit when `run_http_server()` returns `Err` — the spawning thread used to log and exit silently, leaving `:6063` dead while the rest of rockboxd kept running and surfaced only as downstream GraphQL request errors
- `playback`: skip HTTP look-ahead of the currently-playing URL — `REPEAT_ALL` on a single-track playlist wrapped `playlist_peek(+N)` back to the same URL every look-ahead cycle, each iteration spawning a fresh TCP+TLS+GET and a 4 MB prefetch; treating same-URL HTTP look-ahead as end-of-playlist lets natural-end re-load the next URL fresh without thrashing the buffering thread (local files unaffected — they share the page cache)
- `playback`: throttle HTTP look-ahead while the current track is still streaming — parallel HTTP bufopens shared the buffering-thread filechunk round-robin and starved the active codec until "no more PCM data" fired; deferred until the active stream finishes
- `pcm-cmaf`: poll every 10 ms for up to 5 s for the next PCM chunk before bailing — the sink used to give up the instant `pcm_play_dma_complete_callback` returned `false`, killing playback within ~23 ms of every track start whenever the codec hadn't queued the next chunk yet
- `library`: replace `.expect()` panics on `Probe::open()` in the `album_art`, `copyright_message`, and `label` extractors with logged errors that return `Ok(None)`, so an unreadable track no longer aborts the library scan

## [2026.06.15]

### Added
- `rockboxd login <handle>` — OAuth login to Rocksky using your Bluesky handle; opens the authorisation URL in the default browser and spins up a minimal tokio TCP server on port 6996 to receive the callback token, which is persisted to `~/.config/rockbox.org/token`
- `rockboxd whoami` — print the currently logged-in Rocksky user (reads the stored token and resolves the handle via the Rocksky API)
- `rockboxd settings pull [--did <DID_OR_HANDLE>]` — fetch audio settings (equalizer, crossfade, replaygain, tone/bass/treble/balance/channels) from Rocksky and merge them into `~/.config/rockbox.org/settings.toml` without touching any other fields; `--did` enables public access — any user's settings can be pulled without a token by passing their DID or handle
- `rockboxd settings push` — read the current `settings.toml` and upload the audio sections (equalizer, crossfade, replaygain, tone) to Rocksky via `app.rocksky.rockbox.putAudioSettings`; requires a valid stored token
- All four subcommands exit the process immediately after completing — the Zig firmware and gRPC/HTTP servers are never started, avoiding the C-global-initialisation segfault that would occur before `main_c()` runs

## [2026.06.14]

### Added
- `arm-unknown-linux-gnueabihf` cross-compilation target — new `scripts/build-armhf.sh` builds a native ARMv6 hard-float `rockboxd` binary (e.g. Raspberry Pi Zero) using the `Dockerfile.arm-unknown-linux-gnueabihf` cross-toolchain; Zig links with `-Dtarget=arm-linux-gnueabihf -Dcpu=arm1176jzf_s`; `Cross.toml` wires `cross build` to the same Docker image; firmware configure target `208` (ARMHFHOST) reuses the headless target files with `arm-linux-gnueabihf-gcc` and `-march=armv6 -marm -mfpu=vfp -mfloat-abi=hard`
- `crates/alsa-sink` — direct libasound PCM sink for ARM Linux; uses `snd_pcm_writei` (RWInterleaved, same as `aplay`), avoiding cpal's ALSA backend and the `snd_pcm_status_get_htstamp` null-PLT-entry crash on older ARM devices; ALSA is opened once in `pcm_alsa_postinit()` and the writer thread lives for the daemon lifetime so resume after a pcmbuf-dry stall is instant (no re-open latency); enabled via `--features fts5,alsa-sink` in the ARM build; registered as `PCM_SINK_ALSA = 9` in `firmware/export/pcm_sink.h`
- `firmware/target/hosted/headless/pcm-alsa.c` — C PCM sink ops mirroring `pcm-cpal.c` but calling `pcm_alsa_*` entry points
- `.github/workflows/linux-armhf-build.yml` — CI workflow that builds and uploads the armhf binary to GitHub Releases

### Fixed
- ARM Linux: `SIGILL` from `__ARMv7ABSLongThunk__` — Ubuntu's `arm-linux-gnueabihf-gcc` defaults to `-march=armv7-a`; added `-march=armv6 -marm` to the configure `armhfhostcc()` function so all C objects are tagged ARMv6; Zig's LLD then uses ARMv6-compatible thunks that work on ARM1176JZF-S
- ARM Linux: `SIGILL` at startup — LLD derives `HasMovt` from the target triple (`arm-linux-gnueabihf` = conventional ARMv7), generating `movw`/`movt` thunks even when object attributes say ARMv6; fixed by using `ReleaseFast` to produce a compact binary (< 32 MB) that fits within LLD's direct-branch range, eliminating the need for long-range veneers
- ARM Linux: `SIGSEGV` in `SimpleBroker::subscribe` (`dyn Any` vtable null) — Zig's LLD generates zero vtable entries for `dyn Any + Send` COMDAT groups on ARM 32-bit in `ReleaseFast` mode; replaced `HashMap<TypeId, Box<dyn Any + Send>>` in `crates/graphql/src/simplebroker.rs` with a type-erased `ErasedSenders` struct storing the drop function as a heap pointer written at runtime (not a link-time vtable), so every function pointer is a valid non-zero Thumb address
- ARM Linux: `SIGSEGV` in `alsa::pcm::Status::get_htstamp` — on ARM devices where libasound ships `snd_pcm_status_get_htstamp` as a static inline (not an exported symbol) the PLT entry resolves to 0x00000000 at runtime, crashing in cpal's ALSA timing probe; fixed by replacing cpal with the direct `alsa-sink` that never calls this function
- ARM Linux: 32-bit ABI mismatches in `crates/sys` — `c_long`/`c_ulong` are 32-bit on ARM (not 64-bit); added `as c_long` / `as c_ulong` casts in `metadata.rs`, `playback.rs`, `playlist.rs`, `sound/dsp.rs`, `system.rs`, `tagcache.rs`, and `as u64`/`as i64` field casts in `types/mp3_entry.rs`; `crates/cli/src/lib.rs` now uses `libc::rlim_t` instead of `u64` for `rlimit` fields
- ARM Linux: `audiohw_set_volume` undefined reference to `pcm_cpal_set_volume` — gated the cpal volume call on `!ARMHFHOST` in `audiohw-noop.c`; volume is handled by Rockbox's DSP layer (`HAVE_SW_TONE_CONTROLS`) on ARM
- ARM Linux: `PCM_SINK_ALSA = 9` array-bounds error in `pcm.c` — enum entry was declared before `PCM_SINK_CMAF = 8`, making `PCM_SINK_NUM = 9` and `sinks[9]` out-of-bounds; moved ALSA entry after CMAF so `PCM_SINK_NUM = 10`
- `firmware/export/config.h`: added `#elif defined(ARMHFHOST)` → `#include "config/armhfhost.h"` so the ARM hosted build is recognised as a valid platform; added `ARMHFHOST` guard to `audiohw.h` (sdl_codec.h inclusion) and `filesystem-app.c` (`rbhome` pointer declaration)
- `metadata`: `probe_content_type_format` now logs the exact `Content-Type` string received (or reports that `stream_content_type` returned < 0) to stderr, making HTTP format-detection failures visible; added `audio/x-aac` and `audio/vnd.dlna.adts` to the AAC-BSF MIME mapping

## [2026.06.08]

### Fixed
- GraphQL `playAlbum` / `playArtistTracks` / `playGenreTracks` / `playDirectory` / `playTrack` / `playLikedTracks` / `playAllTracks` were no-ops when the active output was the CMAF (HLS / DASH) sink — the `check_and_load_player!` macro used a `host != "" && port != 0` heuristic to detect external cast players, but CMAF advertises `host="localhost"`, `port=7882` for its own HTTP server, so the macro misrouted the request to `/player/load` (which 404s because `state.player` is only populated for Chromecast) and returned `Ok(0)` before building the playlist; now matches the RPC variant and gates on `is_cast_device` instead, so local PCM sinks (CMAF, FIFO, builtin, squeezelite, AirPlay, UPnP) fall through to the regular playlist-build path
- MPD `restore_playlist`: bounds-check the persisted `resume_index` against the current playlist length before indexing — a stale resume index from a prior session with a longer queue was panicking the MPD thread with `index out of bounds: the len is 15 but the index is 91` and aborting the daemon

## [2026.06.07]

### Added
- CMAF (HLS + DASH) PCM sink — new `rockbox-cmaf` crate encodes PCM to AAC-LC (fdk-aac) and serves HLS + DASH manifests with fMP4 segments over HTTP; enabled via `audio_output = "cmaf"` (or `"hls"` / `"dash"`) plus `cmaf_http_port`, `cmaf_bitrate`, and optional `cmaf_segment_dir` for mirroring artefacts to disk for an external origin (nginx, Caddy, CDN); registered as `PCM_SINK_CMAF = 8` in `firmware/pcm.c`; surfaced as a virtual device selectable via `/connect/cmaf` with broadcast icons in the GPUI, Expo, web, and macOS device pickers
- Standalone HLS/DASH player — new `crates/hls` decodes `.m3u8` / `.mpd` URLs and pushes PCM straight into the active sink via new `pcm_external_write` / `pcm_external_set_freq` firmware hooks (no pcmbuf, no codec dispatcher) so the same audio-output graph (cpal, AirPlay, Snapcast, CMAF, …) reroutes a Rockbox-internal HLS broadcast to any sink the user picks; `PlayTrack` / pause / resume / next / previous / seek / `hardStop` in `crates/rpc` detect an active HLS session and dispatch locally or forward to the broadcaster over gRPC so peers stay in sync
- Web UI: `HlsAutoConnect` attaches an `<audio>` element to `/hls/master.m3u8` whenever the active device type is `cmaf` / `hls` / `dash`, and `HlsVolumeControl` adds a local browser volume slider; Docker default `audio_output` flipped to `cmaf` and port `7882` exposed; new Mintlify page documents the sink; GraphQL `globalSettings.cmafHttpPort` added

### Fixed
- CMAF sink: encoder now bootstraps with a full `SEGMENT_WINDOW` of silence so `hls.js` / `dash.js` don't fatal on a fresh manifest; a dedicated silence-pacer thread keeps the manifest live between tracks without ever mixing into real-audio chunks; `pcm_cmaf_start()` is now called eagerly from `load_settings` / device connect so the HTTP endpoint binds before the first track plays
- Android HTTP streaming smoothness — `cpal_thread` priority boosted (`setpriority(PRIO_PROCESS, tid, -19)`) and `NowPlayingService` now acquires `PARTIAL_WAKE_LOCK` + `WifiLock` while the daemon is running, eliminating the doze-induced stutters on Wi-Fi remote streams
- `netstream`: `rb_net_len` and `rb_net_content_type` now wait for `open_done` before reading stream state, so callers see the real length / MIME instead of `-1` / empty when the HTTP open is still in flight
- `netstream`: removed TCP keepalive from the global `reqwest` client — keepalive probes were tripping middleware and aborting long Range reads on some CDNs
- `netstream`: non-blocking `rb_net_open()` returns a handle immediately while the connect happens in a worker; combined with TCP keepalive on the per-stream client (kept) and an EOF probe for servers that omit `Content-Length`, this unblocks both the audio thread and the UI on slow first-byte servers
- `netstream`: detect and reconnect on premature TCP EOF mid-stream — the prefetch thread now restarts the underlying request from the last known offset instead of declaring the stream dead, fixing mid-track cutoffs on lossy mobile connections
- `netstream`: seek `Range` requests now retry on transient errors; huge forward skips on servers that ignore `Range` fast-fail instead of redownloading the whole prefix
- `netstream`: 30 s hard timeout removed from `read_into` — the prefetch thread's own retry budget now governs how long a read can wait, so a brief stall no longer kills the stream
- `pcm-cpal`: DMA thread exits immediately on stream error instead of draining `pcmbuf`, so the next track / device switch can re-arm the sink without waiting for a stale flush
- `cpal` sink: recover from stream errors and break the push deadlock — error callback now signals the writer so `pcm_cpal_push` returns instead of spinning on a dead stream
- Android: larger prefetch buffer (16 MB) + more retries make HTTP streams resilient to Wi-Fi / cellular handoffs
- Navidrome HTTP track artwork — stream URL is now propagated as the track `path` and the bridge derives the cover-art URL from it, so artwork appears in the miniplayer, full-screen player, and queue without an extra round-trip

### Changed
- Default Docker `audio_output` flipped to `cmaf`; port 7882 added to the exposed ports list so HLS / DASH playback works out of the box from a container

### Fixed
- HTTP streaming: removed reqwest total-request timeout (only `connect_timeout` 15 s remains) — the previous 30 s deadline killed large remote files mid-stream; `read_as_file()` reverted to a retry-loop that fills the full requested buffer
- Buffering interleaving: `fill_buffer()` now passes `BUFFERING_DEFAULT_FILECHUNK` instead of `0` when a second handle has remaining data, so next-track pre-buffering round-robins with the current track instead of monopolising the buffering thread and starving the ring buffer
- HTTP pre-buffering cutting current-track playback: `buffer_handle()` caps HTTP handles to one `BUFFERING_DEFAULT_FILECHUNK` per call; `streamfd.c` replaces per-chunk `fprintf(stderr, …)` with `logf()` (compiled out in production) to eliminate hundreds of blocking `write(2)` syscalls per track
- Expo: Navidrome cover art now appears in the miniplayer, full-screen player, and queue when playing ND HTTP streams — `coverArtUrlFromStreamUrl()` added to `navidrome-client.ts` reconstructs a `getCoverArt` URL from the `id`, `u`, `t`, `s` parameters embedded in the stream URL; used as a fallback in `trackFromProto` when `album_art` is empty

## [2026.05.27]

### Added
- Navidrome / Subsonic support in the macOS Swift app — `NavidromeService` (Subsonic API client with MD5 token auth), `NavidromeManager` (multi-server persistence, active server switching, optimistic star toggling, cover art derivation from stream URLs), `NdResponseCache` (stale-while-revalidate actor cache, 30 min fresh TTL, 24 h eviction), `NdLibraryView` (Albums / Artists / Songs / Liked / Playlists sections with infinite-scroll pagination), `NdSongRowView` (track art toggle, hover play, star button, Play Next/Last + Go to Album/Artist context menu), `NdAlbumDetailView`, `NdArtistDetailView`, `NdPlaylistDetailView` (Play / Shuffle), and search integration (when a Navidrome server is active, `search3` replaces local gRPC search with ND artist circles, album cards, and song rows)

### Removed
- PCM volume normalizer (`pcm_normalizer.c`, `pcm_normalizer.h`, Rust bindings, settings field, docs) — superseded by ReplayGain perceived-loudness normalisation

### Fixed
- Expo: `AbortSignal.timeout()` replaced with `AbortController` + `setTimeout` in `navidrome-client.ts` — `AbortSignal.timeout` is absent in some Hermes / React Native versions and was silently swallowing timeouts, making every fetch return `null`; switched to `md5` npm package (removed inline implementation); set `NSAllowsArbitraryLoads=true` in iOS `infoPlist` to unblock HTTPS servers that do not meet strict ATS TLS requirements
- Expo ND album detail: mirror local `album/[id].tsx` hero layout — blurred background image (`blurRadius=40`) + dark gradient overlay + art shadow + scale/fade scroll animation + sticky header title fade-in; cover art URL now uses a stable salt derived from credentials so `expo-image`'s disk cache is not busted on every render
- Expo ND detail screens: cover art now renders correctly by placing computed dimensions on a parent `View` and giving `Image` `className="w-full h-full"` so NativeWind owns the style; track rows in album and playlist detail screens now include a `TrackMenuButton` "…" context menu
- GPUI: Navidrome cover art is now derived directly from the stream URL parameters (`id`, `u`, `t`, `s`) instead of requiring an active server connection, eliminating blank album art when playback starts before the ND panel is connected; removes the `PENDING_COVER_ART` staging mutex and the async `getSong` round-trip
- macOS Now Playing / `MPNowPlayingInfoCenter`: cover art priority corrected — `coverArtUrl(forStreamUrl:)` is now tried first (returns `nil` for local tracks), then falls back to `albumArt`; the previous order always hit the `albumArt` branch even when it pointed at an empty path, so Navidrome tracks showed no artwork in the system Now Playing widget
- CI: Android firmware build workflows now delete `make.dep` before `make lib` to force a fresh dependency scan after prefix-restore cache hits that carry stale header dependencies (e.g. the `pcm_normalizer.h` removal)

## [2026.05.25]

### Added
- Subsonic / Navidrome API compatibility server on port **4533** — any client that works with Navidrome (Cassette, Symfonium, DSub, Ultrasonic, Substreamer, Clementine, Sublime Music, …) can browse and stream music from `rockboxd` without additional setup; enabled by adding `subsonic_username` and `subsonic_password` to `settings.toml`
- Implemented endpoints: `ping`, `getUser`, `getMusicFolders`, `getScanStatus`, `startScan`, `getArtists`, `getArtist`, `getAlbum`, `getSong`, `getIndexes`, `getMusicDirectory`, `getGenres`, `getSongsByGenre`, `getAlbumList` / `getAlbumList2`, `getRandomSongs`, `getStarred` / `getStarred2`, `stream` (with HTTP `Range` / seek support), `download`, `getCoverArt`, `scrobble`, `getNowPlaying`, `updateNowPlaying`, `search2` / `search3`, `getPlaylists`, `getPlaylist`, `createPlaylist`, `updatePlaylist`, `deletePlaylist`, `star` / `unstar` (mirrored to Rocksky), `getArtistInfo` / `getArtistInfo2`, `getAlbumInfo` / `getAlbumInfo2`, `getSimilarSongs` / `getSimilarSongs2`, `getTopSongs`, `getLyrics`
- Auth: MD5 token mode (`t` + `s`) and plaintext / `enc:<hex>` mode (`p`)
- `getCoverArt` resolves bare filenames to `~/.config/rockbox.org/covers/` and proxies Rocksky HTTP URLs for artist images
- Mintlify docs page `mintlify/clients/subsonic.mdx` covering setup, auth modes, all endpoints, recommended clients, cover art IDs, and range-request support

## [2026.05.17]

### Added
- Web UI mobile layout — bottom-tab navigation bar, persistent mini-player dock, and a full-screen player modal; mirrors the Expo mobile app information architecture on small viewports

### Fixed
- Web UI: resuming a paused track now calls `resume` instead of restarting the track from the beginning — `useResumePlaylist` now scopes the playlist-reload logic to `status === 0` (stopped) only, preserves `nowPlaying` fields while paused, and fixes an `onPause` timeout that was permanently locking subscription updates after any pause
- Bluetooth: adapter is powered on before listing paired devices or disconnecting, preventing `BluetoothError::NotPowered` on adapters that idle to off

## [2026.05.15]

### Added
- Plex Media Server browsing via `plex://` scheme — mDNS discovery (`_plexmediasvr._tcp.local.`), token-in-URL auth, full library / playlist / album / artist / track navigation
- Jellyfin Media Server browsing via `jellyfin://` scheme — manual server entry, API-key auth, full content hierarchy browsing
- Navidrome Media Server browsing via `navidrome://` scheme — manual server entry, MD5 token auth (Subsonic API), `getIndexes` + `getMusicDirectory` browsing
- Kodi/XBMC Media Server browsing via `kodi://` scheme — JSON-RPC API, library browsing for audio albums, artists, genres, and tracks
- Expo mobile app: Plex, Jellyfin, Navidrome, and Kodi server browsing surfaced in the Files tab alongside the existing local filesystem view
- WASM browser build: settings API (persist EQ / DSP / volume / crossfade to in-memory config), playlist persistence across reloads, `rb_set_repeat` export (repeat off / all / one / shuffle)
- Real-time DSP/EQ API exposed over HTTP, gRPC, and GraphQL — `setEq` mutation with `enabled`, `precut`, and per-band `cutoff`/`Q`/`gain` fields; backed by `dsp_set_eq_coefs()` called directly on the audio thread to avoid audible cuts

### Changed
- Docker base images upgraded from Debian bookworm → trixie across all three Dockerfiles; Rust base image bumped from 1.94 → 1.95
- Nix flake now builds only `rockboxd` (removed unused outputs)
- `settings.toml` example updated to document the new media-server `audio_output` entries

### Fixed
- WASM: `seek`, crossfade, bass/treble DSP controls now apply correctly; real-time events (position, track change) fire reliably; crossfade mode change posts `Q_AUDIO_REMAKE_AUDIO_BUFFER` only when audio is playing to avoid an audible cut when stopped
- WASM: EQ real-time application and persistence — coefficient updates call `dsp_set_eq_coefs()` in the `wasm_cmd` handler without posting `REMAKE`; band gain multiplied by 10 (tenths of dB) before passing to `rb_set_eq_band`
- WASM: EQ cutoff and Q values now match the preset layout (Q 7.0, 10-band display) after correcting the unit conversion in `web/rockbox.js`
- Dithering, Auditory Frequency Resolution (AFR), and Perceptual Bass Enhancement (PBE) controls in the web UI now reflect changes immediately — `GlobalSettings` mutations now call the corresponding DSP setters and trigger `tracing`-level log output

## [2026.05.09]

### Fixed
- DSP compressor divide-by-zero crash on x86_64 (`SIGFPE` in `get_att_rls_coeff`) — added `release > 0` guard in `compressor_update()` mirroring the existing `attack > 0` guard; ARM64 silently returned 0 on integer divide-by-zero while x86_64 faulted; also added function-level guards in `get_att_rls_coeff` and `get_lpf_coeff` for zero `rc`/`fs`/`rc_units` parameters, and an early `fs <= 0` return in `compressor_update` for uninitialised output frequency
- Startup hang on second+ launch — FTS5 backfill `WHERE NOT EXISTS (SELECT 1 FROM fts_table f WHERE f.id = t.id)` forced an O(N) full scan per row (O(N²) total) because `id` is `UNINDEXED` in FTS5; replaced all four backfill INSERTs with an uncorrelated `WHERE NOT EXISTS (SELECT 1 FROM fts_table)` which SQLite short-circuits at the first row (O(1) for non-empty tables)
- Library startup blocked indefinitely on repeated runs — SQLx hangs when re-executing `CREATE VIRTUAL TABLE IF NOT EXISTS` on an existing FTS5 virtual table; fixed by checking `sqlite_master` before the migration and skipping it entirely if `track_fts` already exists; same guard added for `dedupe_genres` (checks `UNIQUE` constraint on `genre` table)
- FTS5 and `dedupe_genres` migrations ran in slow DELETE journal mode — `PRAGMA journal_mode=WAL` was set only after all migrations; moved to `SqliteConnectOptions::journal_mode(Wal)` so WAL is active from the first connection
- FTS5 index migration moved to a background `tokio::spawn` task so startup is non-blocking; `dedupe_genres` (schema DDL) remains synchronous with an O(1) skip guard
- cpal PCM sink: audible silence gap at the start of every track on Linux — `sink_dma_start()` previously stored the first chunk in `pcm_data`/`pcm_size` and then called `pthread_create`, leaving the ring empty for the 1–5 ms thread-creation window; fixed by pushing the first chunk synchronously via `pcm_cpal_push()` before spawning the writer thread so the ring is pre-filled when `running=true` is set; the writer thread now picks up from chunk 2 onwards; also added `!r.running` early-exit to the f32 cpal callback (mirrors the existing i16 guard) and reset resampler state (`cur_valid = false`, `phase = 0`) in `pcm_cpal_start()` to prevent interpolation artefacts from the tail of the previous track

## [2026.05.05]

### Added
- Headless host target and `cpal` PCM sink (`audio_output = "cpal"`) — runs Rockbox without SDL on any OS audio backend (ALSA, CoreAudio, WASAPI, JACK) via CPAL; build with `scripts/build-headless.sh`; documented in `HEADLESS.md`
- Genres API — gRPC, GraphQL, REST, and CLI endpoints to list genres, fetch tracks by genre, and add genre-based smart playlist rules; genre deduplication SQL migration bundled
- Disc/track number support in the Expo mobile album view — `TrackList` component sorts by (disc, track) and renders disc-section headers for multi-disc releases; `proto track_number`/`disc_number` fields mapped through to the UI
- Pull-to-refresh / rescan in the Expo library tab

### Changed
- CI workflows, macOS build scripts, Dockerfile, and `install.sh` streamlined — significant reduction in duplication and overall build time
- Android `cdylib` option now available in the `tools/configure` interactive menu

### Fixed
- M4A/AAC files decode silently in `CODECS_STATIC` builds — dead-write elimination in `libm4a/demux.c` was optimizing away box-parsing reads; replaced with live-return readers (`stream_read_uint*` + `stream_skip`)
- macOS linker: `Security.framework` explicitly linked in `zig/build.zig` to resolve missing symbol errors when using macOS Security APIs
- Expo mobile app re-establishes gRPC subscriptions when the app returns to the foreground (`reconnectEpoch` bump + `reapplyServerUrl()`)

## [2026.05.03]

### Added
- Mintlify documentation site under `mintlify/` with the Linden theme; OpenAPI spec regenerated and ASCII architecture diagrams replaced with `CardGroup` components
- Linux-specific window controls (minimize / maximize / close) in the GPUI titlebar — macOS/Windows continue to use native traffic-light controls

### Changed
- GPUI titlebar drag areas now call `window.start_window_move()` from an `on_mouse_down` handler instead of relying on `WindowControlArea::Drag`, fixing window dragging on Linux/X11
- Debian and RPM packages now declare XKB/XCB build dependencies (`libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libxcb1-dev`, `libxcb-render0-dev`, `libxcb-shape0-dev`, `libxcb-xfixes0-dev`); README updated with the matching install instructions
- Debian package version bumped to `2026.05.03`

### Fixed
- GPUI app no longer fails to build on Linux: `souvlaki` is now a non-Linux-only dependency and `NowPlayingManager` ships a no-op Linux stub, since the OS media-control APIs souvlaki targets are not available there

## [2026.05.02]

### Added
- New SDKs for controlling rockboxd from Python, Ruby, Elixir, Gleam, and Clojure (`sdk/python/`, `sdk/ruby/`, `sdk/elixir/`, `sdk/gleam/`, `sdk/clojure/`) — each ships with examples covering playback, queue, library search, saved/smart playlists, volume/EQ, browse, devices, Bluetooth, and plugins
- TypeScript SDK gains 15 runnable examples (`sdk/typescript/examples/`) plus a Bluetooth API (`api/bluetooth.ts`) and a `getVolume` / `VolumeInfo` endpoint on `api/sound.ts`
- TS SDK types extended with `browse.displayName` and `album.copyrightMessage`

### Fixed
- HTTP/remote tracks now hydrate `Mp3Entry` metadata (title, artist, album, duration, etc.) from the DB `Track` record in the playlist handlers when Rockbox cannot read tags locally
- GPUI Library page: text truncation and unexpected overflow on likes and track rows resolved by adding `min_w_0` / `flex_shrink_0` to the flex containers
- Regenerated tonic/prost UPnP bindings under `crates/upnp/src/api/`

## [2026.05.01]

### Added
- Bluetooth button in the GPUI mini-player — shown when Bluetooth is available; opens the device picker and fetches paired devices on toggle
- Cover URLs in GPUI now follow the active server via `get_covers_base()` instead of the hardcoded `http://localhost:6062/covers/` base

### Changed
- HTTP server (`crates/server`) migrated from a custom request/response layer to **Actix-web** — handlers now accept `web::Data`, `web::Path`, and `web::Query` and return `actix_web::Result<HttpResponse>`; blocking C FFI work is offloaded to `web::block`
- Tokio runtimes for the controls and MPD servers are now shared via `OnceLock` instead of being created per-thread, reducing overhead and avoiding nested-runtime panics
- `RLIMIT_NOFILE` is raised to 4 096 at startup on Unix to accommodate large music libraries

### Fixed
- Audio `stop` and `pause` are now non-blocking — they use `audio_queue_post` so they can safely be called from any OS thread; `audio_hard_stop` posts `Q_AUDIO_STOP` with `data=2` and the audio thread frees `audiobuf_handle` itself, preventing cross-thread frees
- Blocking C FFI calls in playlist handlers run on `web::block` threads to avoid starving Actix worker threads and prevent nested tokio/reqwest blocking contexts
- Live metadata lookups are skipped for HTTP tracks; Rockbox's own UPnP renderers are excluded from the UPnP device list
- Bluetooth availability check uses `fetchGlobalStatus()` (gRPC `GetGlobalStatus`) instead of `getDevices()` to avoid spurious `UNIMPLEMENTED` errors on probe
- Bluetooth availability is now polled in a background task and updated via `std::sync::mpsc` to avoid cross-runtime waker issues when bridging Tokio → GPUI
- `observe_global` registrations in GPUI now call `.detach()` instead of silently dropping the subscription handle
- RFC3339 datetime migration — a SQL migration normalises `NULL`/blank and `YYYY-MM-DD HH:MM:SS` timestamps in the library database to RFC3339 so SQLx `DateTime<Utc>` decoding no longer fails
- Favourites queries now use `INNER JOIN` and filter out empty-string IDs, excluding bogus entries from results
- mDNS scanning now prefers IPv4 addresses (192.168 → 10 → others) and selects the best non-loopback/link-local address so multiple records for the same host coalesce correctly
- `println!`/`eprintln!` diagnostics in `crates/controls` and `crates/mpd` replaced with `tracing::error!`
- macOS app listens for server-change notifications and restarts streaming, re-fetches settings, device state, and Bluetooth state on server switch

## [2026.04.31]

### Fixed
- mDNS device ID is now persisted across restarts — a 64-bit hex ID is generated once and cached in `~/.config/rockbox.org/device-id`, so the registered mDNS service name remains stable between daemon restarts instead of changing on every launch

## [2026.04.30]

### Added
- Bluetooth device support in the GPUI and web UIs — list paired/discovered devices, connect and disconnect directly from the device picker
- mDNS-based server discovery and runtime server switching — `scan_mdns()` in the daemon registers itself via mDNS; the GPUI app and macOS app gain a Server Picker UI that enumerates nearby `rockboxd` instances and switches without restart; a notification triggers one-shot syncs to re-run on server change
- UPnP album art saved for remote tracks — `album_art_uri` is returned from UPnP directory listings; `save_audio_metadata` downloads and caches the cover when no embedded art is present; remote metadata is persisted concurrently (semaphore-limited) without blocking C/FFI
- `copyright_message` field on the `Album` GraphQL type, displayed in `AlbumDetails` alongside a formatted release date
- Typesense bundled in the Docker image — the Dockerfile now pulls the typesense image and copies `typesense-server` into the final image

## [2026.04.29-2]

### Added
- Bluetooth speaker management commands in the `rockbox` CLI (`bluetooth scan`, `bluetooth devices`, `bluetooth connect <address>`, `bluetooth disconnect <address>`) — Linux only, talks to a running `rockboxd` via gRPC
- Bluetooth GraphQL resolvers (`bluetoothDevices` query, `bluetoothScan` / `bluetoothConnect` / `bluetoothDisconnect` mutations) now call `rockbox-bluetooth` directly instead of going through the HTTP server — eliminates an extra round-trip on Linux

### Fixed
- `BluetoothService` gRPC RPC renamed from `Connect` to `ConnectDevice` to avoid a name collision with tonic's auto-generated transport `connect` constructor, which caused a compile error (`duplicate definitions with name connect`)

## [2026.04.29-1]

### Fixed
- macOS app Files view: navigating from the root into Music no longer yields an empty list — `.task` ID now encodes both mode and path so a mode change with a nil path correctly triggers a reload
- macOS app device picker: now lists all output devices (including the current one, marked with a checkmark) instead of only non-current devices; added `snapcast` icon/colour entry
- macOS app device picker: no longer shows a loading spinner on open when devices were already preloaded at startup — `refresh()` only sets `isLoading` when the device list is empty

## [2026.04.29]

### Added
- UPnP device browsing in the Files view — queue and play tracks directly from any UPnP/DLNA media server on the local network

### Fixed
- HTTP stream (`netstream`) no longer permanently breaks after a failed seek: `seek_to()` now only replaces the active response on success, so a failed Range request leaves the stream readable at the current position
- Small forward seeks (≤ 128 KB) in HTTP streams are now satisfied by skipping bytes in the existing response body instead of issuing a new Range request, avoiding unnecessary round-trips during codec metadata parsing
- Buffering: `TYPE_ID3` handles for remote tracks that fail to open now send `BUFFER_EVENT_FINISHED` with an empty `mp3entry` instead of silently never posting `Q_AUDIO_FINISH_LOAD_TRACK`, which caused the track-loading chain to stall on playlist restore with many queued UPnP tracks
- Web UI Files view: Music and UPnP Devices row icons no longer disappear on hover — CSS selector changed from descendant (` `) to direct-child (`>`) combinator so the `.no-play` guard is respected

## [2026.04.28-1]

### Added
- Real-time PCM loudness normalizer (`normalize_volume = true` in `settings.toml`) — RMS-based AGC with asymmetric attack/release, similar to Spotify's "Normalize Volume"; applied across all PCM sinks (SDL, FIFO, AirPlay, Squeezelite, UPnP, Chromecast, Snapcast TCP)
- `GET /player/volume` REST endpoint returning `{ volume, min, max }`
- `volume` GraphQL query returning live current volume with min/max range
- `useGetVolumeQuery` GraphQL hook in the web UI
- `get_current_volume()` gRPC client helper in the GPUI app

### Fixed
- Volume slider in GPUI mini-player now responds to mouse clicks (replaced plain `div` with `SeekBar` component)
- Volume slider in web UI now uses correct 0–100 range with explicit `min`/`max` on the MUI Slider
- `globalSettings.volume` in GraphQL now returns the live current volume via `rb::sound::current(0)` instead of a hardcoded `0`
- `VOLUME_MIN_DB` constant in GPUI corrected from `-74` to `-80` (SDL target range)
- Volume in GPUI loads the live value at startup via `SoundCurrent` gRPC instead of the stale saved setting
- `adjust_volume` now has audible effect on all non-SDL PCM sinks (FIFO, AirPlay, Squeezelite, UPnP, Chromecast, Snapcast TCP) — SW volume scaling (`pcm_copy_buffer`) was not being applied in any of these sinks

## [2026.04.28]

### Added
- Snapcast TCP PCM sink (`audio_output = "snapcast_tcp"`) — streams S16LE PCM directly to a Snapcast `tcp://` source; compatible with snapserver v0.35+
- Stream metadata forwarding for Snapcast TCP sink

### Fixed
- MPD `getvol` / `setvol` handlers now correctly map the Rockbox dB range (−80..0) to the MPD 0–100 scale

## [2026.04.27]

### Added
- TypeScript SDK (`@rockbox/sdk`) for controlling rockboxd from Node.js / browser applications
- Playlists UI in the web interface — create, edit, and manage saved and smart playlists
- Album art footer overlay shown on album cover hover

### Changed
- Web UI data layer migrated from Apollo Client to TanStack React Query
- Playlist modals rendered into document body via React portal (fixes z-index stacking issues)

## [2026.04.26]

### Added
- Chromecast PCM sink (`audio_output = "chromecast"`) — streams WAV over HTTP and controls playback via the Cast Media protocol
- UPnP/DLNA support: ContentDirectory media server (`upnp_server_enabled`), MediaRenderer:1 (`upnp_renderer_enabled`), and UPnP PCM sink (`audio_output = "upnp"`) with auto-renderer discovery
- Device picker UI in the GPUI and web mini-player — switch audio output (Rockbox built-in, AirPlay, Squeezelite, Chromecast) without restarting
- Multi-room AirPlay: `airplay_receivers` list in `settings.toml` supports sending to multiple RAOP receivers simultaneously
- Squeezelite multi-room PCM sink (`audio_output = "squeezelite"`) — Slim Protocol TCP server + HTTP PCM broadcast; supports unlimited concurrent squeezelite clients with independent reader cursors

### Fixed
- Duplicate Chromecast devices skipped during discovery
- Typesense search index initialised before the HTTP server accepts requests (avoids empty results on cold start)

## [2026.04.25]

### Added
- Saved playlists: create, rename, delete, and reorder tracks via gRPC, GraphQL, and REST APIs
- Smart playlists: rule-based auto-generated playlists with play-count and skip-count tracking
- Playlist search integration with Typesense
- `StreamLibrary` gRPC server-streaming RPC — pushes library updates to clients when a scan completes
- GPUI file browser: navigate the local filesystem and enqueue directories directly

### Fixed
- Now Playing widget in GPUI shows correctly when the app opens with a paused track (initial status fetched once at startup)
- Rocksky registration failures logged at `debug` level instead of `warn` to reduce noise
- Global Play/Pause keybind no longer fires when a text input field has focus
