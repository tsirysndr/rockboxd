module Main where

import Prelude
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
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run
            (if input.platform == "linux/arm64" then LinuxArm64Check else LinuxX64Check)
            client input [] Nothing Nothing)
      , Module.function "linuxRelease" "Package a Linux release for amd64 or arm64, with checksums."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.argument "tag"
            in { source, platform, tag })
          (\client input -> Runner.run
            (if input.platform == "linux/arm64" then LinuxArm64Release else LinuxX64Release)
            client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "sdlBuild" "Build and install SDL firmware, CLI, daemon, codecs and assets."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Sdl client input [] Nothing Nothing)
      , Module.function "linuxArmhfRelease" "Run linux-armhf-release; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run LinuxArmhfRelease client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "build" "Run linux-release; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Build client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "embed" "Run embed; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Embed client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "desktop" "Run desktop; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Desktop client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "goTest" "Run go-test; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run GoTest client { source: input.source, platform: input.platform }
            [  ] Nothing Nothing)
      , Module.function "ffi" "Run ffi; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Ffi client { source: input.source, platform: input.platform }
            [  ] Nothing Nothing)
      , Module.function "nif" "Run nif; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Nif client { source: input.source, platform: input.platform }
            [  ] Nothing Nothing)
      , Module.function "androidCheck" "Run android-check; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            androidSdk <- Module.argument "androidSdk"
            in { source, platform, androidSdk })
          (\client input -> Runner.run AndroidCheck client { source: input.source, platform: input.platform }
            [  ] Nothing (Just input.androidSdk))
      , Module.function "androidApk" "Run android-apk; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            features <- Module.defaultArgument "features" "embedded-daemon"
            architectures <- Module.defaultArgument "architectures" "arm64-v8a"
            androidSdk <- Module.argument "androidSdk"
            in { source, platform, features, architectures, androidSdk })
          (\client input -> Runner.run AndroidApk client { source: input.source, platform: input.platform }
            [ { name: "FEATURES", value: input.features }, { name: "ARCHITECTURES", value: input.architectures } ] Nothing (Just input.androidSdk))
      , Module.function "androidRelease" "Run android-release; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            androidSdk <- Module.argument "androidSdk"
            in { source, platform, tag, androidSdk })
          (\client input -> Runner.run AndroidRelease client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing (Just input.androidSdk))
      , Module.function "androidFfi" "Run android-ffi; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            abi <- Module.defaultArgument "abi" "arm64-v8a"
            androidSdk <- Module.argument "androidSdk"
            in { source, platform, abi, androidSdk })
          (\client input -> Runner.run AndroidFfi client { source: input.source, platform: input.platform }
            [ { name: "ABI", value: input.abi } ] Nothing (Just input.androidSdk))
      , Module.function "ruby" "Run package-ruby; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            artifacts <- Module.argument "artifacts"
            version <- Module.argument "version"
            in { source, platform, artifacts, version })
          (\client input -> Runner.run Ruby client { source: input.source, platform: input.platform }
            [ { name: "BINDING_VERSION", value: input.version } ] (Just input.artifacts) Nothing)
      , Module.function "python" "Run package-python; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            artifacts <- Module.argument "artifacts"
            version <- Module.argument "version"
            in { source, platform, artifacts, version })
          (\client input -> Runner.run Python client { source: input.source, platform: input.platform }
            [ { name: "BINDING_VERSION", value: input.version } ] (Just input.artifacts) Nothing)
      , Module.function "npm" "Run package-npm; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            artifacts <- Module.argument "artifacts"
            version <- Module.argument "version"
            in { source, platform, artifacts, version })
          (\client input -> Runner.run Npm client { source: input.source, platform: input.platform }
            [ { name: "BINDING_VERSION", value: input.version } ] (Just input.artifacts) Nothing)
      , Module.function "format" "Run format; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Format client { source: input.source, platform: input.platform }
            [  ] Nothing Nothing)
      , Module.function "webTest" "Run web-test; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run WebTest client { source: input.source, platform: input.platform }
            [  ] Nothing Nothing)
      , Module.function "deb" "Run deb; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Deb client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "rpm" "Run rpm; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Rpm client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "release" "Run legacy-release; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, platform, tag })
          (\client input -> Runner.run Release client { source: input.source, platform: input.platform }
            [ { name: "TAG", value: input.tag } ] Nothing Nothing)
      , Module.function "electron" "Run electron; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Electron client { source: input.source, platform: input.platform }
            [  ] Nothing Nothing)
      , Module.function "storybook" "Run storybook; return only its artifacts and success report."
          (ado
            source <- Module.argument "source"
            platform <- Module.defaultArgument "platform" "linux/amd64"
            in { source, platform })
          (\client input -> Runner.run Storybook client { source: input.source, platform: input.platform }
            [  ] Nothing Nothing)
      , Module.function "image" "Build a multi-platform OCI image archive from the project's Dockerfile."
          (ado
            source <- Module.argument "source"
            tag <- Module.defaultArgument "tag" "latest"
            in { source, tag }) Image.archive
      , Module.function "nixBuild" "Build a flake attribute and export its store closure for host Cachix publishing."
          (ado
            source <- Module.argument "source"
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
