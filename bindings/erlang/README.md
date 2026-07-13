# rockbox_ffi_nif

Shared Erlang NIF over the [`librockbox_ffi`](../../crates/rockbox-ffi) C ABI —
the Rockbox DSP / metadata / codecs / playback engine exposed to the BEAM.

This is the **common native layer** for the language bindings that run on the
BEAM. The [Elixir](../elixir) (`rockbox_ex_ffi`) and [Gleam](../gleam)
(`rockbox_ffi`) packages both depend on this one instead of each vendoring
their own copy of the C shim and Erlang loader; the ergonomic wrappers live in
those packages.

## What's here

| Path                          | Role                                                            |
| ----------------------------- | --------------------------------------------------------------- |
| `c_src/rockbox_ffi_nif.c`     | erl_nif shim — a 1:1 surface of `include/rockbox_ffi.h`          |
| `src/rockbox_ffi_nif.erl`     | NIF loader + stub module; downloads the `.so` on first load     |
| `priv/rockbox_ffi_nif.manifest` | per-triple sha256 checksums for the first-load download       |
| `Makefile`                    | local-dev build of `priv/rockbox_ffi_nif.so`                    |

## NIF delivery

The compiled NIF is a multi-megabyte shared object (it statically links the
Rust `librockbox_ffi.a`), too large to bundle in a Hex tarball. So the Hex
package ships only the checksum manifest, and `rockbox_ffi_nif.erl`'s `-on_load`
hook downloads the matching `rockbox_ffi_nif-<triple>.so` from the GitHub
release named in the manifest into the user cache on first use, verifying it
against the manifest sha256. This is why both the Elixir and Gleam bindings get
their native code the same way.

## Local development

Inside a full monorepo checkout (needs the Cargo workspace + `include/` header):

```sh
# 1. Build the Rust static archive.
cargo build --release -p rockbox-ffi

# 2. Build the NIF into priv/rockbox_ffi_nif.so.
cd bindings/erlang && make

# 3. Compile the Erlang app.
rebar3 compile
```

The Elixir/Gleam bindings pick up this locally-built `.so` via a path dependency
on `../erlang` — the loader prefers a local `priv/*.so` over any cached download.

## Supported prebuilt targets

`aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-linux-gnu`,
`x86_64-linux-gnu`, `x86_64-unknown-freebsd`, `x86_64-unknown-netbsd`.
Requires OTP 27+ (the bindings decode JSON with the built-in `json` module).
Other platforms build from source (steps above).
