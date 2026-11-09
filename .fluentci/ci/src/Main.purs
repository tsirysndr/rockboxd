module Main (dagger, githubAssets, gemfury, flakehub, cachix, importNix, check, python, setupDagger) where

import Prelude
import CI.Support as S
import CI.Dagger as Dagger
import CI.Gemfury as Gemfury
import Data.Array as A
import Data.String as String
import Effect (Effect)
import Effect.Exception (throw)

-- | Run an existing typed container graph without changing its build environment.
-- | @param directory Directory for relative Dagger module/source paths.
-- | @param arguments Literal argv; environment references expand once in PureScript.
dagger :: { directory :: String, arguments :: Array String } -> Effect Unit
dagger = S.dagger

-- | Upload release assets; optionally create a release with generated notes.
-- | @param directory Directory containing release assets.
-- | @param tag Existing or new release tag.
-- | @param create Create the release if missing.
githubAssets :: { directory :: String, tag :: String, create :: Boolean } -> Effect Unit
githubAssets = S.githubAssets

-- | Upload selected Debian/RPM packages; skip when FURY_TOKEN is absent.
-- | @param directory Directory to scan recursively for Debian/RPM packages.
gemfury :: { directory :: String } -> Effect Unit
gemfury args = do
  token <- S.env "FURY_TOKEN" ""
  when (String.trim token /= "") do
    paths <- S.files args.directory
    Gemfury.uploadFiles (A.filter (\path -> String.endsWith (String.Pattern ".deb") path || String.endsWith (String.Pattern ".rpm") path) paths)

-- | Publish the selected flake using the installed FlakeHub CLI and GitHub OIDC.
-- | @param name FlakeHub owner/repository name.
-- | @param tag Release tag; empty uses the workflow tag.
-- | @param visibility public, unlisted, or private.
-- | @param includeOutputPaths Register output paths with FlakeHub.
-- | @param rolling Publish a rolling version.
flakehub :: { name :: String, tag :: String, visibility :: String, includeOutputPaths :: Boolean, rolling :: Boolean } -> Effect Unit
flakehub args = do
  tag <- if args.tag == "" then S.requiredEnv "GITHUB_REF_NAME" else pure args.tag
  S.run ([ "flakehub-push", "--name", args.name, "--tag", tag, "--visibility", args.visibility ]
    <> (if args.includeOutputPaths then [ "--include-output-paths=true" ] else [])
    <> (if args.rolling then [ "--rolling=true" ] else []))

-- | Configure a public Cachix cache or push a result closure with host credentials.
-- | @param directory Checkout containing the Nix flake and result link.
-- | @param cache Cachix cache name.
-- | @param operation setup or push.
cachix :: { directory :: String, cache :: String, operation :: String } -> Effect Unit
cachix args = do
  let command = [ "nix", "run", "--no-write-lock-file", "--inputs-from", ".", "nixpkgs#cachix", "--" ]
  if args.operation == "setup" then S.runIn args.directory [ command <> [ "use", args.cache ] ]
  else do
    token <- S.env "CACHIX_AUTH_TOKEN" ""
    when (String.trim token /= "") (S.runIn args.directory [ command <> [ "push", args.cache, "./result" ] ])

-- | Import a container-built Nix closure and optionally push all its store paths.
-- | @param directory Exported closure directory containing store and store-paths.
-- | @param cache Cachix cache name.
importNix :: { directory :: String, cache :: String } -> Effect Unit
importNix args = do
  paths <- S.files args.directory
  when (A.elem (args.directory <> "/store-paths") paths) do
    contents <- S.readText (args.directory <> "/store-paths")
    let stores = A.filter (_ /= "") (map String.trim (String.split (String.Pattern "\n") contents))
    unless (A.null stores) do
      directory <- S.absolute args.directory
      S.run ([ "nix", "copy", "--from", "file://" <> directory <> "/store" ] <> stores)
      token <- S.env "CACHIX_AUTH_TOKEN" ""
      when (String.trim token /= "") do
        S.run ([ "nix", "run", "--no-write-lock-file", "--inputs-from", ".", "nixpkgs#cachix", "--", "push", args.cache ] <> stores)

-- | Verify literal argv expansion and lint every GitHub workflow.
-- | @param directory Repository checkout whose workflows should be linted.
check :: { directory :: String } -> Effect Unit
check args = do
  temporary <- String.trim <$> S.captureIn "." [ "mktemp", "-d" ]
  let probe = temporary <> "/native-files.txt"
  S.writeText probe "FluentCI native file operations\n"
  contents <- S.readText probe
  unless (contents == "FluentCI native file operations\n") (throw "Native file roundtrip failed")
  files <- S.files (args.directory <> "/.github/workflows")
  S.run ([ "go", "run", "github.com/rhysd/actionlint/cmd/actionlint@v1.7.12", "-shellcheck=" ] <> A.filter (String.endsWith (String.Pattern ".yml")) files)

-- | Create a managed Python environment using uv, including pip for existing checks.
-- | @param version Python version requested by the workflow.
python :: { version :: String } -> Effect Unit
python args = do
  temporary <- S.requiredEnv "RUNNER_TEMP"
  let prefix = temporary <> "/fluentci-python"
  S.run [ "uv", "python", "install", args.version ]
  S.run [ "uv", "venv", "--python", args.version, prefix ]
  S.run [ "uv", "pip", "install", "--python", prefix <> "/bin/python", "pip" ]
  path <- S.requiredEnv "PATH"
  destination <- S.requiredEnv "GITHUB_ENV"
  previous <- S.readText destination
  S.writeText destination (previous <> "PATH=" <> prefix <> "/bin:" <> path <> "\nVIRTUAL_ENV=" <> prefix <> "\n")

-- | Install the repository-pinned Dagger beta with verified upstream checksums.
setupDagger :: Effect Unit
setupDagger = Dagger.setup
