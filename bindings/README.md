# Language bindings

Bindings for the Rockbox **DSP**, **metadata**, and **playback** engine in
several languages. All of them sit on top of one shared flat **C ABI**,
[`rockbox-ffi`](../crates/rockbox-ffi) (`crates/rockbox-ffi`), whose public
surface is declared in [`include/rockbox_ffi.h`](../include/rockbox_ffi.h).

```
                 crates/rockbox-ffi  (cdylib + staticlib)
             rb_dsp_* / rb_meta_* / rb_player_*   ← include/rockbox_ffi.h
                              │
   ┌───────────┬─────────────┼──────────────┬───────────────┐
 Python      TypeScript    Elixir         Gleam          (your language)
 (cffi)   (Bun/Deno/Node)  (erl_nif) ───── (erl_nif)      C FFI
                                └── shared rockbox_ffi_nif.{c,erl} ──┘
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
| TypeScript     | [`typescript/`](typescript)    | Bun / Deno / Node.js FFI      | `bun run examples/smoke.bun.ts`                   |
| Elixir         | [`elixir/`](elixir)            | `erl_nif` (shared)            | `mix test`                                         |
| Gleam          | [`gleam/`](gleam)              | `erl_nif` (shared)            | `make && gleam test`                              |

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
