module Main where

import Prelude
import CI.Base as Base
import CI.Job (Job(..))
import CI.Runner as Runner
import CI.Image as Image
import CI.Nix as Nix
import Data.Maybe (Maybe(..))
import Dagger.Modules as Module
import Effect (Effect)

main :: Effect Unit
main = Module.runModule
  [ Module.object_ "RockboxCi" {}
      [ Module.function "linuxCheck" "Build the Linux headless daemon and CLI for the selected platform."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run
            (if input.platform == "linux/arm64" then LinuxArm64Check else LinuxX64Check)
            client input Runner.defaults Nothing Nothing)
      , Module.function "linuxRelease" "Package a Linux release for amd64 or arm64, with checksums."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.argument "tag"
            in { source, platform, tag })
          (\client input -> Runner.run
            (if input.platform == "linux/arm64" then LinuxArm64Release else LinuxX64Release)
            client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "sdlBuild" "Build and install SDL firmware, CLI, daemon, codecs and assets."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Sdl client input Runner.defaults Nothing Nothing)
      , Module.function "linuxArmhfRelease" "Run linux-armhf-release and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run LinuxArmhfRelease client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "build" "Run linux-release and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Build client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "embed" "Run embed and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Embed client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "desktop" "Run desktop and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Desktop client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "goTest" "Run go-test and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run GoTest client { source: input.source, platform: input.platform }
            Runner.defaults Nothing Nothing)
      , Module.function "ffi" "Run ffi and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Ffi client { source: input.source, platform: input.platform }
            Runner.defaults Nothing Nothing)
      , Module.function "nif" "Run nif and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Nif client { source: input.source, platform: input.platform }
            Runner.defaults Nothing Nothing)
      , Module.function "androidCheck" "Run android-check and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            androidSdk <- Module.directoryArgument "androidSdk" [ "**", "!ndk" ]
            in { source, platform, androidSdk })
          (\client input -> Runner.run AndroidCheck client { source: input.source, platform: input.platform }
            Runner.defaults Nothing (Just input.androidSdk))
      , Module.function "androidApk" "Run android-apk and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            features <- Module.defaultArgument "features" "embedded-daemon"
            architectures <- Module.defaultArgument "architectures" "arm64-v8a"
            androidSdk <- Module.directoryArgument "androidSdk" [ "emulator", "system-images", "sources", "extras", "docs" ]
            in { source, platform, features, architectures, androidSdk })
          (\client input -> Runner.run AndroidApk client { source: input.source, platform: input.platform }
            (Runner.defaults { features = input.features, architectures = input.architectures }) Nothing (Just input.androidSdk))
      , Module.function "androidRelease" "Run android-release and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            androidSdk <- Module.directoryArgument "androidSdk" [ "emulator", "system-images", "sources", "extras", "docs" ]
            in { source, platform, tag, androidSdk })
          (\client input -> Runner.run AndroidRelease client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing (Just input.androidSdk))
      , Module.function "androidFfi" "Run android-ffi and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            abi <- Module.defaultArgument "abi" "arm64-v8a"
            androidSdk <- Module.directoryArgument "androidSdk" [ "**", "!ndk" ]
            in { source, platform, abi, androidSdk })
          (\client input -> Runner.run AndroidFfi client { source: input.source, platform: input.platform }
            (Runner.defaults { abi = input.abi }) Nothing (Just input.androidSdk))
      , Module.function "ruby" "Run package-ruby and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            artifacts <- Module.argument "artifacts"
            version <- Module.argument "version"
            in { source, platform, artifacts, version })
          (\client input -> Runner.run Ruby client { source: input.source, platform: input.platform }
            (Runner.defaults { version = input.version }) (Just input.artifacts) Nothing)
      , Module.function "python" "Run package-python and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            artifacts <- Module.argument "artifacts"
            version <- Module.argument "version"
            in { source, platform, artifacts, version })
          (\client input -> Runner.run Python client { source: input.source, platform: input.platform }
            (Runner.defaults { version = input.version }) (Just input.artifacts) Nothing)
      , Module.function "npm" "Run package-npm and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            artifacts <- Module.argument "artifacts"
            version <- Module.argument "version"
            in { source, platform, artifacts, version })
          (\client input -> Runner.run Npm client { source: input.source, platform: input.platform }
            (Runner.defaults { version = input.version }) (Just input.artifacts) Nothing)
      , Module.function "format" "Run format and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Format client { source: input.source, platform: input.platform }
            Runner.defaults Nothing Nothing)
      , Module.function "webTest" "Run web-test and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run WebTest client { source: input.source, platform: input.platform }
            Runner.defaults Nothing Nothing)
      , Module.function "deb" "Run deb and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Deb client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "rpm" "Run rpm and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Rpm client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "release" "Run legacy-release and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Release client { source: input.source, platform: input.platform }
            (Runner.defaults { tag = input.tag }) Nothing Nothing)
      , Module.function "electron" "Run electron and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Electron client { source: input.source, platform: input.platform }
            Runner.defaults Nothing Nothing)
      , Module.function "storybook" "Run storybook and return its artifacts."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Storybook client { source: input.source, platform: input.platform }
            Runner.defaults Nothing Nothing)
      , Module.function "image" "Build a multi-platform OCI image archive with typed Dagger operations."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            tag <- Module.defaultArgument "tag" "latest"
            in { source, tag }) Image.archive
      , Module.function "nixBuild" "Build a flake attribute and export its store closure for host Cachix publishing."
          (ado
            source <- Module.directoryArgument "source" Base.sourceExcludes
            target <- Module.defaultArgument "target" ".#rockboxd"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, target, platform }) Nix.build
      , Module.function "nixConsume" "Install the published flake and verify the rockboxd executable."
          (ado
            ref <- Module.defaultArgument "ref" "git+https://github.com/tsirysndr/rockboxd?submodules=1"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { ref, platform }) Nix.consume
      ]
  ]
