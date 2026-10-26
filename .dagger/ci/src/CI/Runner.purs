module CI.Runner where

import Prelude
import CI.Android as Android
import CI.Base as Base
import CI.Bindings as Bindings
import CI.Build as Build
import CI.Firmware as Firmware
import CI.Format as Format
import CI.Graph as G
import CI.Job (Job(..))
import CI.Job as Job
import CI.Legacy as Legacy
import CI.Toolchain as T
import Data.Array as A
import Data.Maybe (Maybe(..))
import Data.String.CodeUnits as CU
import Dagger (Client)
import Dagger.Container as C
import Dagger.Directory as D
import Dagger.Query as Q
import Dagger.Types (Container, Directory)
import Effect.Aff (Aff)

type Input = { source :: Directory, platform :: String }
type Options = { tag :: String, features :: String, architectures :: String, abi :: String, version :: String }

defaults :: Options
defaults = { tag: "latest", features: "embedded-daemon", architectures: "arm64-v8a", abi: "arm64-v8a", version: "0.0.0" }

validVersion :: String -> Boolean
validVersion value = value /= "" && A.all (\c -> (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || A.elem c [ '.', '+', '-', '_', '~' ]) (CU.toCharArray value)

run :: Job -> Client -> Input -> Options -> Maybe Directory -> Maybe Directory -> Aff Directory
run job client input options artifacts androidSdk = do
  G.ensure (validVersion options.tag && validVersion options.version) "Invalid release version"
  let family = Job.toolchain job
  platform <- Base.platform (if family == T.Cross then "linux/amd64" else input.platform)
  G.ensure (family /= T.Android || input.platform == "linux/amd64") "Android requires a Linux amd64 SDK/NDK"
  let
    arch = if input.platform == "linux/arm64" && family /= T.Cross then "aarch64" else "x86_64"
    source = Base.sourceTree client input.source
  -- 'firmware/target' contains source code, despite sharing Cargo's output name.
  header <- D.exists "firmware/target/hosted/filesystem-app.h" source
  makefile <- D.exists "firmware/target/hosted/headless/headless.make" source
  G.ensure (header && makefile) "Source filter removed firmware/target; required hosted firmware files are missing"
  let
    prepared = T.prepare client platform arch source family
      # C.withMountedCache "/usr/local/cargo/registry" (Q.cacheVolume "rockbox-cargo-registry" client)
      # C.withMountedCache "/usr/local/cargo/git" (Q.cacheVolume "rockbox-cargo-git" client)
      # C.withMountedCache "/root/.cache/zig" (Q.cacheVolume ("rockbox-zig-" <> input.platform) client)
      # C.withMountedCache "/root/.gradle" (Q.cacheVolume "rockbox-gradle" client)
      # C.withDirectory "/src" source # C.withWorkdir "/src"
      # C.withEnvVariable "TAG" options.tag # C.withEnvVariable "CI" "true"
      # C.withEnvVariable "RUSTFLAGS" (if arch == "x86_64" then "-C target-cpu=x86-64 -A warnings" else "-A warnings")
      # G.exec [ "mkdir", "-p", "/out" ]
    withArtifacts = case artifacts of
      Nothing -> prepared
      Just directory -> C.withDirectory "/src/artifacts" directory prepared
    withSdk = case androidSdk of
      Nothing -> withArtifacts
      Just directory -> C.withDirectory "/opt/android-sdk" directory withArtifacts
  execute job client arch options withSdk

execute :: Job -> Client -> String -> Options -> Container -> Aff Directory
execute job client arch options ctr = case job of
  LinuxX64Check -> linuxCheck
  LinuxArm64Check -> linuxCheck
  LinuxX64Release -> Build.nativeRelease client arch options.tag ctr
  LinuxArm64Release -> Build.nativeRelease client arch options.tag ctr
  LinuxArmhfRelease -> Build.armhfRelease options.tag ctr
  Build -> do
    built <- Build.headless arch ctr
    pure $ Build.cli built # Build.stageBinaries
      # G.archive ("rockbox_" <> options.tag <> "_" <> arch <> "-linux.tar.gz") "/stage/bin" [ "rockbox", "rockboxd" ]
      # C.directory "/out"
  Embed -> do
    built <- Build.headless arch ctr >>= Firmware.embedded arch
    pure $ built # Build.embeddedArchive ("librockboxd_" <> options.tag <> "_" <> arch <> "-linux.tar.gz") # C.directory "/out"
  Desktop -> do
    built <- Build.headless arch ctr >>= Firmware.embedded arch
    let app = Build.stageDesktop built
        name = "rockbox-desktop-" <> options.tag <> "-linux-" <> arch
        staged = app # C.withFile ("/stage/" <> name <> "/rockbox-desktop") (C.file "/src/desktop/target/release/rockbox-desktop" app)
          # C.withFile ("/stage/" <> name <> "/README.md") (C.file "/src/desktop/README.md" app)
          # C.withDirectory ("/stage/" <> name <> "/skins") (C.directory "/src/desktop/skins" app)
    pure (C.directory "/out" (G.archive (name <> ".tar.gz") "/stage" [ name ] staged))
  GoTest -> G.report client "go-test" (ffi # G.at "/src/bindings/go" [ "go", "vet", "-unsafeptr=false", "./..." ] # G.at "/src/bindings/go" [ "go", "test", "-v", "./..." ])
  Ffi -> pure $ Q.directory client # D.withFile "out/librockbox_ffi.so" (C.file "/src/target/release/librockbox_ffi.so" ffi)
    # D.withFile "out/librockbox_ffi.a" (C.file "/src/target/release/librockbox_ffi.a" ffi)
  Nif -> G.globFiles client (Build.audioDependencies ctr # G.at "/src/bindings/erlang" [ "make", "TARGET=" <> arch <> "-linux-gnu" ]) "/src" [ "bindings/erlang/priv/*.so" ]
  AndroidCheck -> do
    prepared <- Android.prepare ctr >>= Android.firmware
    G.globFiles client (Android.native options.features prepared) "/src" [ "expo/modules/rockbox-rpc/android/src/main/jniLibs/*/*.so" ]
  AndroidApk -> apk
  AndroidRelease -> apk
  AndroidFfi -> do
    G.ensure (A.elem options.abi [ "arm64-v8a", "x86_64" ]) "Unsupported Android FFI ABI"
    prepared <- Android.prepare ctr
    let built = prepared # G.exec [ "cargo", "ndk", "-t", options.abi, "--platform", "26", "-o", "jniout", "build", "--release", "-p", "rockbox-ffi" ]
    pure (D.withFile "out/librockbox_ffi.so" (C.file ("/src/jniout/" <> options.abi <> "/librockbox_ffi.so") built) (Q.directory client))
  Ruby -> Bindings.ruby options.version ctr >>= \built -> G.globFiles client built "/src" [ "bindings/ruby/pkg/*.gem" ]
  Python -> Bindings.python options.version ctr >>= \built -> G.globFiles client built "/src" [ "bindings/python/wheelhouse/*" ]
  Npm -> Bindings.npm options.version ctr >>= \built -> G.globFiles client built "/src" [ "bindings/typescript/tarballs/*.tgz" ]
  Format -> Format.check client ctr
  WebTest -> G.report client "web-test" (ctr # G.at "/src/webui/rockbox" [ "bun", "install" ] # G.at "/src/webui/rockbox" [ "bun", "run", "test", "--run" ])
  Sdl -> do
    let built = Legacy.sdl ctr
    pure (Q.directory client # D.withFile "runtime/usr/local/bin/rockbox" (C.file "/usr/local/bin/rockbox" built)
      # D.withFile "runtime/usr/local/bin/rockboxd" (C.file "/usr/local/bin/rockboxd" built)
      # D.withDirectory "runtime/usr/local/lib/rockbox" (C.directory "/usr/local/lib/rockbox" built)
      # D.withDirectory "runtime/usr/local/share/rockbox" (C.directory "/usr/local/share/rockbox" built))
  Release -> pure (Legacy.release arch options.tag ctr)
  Deb -> Legacy.deb client options.tag ctr
  Rpm -> Legacy.rpm client options.tag ctr
  Electron -> G.globFiles client (ctr # G.at "/src/webui/rockbox" [ "bun", "install" ] # G.at "/src/webui/rockbox" [ "bun", "run", "build:electron", "--publish", "never" ]) "/src" [ "webui/rockbox/release/*/*.AppImage", "webui/rockbox/release/*/*.tar.gz" ]
  Storybook -> do
    let built = ctr # G.at "/src/webui/rockbox" [ "bun", "install" ] # G.at "/src/webui/rockbox" [ "bun", "run", "build-storybook" ]
    pure (D.withDirectory "webui/rockbox/storybook-static" (C.directory "/src/webui/rockbox/storybook-static" built) (Q.directory client))
  where
  ffi = Build.audioDependencies ctr # G.exec [ "cargo", "build", "--release", "-p", "rockbox-ffi" ]
  linuxCheck = do
    built <- Build.headless arch ctr
    pure (G.files client (Build.cli built) [ "target/release/rockbox", "zig/zig-out/bin/rockboxd" ])
  apk = do
    built <- Android.prepare ctr >>= Android.apk options.features options.architectures
    pure (G.files client built [ "expo/android/app/build/outputs/apk/release/app-release.apk" ])
