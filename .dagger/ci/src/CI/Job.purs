module CI.Job where

-- Closed set: callers cannot select arbitrary shell scripts or toolchain images.
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

toolchain :: Job -> String
toolchain = case _ of
  Sdl -> "full"
  LinuxX64Check -> "full"
  LinuxArm64Check -> "full"
  LinuxX64Release -> "full"
  LinuxArm64Release -> "full"
  LinuxArmhfRelease -> "cross"
  Build -> "full"
  Embed -> "full"
  Desktop -> "full"
  GoTest -> "go"
  Ffi -> "rust"
  Nif -> "erlang"
  AndroidCheck -> "android"
  AndroidApk -> "android"
  AndroidRelease -> "android"
  AndroidFfi -> "android"
  Ruby -> "ruby"
  Python -> "python"
  Npm -> "npm"
  Format -> "rust"
  WebTest -> "web"
  Deb -> "packages"
  Rpm -> "packages"
  Release -> "full"
  Electron -> "web"
  Storybook -> "web"
