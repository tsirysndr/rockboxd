# Rockbox CI in PureScript

The pipeline is a Dagger graph written in PureScript. PureScript owns the
branches, loops, toolchain composition, firmware preparation, artifact selection,
version stamping and packaging. Commands such as `cargo`, `make`, `zig`, `tar`
and `dpkg-deb` receive argument arrays directly; there is no CI shell dispatcher,
inline `bash -c`, or `.dagger/ci/scripts` directory.

This uses Tsiry Sandratraina's **personal, experimental, unofficial PureScript
SDK**, not an official Dagger SDK. The [vendored SDK](sdk/purescript/UPSTREAM.md)
and generated beta.21 bindings are checked in, so a fresh checkout does not
need the sibling SDK repository or a code-generation step.

## Run a job

Clone with recursive submodules, install [mise](https://mise.jdx.dev/), then:

```sh
mise trust
mise install
mise exec -- dagger api call -m .dagger/ci --help
mise exec -- dagger api call -m .dagger/ci linux-check --source . export --path artifacts
mise exec -- dagger api call -m .dagger/ci linux-release \
  --source . --platform linux/arm64 --tag 2026.10.26 export --path artifacts
```

`mise.toml` pins **Dagger 1.0.0-beta.21**, with download checksums. No 0.x CLI,
FluentCI, pkgx or WASM runner is needed. These commands execute real container
builds and require a Dagger engine. They have not been run locally during this
refactor. Select a remote engine through your normal Dagger CLI configuration.

Most functions accept `--source .` and optional `--platform linux/amd64` or
`linux/arm64`. Build functions return a `Directory`; export it wherever you
want. Tests return a success report only after their final graph node succeeds.
Inputs are typed module arguments, not a bag of shell environment variables.

Source directory exclusions are declared on module arguments so the CLI filters
host files before upload, then applied again inside the graph. Native Android
checks upload only the SDK's `ndk/` subtree; APK builds omit emulator images,
sources, extras, and docs while retaining Android build tools.

The PureScript client uses Node's HTTP transport without a default request
deadline, allowing a GraphQL request to evaluate a long build. Explicit positive
`timeoutMs` values still impose a deadline, and cancellation aborts the request.
Both the module runtime and build toolchain use Node 24.

`format` reads Cargo metadata and checks every Rockbox workspace package with
explicit `--package` arguments. It excludes packages inside the `deno/` and
`rmpc/` submodules, including generated protobuf code. Cargo automatically
includes some path dependencies as workspace members, so neither `--all` nor
an unqualified `cargo fmt` provides that boundary. Formatting differences in
Rockbox-owned packages still fail the check.

| Function | Output / purpose |
| --- | --- |
| `linux-check` | Headless daemon + CLI binaries |
| `build` | Combined headless daemon + CLI tarball and checksum |
| `linux-release` | Linux archives + embedded library; amd64 deb/rpm with desktop, or arm64 deb |
| `linux-armhf-release` | ARMv6 hard-float daemon archive and deb |
| `embed` | Embedded daemon archive and C header |
| `desktop` | Slint desktop binary, README and skins archive |
| `format`, `web-test`, `go-test` | Rust formatting, Vitest, Go vet/tests |
| `ffi`, `nif` | Linux FFI libraries or Erlang OTP 27 NIF |
| `android-check` | Android native embedded-daemon library |
| `android-apk`, `android-release` | Release APK |
| `android-ffi` | Android FFI library for `--abi arm64-v8a` or `x86_64` |
| `ruby`, `python`, `npm` | Package native binding artifacts |
| `sdl-build` | Installed SDL player/daemon, codecs and assets |
| `release` | Four SDL archives and checksums |
| `deb`, `rpm` | Repackage an existing four-archive SDL release |
| `electron`, `storybook` | Electron Linux packages / static Storybook |
| `image` | Multi-platform amd64 + arm64 OCI archive (`File`) |
| `nix-build` | Nix store closure and `store-paths` manifest |
| `nix-consume` | Published-flake installation verification log (`String`) |

### Android and binding packages

Android requires `--android-sdk` pointing at a **Linux amd64 SDK with a
side-by-side NDK**, including when the engine runs remotely. GitHub Ubuntu
runners supply `ANDROID_SDK_ROOT`. PureScript selects the latest installed NDK
numerically and constructs the container paths. JDK 17 and cargo-ndk are
installed in the toolchain container.

```sh
mise exec -- dagger api call -m .dagger/ci android-apk \
  --source . --android-sdk "$ANDROID_SDK_ROOT" \
  --features embedded-daemon --architectures arm64-v8a export --path artifacts

mise exec -- dagger api call -m .dagger/ci python \
  --source . --artifacts native-artifacts --version 1.2.3 export --path packages
```

Binding artifacts retain download directories such as
`librockbox_ffi-linux-x64/librockbox_ffi.so`. PureScript iterates the supported
platforms and skips missing optional artifacts, as before. Package manifests
are stamped with PureScript string/JSON operations; there are no sed commands
or Node/Python scripts performing CI orchestration.

### Releases, images and Nix

Use a numeric `--tag` for deb/rpm metadata. The `release`, `deb` and `rpm`
functions preserve the old SDL release format: separate CLI, daemon, codecs
and assets archives. These differ from the combined headless archives produced
by `linux-release`. Downloaded SDL release archives are checksum-verified.

```sh
mise exec -- dagger api call -m .dagger/ci image \
  --source . --tag 2026.10.26 export --path artifacts/rockbox-image.tar
mise exec -- dagger api call -m .dagger/ci nix-build \
  --source . --target '.#rockboxd' export --path artifacts/nix
mise exec -- dagger api call -m .dagger/ci nix-consume \
  --ref 'git+https://github.com/tsirysndr/rockboxd?submodules=1'
```

Image construction and ARMhf cross-toolchain preparation now use the typed
Dagger API directly, without Dockerfile build calls or nested Docker. The
application's image entrypoint remains its runtime supervisor. Nix builds
return a store closure for the host's existing Cachix publisher; Rust staticlibs
are imported/published before the final target, retaining that cache on failure.
A missing platform-specific Nix attribute is skipped; other failures propagate.

GitHub workflows call the module functions directly. Native macOS/iOS/BSD
builds, GPUI/DMGs, release creation and uploads, Gemfury, registry publication
and Cachix publication retain their host jobs and credentials. The disabled
Electron, Chromatic and SDL release workflows remain disabled. Application
build tools (Make, Gradle, package-manager tasks, the existing binary archive
parser) remain part of the project; CI no longer delegates to its shell build
wrappers. Existing standalone developer scripts remain independently usable.

## Develop and check without an engine

```sh
rustup component add rustfmt
npm install --global purescript@0.15.16 spago@1.0.5
cd .dagger/ci
spago build -p rockbox-ci --pedantic-packages
node --test checks/*.test.mjs
cd ../..
python3 -m pip install PyYAML==6.0.3
python3 .dagger/check.py
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 -shellcheck= .github/workflows/*.yml
```

The graph tests inspect the actual compiled PureScript functions with network
access disabled. A Cargo fixture also reproduces submodule auto-enrollment and
checks that owned formatting errors still fail. Tests verify direct command arguments, artifact paths, source
filtering and input validation. In particular, they preserve **all of
`firmware/target`**, which is source code, while excluding the specific Cargo
build-output directories. This prevents the missing `filesystem-app.h` and
`headless.make` failure caused by a recursive `**/target` exclusion. The module
also checks those files before preparing toolchains.

`dagger-check.yml` runs compilation, graph tests, configuration/wiring checks
and actionlint. These checks do not replace end-to-end container builds.

| Module | Responsibility |
| --- | --- |
| `Main.purs` | Typed module functions, defaults and arguments |
| `CI.Runner` / `CI.Job` | Typed pipeline dispatch and options |
| `CI.Toolchain` | Toolchain images, packages and binary composition |
| `CI.Base` / `CI.Graph` | Source filtering, argv execution, file operations and artifacts |
| `CI.Build` / `CI.Firmware` | Linux builds, codec handling, embedded archives and packages |
| `CI.Android` / `CI.Bindings` | Android builds and binding packages |
| `CI.Format` | Cargo metadata selection and formatting checks |
| `CI.Legacy` | SDL releases and their package formats |
| `CI.Image` / `CI.Nix` | OCI images and Nix builds |

Keep generated bindings and `spago.lock` committed. See the SDK's `UPSTREAM.md`
for updates. The former pipeline's MIT license is retained in `LICENSE`;
Rockbox and vendored code retain their existing licenses and notices.
