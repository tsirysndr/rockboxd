module CI.Nix where

import Prelude
import CI.Base as Base
import CI.Graph as G
import Data.Maybe (Maybe(..))
import Data.String as S
import Dagger (Client)
import Dagger.Container as C
import Dagger.Types (Container, Directory, Platform, ReturnType(..))
import Effect.Aff (Aff)

prepared :: Client -> Platform -> Container
prepared client arch = Base.container client arch "nixos/nix:2.31.2"
  # C.withEnvVariable "NIX_CONFIG" "experimental-features = nix-command flakes\nfilter-syscalls = false\nsandbox = false\n"
  # G.exec [ "nix", "run", "nixpkgs#cachix", "--", "use", "rockbox" ]

build :: Client -> { source :: Directory, target :: String, platform :: String } -> Aff Directory
build client input = do
  arch <- Base.platform input.platform
  G.ensure (S.take 2 input.target == ".#") "Expected a local flake attribute such as .#rockboxd"
  let
    target = "path:" <> input.target
    ctr = prepared client arch # C.withDirectory "/src" (Base.sourceTree client input.source) # C.withWorkdir "/src"
      # G.exec [ "mkdir", "-p", "/out" ] # G.exec [ "nix", "flake", "show", "path:." ]
    probe = ctr # C.withExecWith [ "nix", "eval", "--raw", target <> ".drvPath" ] (C.defaultWithExecOpts { expect = Just ReturnTypeAny })
  code <- C.exitCode probe
  if code == 0 then do
    let built = ctr # G.exec [ "nix", "build", "--print-build-logs", "--no-link", "--print-out-paths", target ]
    paths <- G.lines <$> C.stdout built
    pure $ built # C.withNewFile "/out/store-paths" (S.joinWith "\n" paths <> "\n")
      # G.exec ([ "nix", "copy", "--to", "file:///out/store" ] <> paths) # C.directory "/out"
  else do
    message <- C.stderr probe
    G.ensure (S.contains (S.Pattern "does not provide attribute") message) message
    pure (C.directory "/out" (C.withNewFile "/out/store-paths" "" ctr))

consume :: Client -> { ref :: String, platform :: String } -> Aff String
consume client input = do
  arch <- Base.platform input.platform
  prepared client arch
    # G.exec [ "nix", "profile", "add", "--print-build-logs", input.ref ]
    # G.exec [ "test", "-x", "/root/.nix-profile/bin/rockboxd" ]
    # G.exec [ "ls", "-l", "/root/.nix-profile/bin/rockboxd" ]
    # G.exec [ "nix", "profile", "list" ]
    # C.stdout
