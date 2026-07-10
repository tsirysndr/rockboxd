// RockboxFFIDynamic — runtime-loaded product.
//
// Links nothing at build time. The librockbox_ffi shared library is located
// and `dlopen`ed at runtime (ROCKBOX_FFI_LIB env var, else a `target/release`
// walk, else a bundled `Libs` dir), exactly like the Ruby / Python bindings.
// Use this when you'd rather ship the native library alongside your app than
// bake it into the binary.
//
// The public API (Dsp, Player, Metadata, enums, …) lives in RockboxFFICore and
// is re-exported here, so `import RockboxFFIDynamic` is all a consumer needs.
@_exported import RockboxFFICore
