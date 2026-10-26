module CI.Nix where

import Prelude
import CI.Base as Base
import Dagger (Client)
import Dagger.Container as Container
import Dagger.Types (Container, Directory, Platform)
import Effect.Aff (Aff)

prepared :: Client -> Platform -> Container
prepared client arch = Base.container client arch "nixos/nix:2.31.2"
  # Container.withEnvVariable "NIX_CONFIG" "experimental-features = nix-command flakes\nfilter-syscalls = false\nsandbox = false\n"
  # Container.withExec [ "nix", "run", "nixpkgs#cachix", "--", "use", "rockbox" ]

build :: Client -> { source :: Directory, target :: String, platform :: String } -> Aff Directory
build client input = do
  arch <- Base.platform input.platform
  pure $ prepared client arch
    # Container.withDirectory "/src" (Base.sourceTree client input.source)
    # Container.withWorkdir "/src"
    # Container.withEnvVariable "FLAKE_TARGET" input.target
    # Container.withExec [ "bash", "/src/.dagger/ci/scripts/nix-build.sh" ]
    # Container.directory "/out"

consume :: Client -> { ref :: String, platform :: String } -> Aff String
consume client input = do
  arch <- Base.platform input.platform
  prepared client arch
    # Container.withExec [ "nix", "profile", "add", "--print-build-logs", input.ref ]
    # Container.withExec [ "bash", "-ec", "nix profile list; test -x /root/.nix-profile/bin/rockboxd; ls -l /root/.nix-profile/bin/rockboxd" ]
    # Container.stdout
