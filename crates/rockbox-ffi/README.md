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
| [`rockbox-playback`](../rockbox-playback)                 | Queue-based player with native ReplayGain and Rockbox crossfade     |

The public surface is declared in [`include/rockbox_ffi.h`](../../include/rockbox_ffi.h).

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

See [`bindings/`](../../bindings) for Python (cffi), TypeScript (Bun/Deno
FFI), Elixir, and Gleam packages that consume this library.

## ABI stability

`rb_ffi_abi_version()` returns the ABI major version (currently `1`). Bump it
on any breaking change to a signature or JSON shape.
