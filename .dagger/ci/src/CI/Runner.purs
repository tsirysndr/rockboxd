module CI.Runner where

import Prelude
import CI.Base as Base
import CI.Job (Job)
import CI.Job as Job
import Data.Foldable (foldl)
import Data.Maybe (Maybe(..))
import Dagger (Client)
import Dagger.Container as Container
import Dagger.Directory as Directory
import Dagger.Query as Query
import Dagger.Types (Container, Directory, Platform)
import Effect.Aff (Aff)

type Input = { source :: Directory, platform :: String }
type Env = { name :: String, value :: String }

-- Images match each language's existing workflow. Native release builds retain
-- Ubuntu 24.04's ABI; in particular they must not silently move to newer glibc.
image :: String -> String
image = case _ of
  "go" -> "golang:1.26.2-bookworm"
  "erlang" -> "erlang:27"
  "ruby" -> "ruby:3.3-bookworm"
  "python" -> "python:3.12-bookworm"
  _ -> "ubuntu:24.04"

toolchain :: Client -> Platform -> Directory -> String -> Container
toolchain client arch source family =
  let
    base = if family == "cross" then Directory.dockerBuildWith
      (Directory.defaultDockerBuildOpts
        { dockerfile = Just "Dockerfile.arm-unknown-linux-gnueabihf", platform = Just arch }) source
      else Base.container client arch (image family)
    node = Base.container client arch "node:22-bookworm-slim"
  in base
    # Container.withDirectory "/usr/local" (Container.directory "/usr/local" node)
    # Container.withEnvVariable "PATH" "/root/.cargo/bin:/usr/local/go/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    # Container.withEnvVariable "TOOLCHAIN" family
    # Container.withDirectory "/ci/scripts" (Directory.directory ".dagger/ci/scripts" source)
    # Container.withExec [ "bash", "/ci/scripts/setup.sh" ]

run :: Job -> Client -> Input -> Array Env -> Maybe Directory -> Maybe Directory -> Aff Directory
run job client input env artifacts androidSdk = do
  -- The armhf compiler runs on x86_64 and targets ARMv6; it isn't an arm64 build.
  let family = Job.toolchain job
  arch <- Base.platform (if family == "cross" then "linux/amd64" else input.platform)
  let
    source = Base.sourceTree client input.source
    prepared = toolchain client arch source family
      # Container.withMountedCache "/root/.cargo/registry" (Query.cacheVolume "rockbox-cargo-registry" client)
      # Container.withMountedCache "/root/.cargo/git" (Query.cacheVolume "rockbox-cargo-git" client)
      # Container.withMountedCache "/root/.cache/zig" (Query.cacheVolume ("rockbox-zig-" <> input.platform) client)
      # Container.withMountedCache "/root/.gradle" (Query.cacheVolume "rockbox-gradle" client)
      # Container.withDirectory "/src" source
      # Container.withWorkdir "/src"
      # Container.withEnvVariable "JOB" (Job.name job)
      # Container.withEnvVariable "PLATFORM" input.platform
      # Container.withEnvVariable "TAG" "latest"
      # Container.withEnvVariable "FEATURES" "embedded-daemon"
      # Container.withEnvVariable "ARCHITECTURES" "arm64-v8a"
      # Container.withEnvVariable "ABI" "arm64-v8a"
      # Container.withEnvVariable "LIBFILE" "librockbox_ffi.so"
    withArtifacts = case artifacts of
      Nothing -> prepared
      Just directory -> Container.withDirectory "/src/artifacts" directory prepared
    withSdk = case androidSdk of
      Nothing -> withArtifacts
      Just directory -> Container.withDirectory "/opt/android-sdk" directory withArtifacts
    configured = foldl (\container variable -> Container.withEnvVariable variable.name variable.value container) withSdk env
    built = configured # Container.withExec [ "bash", "/ci/scripts/run.sh" ]
  pure (Container.directory "/out" built)
