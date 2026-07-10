// RockboxFFI — self-contained, statically linked product.
//
// This target `-force_load`s librockbox_ffi.a at build time (see Package.swift),
// so the entire Rockbox DSP / metadata / playback engine — Rust glue plus the
// compiled C firmware objects — is baked into the consumer's binary. Nothing
// is dlopen'd and no ROCKBOX_FFI_LIB / target-release lookup happens at
// runtime; the loader finds the symbols already present in the process image.
//
// The public API (Dsp, Player, Metadata, enums, …) lives in RockboxFFICore and
// is re-exported here, so `import RockboxFFI` is all a consumer needs.
@_exported import RockboxFFICore
