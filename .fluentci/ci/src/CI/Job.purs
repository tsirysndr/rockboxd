module CI.Job where

import CI.Toolchain (Toolchain)
import CI.Toolchain as T

-- A closed set of typed pipelines.
data Job
  = Sdl
  | LinuxX64Check
  | LinuxArm64Check
  | LinuxX64Release
  | LinuxArm64Release
  | LinuxArmhfRelease
  | Build
  | Embed
  | Desktop
  | GoTest
  | Ffi
  | Nif
  | AndroidCheck
  | AndroidApk
  | AndroidRelease
  | AndroidFfi
  | Ruby
  | Python
  | Npm
  | Format
  | WebTest
  | Deb
  | Rpm
  | Release
  | Electron
  | Storybook

name :: Job -> String
name = case _ of
  Sdl -> "sdl"
  LinuxX64Check -> "linux-x86_64-check"
  LinuxArm64Check -> "linux-aarch64-check"
  LinuxX64Release -> "linux-x86_64-release"
  LinuxArm64Release -> "linux-aarch64-release"
  LinuxArmhfRelease -> "linux-armhf-release"
  Build -> "linux-release"
  Embed -> "embed"
  Desktop -> "desktop"
  GoTest -> "go-test"
  Ffi -> "ffi"
  Nif -> "nif"
  AndroidCheck -> "android-check"
  AndroidApk -> "android-apk"
  AndroidRelease -> "android-release"
  AndroidFfi -> "android-ffi"
  Ruby -> "package-ruby"
  Python -> "package-python"
  Npm -> "package-npm"
  Format -> "format"
  WebTest -> "web-test"
  Deb -> "deb"
  Rpm -> "rpm"
  Release -> "legacy-release"
  Electron -> "electron"
  Storybook -> "storybook"

toolchain :: Job -> Toolchain
toolchain = case _ of
  Sdl -> T.Full
  LinuxX64Check -> T.Full
  LinuxArm64Check -> T.Full
  LinuxX64Release -> T.Full
  LinuxArm64Release -> T.Full
  LinuxArmhfRelease -> T.Cross
  Build -> T.Full
  Embed -> T.Full
  Desktop -> T.Full
  GoTest -> T.Go
  Ffi -> T.Rust
  Nif -> T.Erlang
  AndroidCheck -> T.Android
  AndroidApk -> T.Android
  AndroidRelease -> T.Android
  AndroidFfi -> T.Android
  Ruby -> T.Ruby
  Python -> T.Python
  Npm -> T.Npm
  Format -> T.Rust
  WebTest -> T.Web
  Deb -> T.Packages
  Rpm -> T.Packages
  Release -> T.Full
  Electron -> T.Web
  Storybook -> T.Web
