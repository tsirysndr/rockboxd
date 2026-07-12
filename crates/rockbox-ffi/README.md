# rockbox-ffi

![Rust](https://img.shields.io/badge/Rust-cdylib%20%2B%20staticlib-000000?logo=rust&logoColor=white)
![C ABI](https://img.shields.io/badge/C%20ABI-stable%20v1-A8B9CC?logo=c&logoColor=white)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

A flat **C ABI** over three Rockbox-derived Rust crates, built as a
`cdylib` + `staticlib` so the engine can be driven from any language with a
C FFI:

| Crate                                                     | What it gives you                                                   |
| --------------------------------------------------------- | ------------------------------------------------------------------- |
| [`rockbox-dsp`](../rockbox-dsp)                           | EQ, tone, crossfeed, compressor, ReplayGain, resampler DSP pipeline |
| [`rockbox-metadata`](../rockbox-metadata)                 | Tag / duration / ReplayGain / album-art parser for 40+ formats      |
| [`rockbox-playback`](../rockbox-playback)                 | Queue-based player with native ReplayGain, full DSP chain & crossfade |

The public surface is declared in [`include/rockbox_ffi.h`](../../include/rockbox_ffi.h).

### Playback surface

`rb_player_*` covers the whole `rockbox-playback` feature set:

- **Transport / queue** — `play` / `pause` / `toggle` / `stop`, `next` /
  `previous` / `skip_to`, `seek_ms`, `set_volume`, `set_queue_json`,
  `enqueue`, `queue_json`.
- **Rockbox queue insertion** — `rb_player_insert_json(json, position, index)`
  with the full position set (0 prepend, 1 insert, 2 insert-next,
  3 insert-last, 4 shuffled, 5 last-shuffled, 6 replace, 7 explicit index).
- **DSP chain** — the whole Rockbox pipeline past ReplayGain:
  `rb_player_set_eq_enabled` / `is_eq_enabled` / `set_eq_band` /
  `set_eq_precut` / `set_eq_preset` (21 built-in presets), `set_tone` /
  `set_bass` / `set_treble`, `set_surround`, `set_channel_mode` /
  `set_stereo_width`, `set_compressor`, `set_dither`, `set_pitch` — plus
  `rb_player_dsp_settings_json` to read the whole state back.
- **HTTP remote media** — queue `http(s)://` URLs beside local paths; finite
  files stream on demand via range requests, and unbounded **live radio**
  decodes on the fly. For live radio the status JSON's `metadata` carries the
  ICY now-playing info (`title` / `artist`, `album` = station, `bitrate`,
  `sample_rate`), refreshed as songs change.
- **Resume** — build with `rb_player_new_with_config_ex(…, resume_file,
  interval_ms)`, then `rb_player_resume` / `save_resume` / `clear_resume`;
  `rb_load_resume_json(path)` peeks without a player.
- **`.m3u` / `.m3u8`** — `rb_player_import_m3u` / `load_m3u` / `export_m3u`,
  plus standalone `rb_m3u_read_json` / `rb_m3u_write_json`; `rb_is_url(s)`
  classifies an entry.

Rich values (status, metadata, queue, resume state, m3u entries) come back as
**JSON** C strings — free them with `rb_string_free`.

## Build

```sh
cargo build --release -p rockbox-ffi
# → target/release/librockbox_ffi.dylib   (macOS, for dlopen)
# → target/release/librockbox_ffi.so      (Linux)
# → target/release/librockbox_ffi.a       (static, for embedding / NIFs)
```

## Conventions

- **Handles** (`RbDsp*`, `RbPlayer*`) are opaque pointers; every `*_new` has
  a matching `*_free`. Passing `NULL` to a `*_free` or setter is a safe no-op.
- **Strings** returned by `*_json` / `*_probe` are heap-allocated JSON (or a
  label) — free them with `rb_string_free()`.
- **Sample buffers** returned by `rb_dsp_process` are heap-allocated `int16`
  — free them with `rb_buffer_free(ptr, out_len)`.
- **Optional floats** use `NaN` for "absent".
- Rich values (metadata, player status) are returned as **JSON** — the
  lowest common denominator across binding languages.

## Language bindings

See [`bindings/`](../../bindings) for Python (cffi), Go (purego), TypeScript
(Bun/Deno FFI), Ruby, Swift, Elixir, Gleam, Kotlin, and Clojure packages that
consume this library.

## ABI stability

`rb_ffi_abi_version()` returns the ABI major version (currently `1`). Bump it
on any breaking change to a signature or JSON shape.
