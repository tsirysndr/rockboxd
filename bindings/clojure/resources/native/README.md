# Bundled native libraries

At publish time this directory holds the prebuilt `librockbox_ffi` for every
supported OS/arch, one per subdirectory:

```
native/darwin-arm64/librockbox_ffi.dylib
native/darwin-x64/librockbox_ffi.dylib
native/linux-x64/librockbox_ffi.so
native/linux-arm64/librockbox_ffi.so
native/freebsd-x64/librockbox_ffi.so
native/netbsd-x64/librockbox_ffi.so
```

They are **not** checked in — stage them from a GitHub release before building
the jar:

```sh
bindings/scripts/fetch-libs.sh --all      # downloads every target here
```

At runtime `rockbox.ffi/extract-bundled` picks `native/<os>-<arch>/…` for the
running JVM, extracts it to a temp file, and loads it. `ROCKBOX_FFI_LIB` still
overrides, and a repo checkout falls back to `target/release`.
