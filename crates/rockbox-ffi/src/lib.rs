//! `rockbox-ffi` — a flat C ABI over [`rockbox-dsp`], [`rockbox-metadata`]
//! and [`rockbox-playback`], built as a `cdylib`/`staticlib` so it can be
//! driven from any language with a C FFI (Python via cffi, TypeScript via
//! Bun/Deno FFI, Elixir/Gleam via an `erl_nif` shim, …).
//!
//! Conventions across the whole surface:
//! - Handles are opaque pointers; each `*_new` has a matching `*_free`.
//!   Passing null to a `*_free` or setter is a safe no-op.
//! - Rich values are returned as heap-allocated JSON C strings — free them
//!   with [`rb_string_free`]. `i16` sample buffers use [`rb_buffer_free`].
//! - Optional `f32` inputs use `NaN` for "absent"; enum inputs are small
//!   integers documented on each function.
//!
//! The public C declarations live in `include/rockbox_ffi.h`.

mod dsp;
mod meta;
mod player;
mod util;

pub use dsp::*;
pub use meta::*;
pub use player::*;
pub use util::*;

/// ABI version of this library. Bump the major on any breaking change to a
/// function signature or the JSON shapes.
#[no_mangle]
pub extern "C" fn rb_ffi_abi_version() -> u32 {
    1
}
