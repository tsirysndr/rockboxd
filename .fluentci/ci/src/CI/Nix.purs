module CI.Nix where

import Prelude
import CI.Support as S
import Data.Array as A
import Data.Maybe (Maybe(..), fromMaybe)
import Data.String as String
import Effect (Effect)
import Effect.Exception (throw)
import FluentCI.Host.Plan as H
import FluentCI.Plugin as P

command :: String -> Array String -> Effect { code :: Int, output :: String, errors :: String }
command source argv = do
  root <- String.trim <$> S.captureIn "." [ "mktemp", "-d" ]
  case A.uncons argv of
    Nothing -> throw "Empty command"
    Just { head, tail } -> do
      result <- P.runPlan (root <> "/run") (H.plan [ H.task "nix" head source # H.withArgs tail # H.allowFailure true ] # H.withInheritedEnv true) >>= S.unwrap
      case A.head result.tasks of
        Nothing -> throw "Nix command returned no result"
        Just task -> do
          output <- S.readText task.stdout
          errors <- S.readText task.stderr
          pure { code: fromMaybe 1 task.exit_code, output, errors }

-- All build/evaluation commands run on the GitHub runner, with host Cachix
-- credentials. A missing platform attribute may be skipped; build failures may not.
build :: { source :: String, target :: String, cache :: String } -> Effect Unit
build args = do
  unless (String.take 2 args.target == ".#") (throw "Expected a local flake attribute")
  let cachix argv = S.runIn args.source [ [ "nix", "run", "--no-write-lock-file", "--inputs-from", ".", "nixpkgs#cachix", "--" ] <> argv ]
  when (args.cache /= "") (cachix [ "use", args.cache ])
  probe <- command args.source [ "nix", "eval", "--raw", args.target <> ".drvPath" ]
  if probe.code /= 0 then
    unless (String.contains (String.Pattern "does not provide attribute") probe.errors) (throw (probe.errors <> probe.output))
  else do
    built <- command args.source [ "nix", "build", "--accept-flake-config", "--print-build-logs", "--no-link", "--print-out-paths", args.target ]
    unless (built.code == 0) (throw (built.errors <> built.output))
    let paths = A.filter (_ /= "") (map String.trim (String.split (String.Pattern "\n") built.output))
    when (A.null paths) (throw "Nix build returned no store paths")
    token <- S.env "CACHIX_AUTH_TOKEN" ""
    when (args.cache /= "" && String.trim token /= "") (cachix ([ "push", args.cache ] <> paths))

package :: { source :: String, cache :: String } -> Effect Unit
package args = do
  S.runIn args.source [ [ "nix-build", "-I", "nixpkgs=channel:nixos-unstable", "-E", "with import <nixpkgs> {}; callPackage ./nix/package.nix {}" ] ]
  token <- S.env "CACHIX_AUTH_TOKEN" ""
  when (args.cache /= "" && String.trim token /= "") do
    S.runIn args.source [ [ "nix", "run", "--no-write-lock-file", "--inputs-from", ".", "nixpkgs#cachix", "--", "push", args.cache, "./result" ] ]

consume :: { ref :: String } -> Effect Unit
consume args = do
  S.run [ "nix", "profile", "add", "--print-build-logs", args.ref ]
  home <- S.requiredEnv "HOME"
  S.run [ "test", "-x", home <> "/.nix-profile/bin/rockboxd" ]
  S.run [ "nix", "profile", "list" ]
