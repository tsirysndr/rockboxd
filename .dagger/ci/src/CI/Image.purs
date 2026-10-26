module CI.Image where

import Prelude
import CI.Base as Base
import CI.Build as Build
import CI.Graph as G
import CI.Toolchain as T
import Data.Foldable (foldl)
import Data.Maybe (Maybe(..))
import Dagger (Client)
import Dagger.Container as C
import Dagger.Query as Q
import Dagger.Types (Container, Directory, File)
import Effect.Aff (Aff)

variant :: Client -> Directory -> String -> String -> Aff Container
variant client source tag platform = do
  arch <- Base.platform platform
  let
    amd = platform == "linux/amd64"
    machine = if amd then "x86_64" else "aarch64"
    debArch = if amd then "amd64" else "arm64"
    clean = Base.sourceTree client source
    toolchain = T.prepare client arch machine clean T.Full
      # C.withDirectory "/src" clean # C.withWorkdir "/src" # C.withEnvVariable "TAG" tag
      # C.withEnvVariable "RUSTFLAGS" (if amd then "-C target-cpu=x86-64 -A warnings" else "-A warnings")
  daemon <- Build.headless machine toolchain
  let
    built = Build.cli daemon
    typesense = Base.container client arch "typesense/typesense:30.1"
    runtime = Base.container client arch "debian:trixie-slim"
      # T.apt [ "ca-certificates", "libunwind8", "libasound2", "libdbus-1-3" ]
      # C.withFile "/tmp/snapserver.deb" (Q.http ("https://github.com/snapcast/snapcast/releases/download/v0.35.0/snapserver_0.35.0-1_" <> debArch <> "_trixie.deb") client)
      # G.exec [ "apt-get", "install", "-y", "/tmp/snapserver.deb" ]
      # G.exec [ "rm", "-rf", "/tmp/snapserver.deb", "/var/lib/apt/lists" ]
      # C.withFile "/usr/local/bin/rockboxd" (C.file "/src/zig/zig-out/bin/rockboxd" built)
      # C.withFile "/usr/local/bin/rockbox" (C.file "/src/target/release/rockbox" built)
      # C.withFile "/usr/local/bin/typesense-server" (C.file "/opt/typesense-server" typesense)
      # C.withFile "/etc/snapserver.conf" (C.file "/src/docker/snapserver.conf" built)
      -- Preserve the application's runtime supervisor; this is not a CI runner.
      # C.withFile "/usr/local/bin/entrypoint.sh" (C.file "/src/docker/entrypoint.sh" built)
      # G.exec [ "chmod", "+x", "/usr/local/bin/entrypoint.sh" ]
      # C.withFile "/root/.config/rockbox.org/settings.toml" (C.file "/src/docker/settings.toml" built)
      # G.exec [ "mkdir", "-p", "/root/Music" ]
      # C.withEntrypoint [ "/usr/local/bin/entrypoint.sh" ]
  pure (foldl (\ctr port -> C.withExposedPort port ctr) runtime [ 6061, 6062, 6063, 6600, 7882, 1704, 1705, 1780, 4533, 8096 ])

archive :: Client -> { source :: Directory, tag :: String } -> Aff File
archive client input = do
  amd <- variant client input.source input.tag "linux/amd64"
  arm <- variant client input.source input.tag "linux/arm64"
  pure (C.asTarballWith (C.defaultAsTarballOpts { platformVariants = Just [ amd, arm ] }) (Q.container client))
