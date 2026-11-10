module Main (publishImage, containerCheck, nixBuild, nixPackage, nixConsume, linuxCheck, linuxRelease, linuxArmhfRelease, build, embed, desktop, goTest, ffi, nif, androidCheck, androidApk, androidRelease, androidFfi, ruby, packagePython, npm, format, webTest, image, githubAssets, gemfury, flakehub, cachix, check, python) where

import CI.Nix as Nix
import Native.Engine as Engine
import Native.Types (Directory(..))
import CI.Runner as Runner
import CI.Image as Image
import CI.Job (Job(..))
import Data.Maybe (Maybe(..))
import CI.ContainerCheck as ContainerCheck
import CI.PublishImage as PublishImage
import Prelude
import CI.Support as S
import CI.Gemfury as Gemfury
import Data.Array as A
import Data.String as String
import Effect (Effect)
import Effect.Exception (throw)

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


-- Native PureScript jobs; commands are literal argument vectors.
linuxCheck :: { source :: String, platform :: String, output :: String } -> Effect Unit
linuxCheck args = Engine.runJob do
  result <- Runner.run (if args.platform == "linux/arm64" then LinuxArm64Check else LinuxX64Check) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing Nothing
  Engine.exportDirectory args.output result

linuxRelease :: { source :: String, platform :: String, output :: String, tag :: String } -> Effect Unit
linuxRelease args = Engine.runJob do
  result <- Runner.run (if args.platform == "linux/arm64" then LinuxArm64Release else LinuxX64Release) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { tag = args.tag }) Nothing Nothing
  Engine.exportDirectory args.output result

linuxArmhfRelease :: { source :: String, platform :: String, output :: String, tag :: String } -> Effect Unit
linuxArmhfRelease args = Engine.runJob do
  result <- Runner.run (LinuxArmhfRelease) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { tag = args.tag }) Nothing Nothing
  Engine.exportDirectory args.output result

build :: { source :: String, platform :: String, output :: String, tag :: String } -> Effect Unit
build args = Engine.runJob do
  result <- Runner.run (Build) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { tag = args.tag }) Nothing Nothing
  Engine.exportDirectory args.output result

embed :: { source :: String, platform :: String, output :: String, tag :: String } -> Effect Unit
embed args = Engine.runJob do
  result <- Runner.run (Embed) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { tag = args.tag }) Nothing Nothing
  Engine.exportDirectory args.output result

desktop :: { source :: String, platform :: String, output :: String, tag :: String } -> Effect Unit
desktop args = Engine.runJob do
  result <- Runner.run (Desktop) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { tag = args.tag }) Nothing Nothing
  Engine.exportDirectory args.output result

goTest :: { source :: String, platform :: String, output :: String } -> Effect Unit
goTest args = Engine.runJob do
  result <- Runner.run (GoTest) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing Nothing
  Engine.exportDirectory args.output result

ffi :: { source :: String, platform :: String, output :: String } -> Effect Unit
ffi args = Engine.runJob do
  result <- Runner.run (Ffi) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing Nothing
  Engine.exportDirectory args.output result

nif :: { source :: String, platform :: String, output :: String } -> Effect Unit
nif args = Engine.runJob do
  result <- Runner.run (Nif) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing Nothing
  Engine.exportDirectory args.output result

androidCheck :: { source :: String, platform :: String, output :: String, androidSdk :: String } -> Effect Unit
androidCheck args = Engine.runJob do
  result <- Runner.run (AndroidCheck) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing (Just (LocalDirectory args.androidSdk))
  Engine.exportDirectory args.output result

androidApk :: { source :: String, platform :: String, output :: String, androidSdk :: String, features :: String, architectures :: String } -> Effect Unit
androidApk args = Engine.runJob do
  result <- Runner.run (AndroidApk) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { features = args.features, architectures = args.architectures }) Nothing (Just (LocalDirectory args.androidSdk))
  Engine.exportDirectory args.output result

androidRelease :: { source :: String, platform :: String, output :: String, androidSdk :: String } -> Effect Unit
androidRelease args = Engine.runJob do
  result <- Runner.run (AndroidRelease) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing (Just (LocalDirectory args.androidSdk))
  Engine.exportDirectory args.output result

androidFfi :: { source :: String, platform :: String, output :: String, androidSdk :: String, abi :: String } -> Effect Unit
androidFfi args = Engine.runJob do
  result <- Runner.run (AndroidFfi) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { abi = args.abi }) Nothing (Just (LocalDirectory args.androidSdk))
  Engine.exportDirectory args.output result

ruby :: { source :: String, platform :: String, output :: String, version :: String, artifacts :: String } -> Effect Unit
ruby args = Engine.runJob do
  result <- Runner.run (Ruby) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { version = args.version }) (Just (LocalDirectory args.artifacts)) Nothing
  Engine.exportDirectory args.output result

packagePython :: { source :: String, platform :: String, output :: String, version :: String, artifacts :: String } -> Effect Unit
packagePython args = Engine.runJob do
  result <- Runner.run (Python) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { version = args.version }) (Just (LocalDirectory args.artifacts)) Nothing
  Engine.exportDirectory args.output result

npm :: { source :: String, platform :: String, output :: String, version :: String, artifacts :: String } -> Effect Unit
npm args = Engine.runJob do
  result <- Runner.run (Npm) unit { source: LocalDirectory args.source, platform: args.platform } (Runner.defaults { version = args.version }) (Just (LocalDirectory args.artifacts)) Nothing
  Engine.exportDirectory args.output result

format :: { source :: String, platform :: String, output :: String } -> Effect Unit
format args = Engine.runJob do
  result <- Runner.run (Format) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing Nothing
  Engine.exportDirectory args.output result

webTest :: { source :: String, platform :: String, output :: String } -> Effect Unit
webTest args = Engine.runJob do
  result <- Runner.run (WebTest) unit { source: LocalDirectory args.source, platform: args.platform } Runner.defaults Nothing Nothing
  Engine.exportDirectory args.output result

image :: { source :: String, tag :: String, platform :: String, output :: String } -> Effect Unit
image args = Engine.runJob (Image.archive unit { source: LocalDirectory args.source, tag: args.tag, platform: args.platform, output: args.output })

nixBuild :: { source :: String, target :: String, cache :: String } -> Effect Unit
nixBuild = Nix.build

nixPackage :: { source :: String, cache :: String } -> Effect Unit
nixPackage = Nix.package

nixConsume :: { ref :: String } -> Effect Unit
nixConsume = Nix.consume


containerCheck :: Effect Unit
containerCheck = ContainerCheck.check

publishImage :: { directory :: String, tag :: String, registries :: Array String } -> Effect Unit
publishImage = PublishImage.publish
