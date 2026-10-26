# Rockbox CI with PureScript and Dagger

Build logic lives here. GitHub Actions selects runners, calls the module, and
uploads the returned artifacts. You can call the same functions from your own
checkout without GitHub or FluentCI credentials.

This uses Tsiry Sandratraina's **personal, experimental, unofficial PureScript
SDK**. It is not an official Dagger SDK. A reviewed copy is vendored under
[`sdk/purescript`](sdk/purescript/UPSTREAM.md), together with the generated beta.21
client in [`ci/dagger-sdk`](ci/dagger-sdk). A fresh checkout needs no sibling SDK
repository and no code-generation step.

## Start here

Clone with submodules (`git submodule update --init --recursive` for an existing
checkout), install [mise](https://mise.jdx.dev/), then:

```sh
mise trust
mise install
.dagger/run web-test
.dagger/run linux-x86_64-check
PLATFORM=linux/arm64 .dagger/run linux-aarch64-check
TAG=2026.10.26 .dagger/run linux-x86_64-release
```

The root `mise.toml` pins **Dagger 1.0.0-beta.21**, including download checksums.
Do not use a 0.x CLI. Running a job requires access to a Dagger engine; these
commands start actual builds. They were not executed locally during this migration.

The wrapper exports into `artifacts/`. `SOURCE`, `OUTPUT`, `PLATFORM`, and `TAG`
can override its defaults. It works from any directory. A named ARM64 check or
release defaults to `linux/arm64`; generic jobs default to `linux/amd64`.

For the typed module API and argument help:

```sh
mise exec -- dagger api call -m .dagger/ci --help
mise exec -- dagger api call -m .dagger/ci linux-release \
  --source . --platform linux/arm64 --tag 2026.10.26 export --path artifacts
mise exec -- dagger api call -m .dagger/ci embed \
  --source . --platform linux/arm64 --tag 2026.10.26 export --path artifacts
```

## Jobs and outputs

Every build/test function returns a `Directory`, including a success report in
`.dagger-results/`. The image function returns an OCI `File`; `nix-consume`
returns its verification log. Failure stops the call before export.

| Wrapper job | Module function | Result |
| --- | --- | --- |
| `build` | `build` | Headless daemon + CLI release tarball and checksum |
| `linux-x86_64-check`, `linux-aarch64-check` | `linux-check` | `target/release/rockbox`, `zig/zig-out/bin/rockboxd` |
| `linux-x86_64-release` | `linux-release` | CLI/daemon + embedded-library archives, desktop-inclusive deb/rpm, checksums |
| `linux-aarch64-release` | `linux-release` | CLI/daemon + embedded-library archives, arm64 deb, checksums |
| `linux-armhf-release` | `linux-armhf-release` | ARMv6 hard-float daemon archive and deb |
| `embed` | `embed` | `librockboxd` archive with C header |
| `desktop` | `desktop` | Slint desktop binary, README and skins archive |
| `format`, `web-test`, `go-test` | `format`, `web-test`, `go-test` | Rust formatting, Vitest, Go vet/tests respectively |
| `ffi`, `nif` | `ffi`, `nif` | Native Linux FFI libraries or Erlang OTP 27 NIF |
| `android-check` | `android-check` | Embedded-daemon Android native library |
| `android-apk`, `android-release` | `android-apk`, `android-release` | Release APK |
| `android-ffi` | `android-ffi` | Android FFI shared library |
| `package-ruby`, `package-python`, `package-npm` | `ruby`, `python`, `npm` | Gems, wheels/sdist, npm tarballs from downloaded native artifacts |
| `sdl` | `sdl-build` | Installed SDL player/daemon, codecs and assets under `runtime/usr/local` |
| `legacy-release` | `release` | Four SDL archives (CLI, daemon, codecs, assets) and checksums |
| `deb`, `rpm` | `deb`, `rpm` | Repackage a published four-archive SDL release |
| `electron`, `storybook` | `electron`, `storybook` | Electron Linux packages or static Storybook |
| `image`, `build_docker`, `build_cloud` | `image` | Multi-platform amd64/arm64 OCI archive |

`deb` builds both amd64 and arm64 packages; `rpm` preserves the legacy x86_64
scope. They download and checksum-verify the four-archive release format,
which is different from the combined archives made by the live Linux release
jobs. Set `TAG` to an existing numeric release version, not `latest`. Their SDL
package manifests do not require Slint or Typesense files. The live Linux
release functions use the project's current `dist/` packaging definitions.

The old plugin's `build`, `release`, `deb`, `rpm`, `build_docker`, `build_cloud`
and `publish` capabilities are covered. Use `sdl` for the old SDL build; `build`
is the current headless release. `.fluentci` and both old `dagger.json` files
have been removed. Application Deno and its submodule remain part of Rockbox.

### Android

Supply a Linux Android SDK with a side-by-side NDK. GitHub's Ubuntu runners
already provide this through `ANDROID_SDK_ROOT`; local macOS SDK/NDK binaries
cannot be used in the Linux container. JDK 17 and the Rust Android targets are
installed inside the toolchain container. NDK paths are resolved after copying
the SDK into the container, so host absolute paths do not leak into the build.

```sh
ANDROID_SDK_ROOT=/path/to/linux/android-sdk .dagger/run android-check
ANDROID_SDK_ROOT=/path/to/linux/android-sdk \
  FEATURES=embedded-daemon ARCHITECTURES=arm64-v8a,x86_64 .dagger/run android-apk
ANDROID_SDK_ROOT=/path/to/linux/android-sdk ABI=x86_64 .dagger/run android-ffi
```

### Binding packages

Download the native artifacts into `artifacts/`, preserving directories such as
`librockbox_ffi-linux-x64/`. Then:

```sh
ARTIFACTS=artifacts BINDING_VERSION=1.2.3 OUTPUT=packages .dagger/run package-python
```

Only packaging happens here. Native macOS, iOS, FreeBSD and NetBSD libraries
are still produced on their corresponding runners/VMs. Missing optional
platforms retain the existing packaging scripts' skip behavior.

### Images and publishing

```sh
TAG=2026.10.26 .dagger/run image
# Explicit publication, using credentials from your existing registry login:
TAG=2026.10.26 IMAGE=ghcr.io/your-account/rockbox:2026.10.26 .dagger/run publish
```

`publish` requires `skopeo`; it builds the OCI archive with Dagger and copies
all platform variants to `IMAGE`. The GitHub Docker workflow uses the same
archive, existing Docker login actions, and Skopeo to publish version + latest
tags to both registries. Credentials never enter the build container.

`build_cloud` is an alias for the same image build. Configure your Dagger CLI
for a remote engine to run it remotely; there is no Docker Build Cloud,
Buildx plugin, pkgx, WASM or FluentCI runner dependency. Standard project
Dockerfiles and standalone developer scripts remain usable independently.

### Nix

```sh
mise exec -- dagger api call -m .dagger/ci nix-build \
  --source . --target '.#rockboxd' export --path artifacts/nix
mise exec -- dagger api call -m .dagger/ci nix-consume \
  --ref 'git+https://github.com/tsirysndr/rockboxd?submodules=1'
```

Linux Nix builds return `store/` plus `store-paths`. Actions imports that closure
and publishes it through its existing Cachix credentials. It builds and imports
`rockbox-rustlibs` first, preserving that cache even if the final link fails.
Containers use the public Rockbox cache without a token. macOS Nix jobs stay
native. A missing platform-specific attribute is skipped; other evaluation
errors still fail.

## What remains in Actions

The existing triggers, release tags, platform matrices, artifact names and
upload destinations are retained. macOS/Intel/iOS, BSD VMs, GPUI/DMG builds,
release creation, Gemfury, registry publication and Cachix publication stay on
the host. The disabled Electron, Chromatic and SDL release workflows remain
disabled, but now call the new module and publish without FluentCI.

ARMhf runs the existing cross-toolchain Dockerfile directly inside Dagger.
Its firmware steps, codec symbol rewriting, ARMv6 flags and glibc compatibility
shim are preserved; there is no nested Docker daemon or `cross` invocation.
Linux native release builds retain Ubuntu 24.04. Toolchains use Zig 0.16.0,
Go 1.26.2, Bun 1.4.3, Node 22, Ruby 3.3, Python 3.12 and Nix 2.31.2. Rust stable,
Deno 2 and OTP 27 track the same release lines as the former workflows.

## Develop the module without running Dagger

```sh
npm install --global purescript@0.15.16 spago@1.0.5
(cd .dagger/ci && spago build -p rockbox-ci --pedantic-packages)
python3 -m pip install PyYAML==6.0.3
python3 .dagger/check.py
python3 .dagger/test_adapter.py
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.12 -shellcheck= .github/workflows/*.yml
```

`dagger-check.yml` runs these checks. They compile the module, validate Bash and
TOML, check every job's script and workflow wiring, and lint Actions. They do
**not** prove that a container build, SDK runtime, Android package, Nix closure
or release upload works end to end; the first CI executions provide that check.

- `ci/src/Main.purs`: idiomatic `Module.function` declarations with typed records,
  defaults, `Directory` inputs and `Aff` handlers.
- `ci/src/CI/Job.purs`: closed job set and toolchain selection.
- `ci/src/CI/Runner.purs`: source filtering, toolchains, caches and artifact values.
- `ci/src/CI/Image.purs`, `CI/Nix.purs`: container image and Nix functions.
- `ci/scripts/`: reviewed build/package commands moved out of workflow YAML.
- `ci.sh`: host argument adapter; `run`: local mise convenience wrapper.

Keep generated bindings and `spago.lock` checked in. See the vendored SDK's
`UPSTREAM.md` for updates. The former pipeline’s MIT license is retained in `LICENSE`;
Rockbox keeps its existing license, and vendored code retains its own notices.
