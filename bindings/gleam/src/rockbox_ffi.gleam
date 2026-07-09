//// Gleam bindings for the Rockbox DSP / metadata / playback engine.
////
//// The interesting modules are `rockbox/metadata`, `rockbox/dsp`, and
//// `rockbox/player`. This module exposes the ABI version.

/// ABI major version of the loaded native library.
@external(erlang, "rockbox_ffi_nif", "abi_version")
pub fn abi_version() -> Int
