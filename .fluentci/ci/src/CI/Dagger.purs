module CI.Dagger (setup) where

import Prelude
import CI.Support as S
import Data.Array as A
import Data.String as String
import Effect (Effect)
import Effect.Exception (throw)
import FluentCI.Plugin as P

-- The 1.0 beta is distributed from dl.dagger.io, rather than GitHub Releases.
-- Preserve the checksums from the repository's existing mise toolchain pin.
setup :: Effect Unit
setup = do
  host <- S.os
  arch <- P.getArch >>= S.unwrap
  let arm = A.elem arch [ "aarch64", "arm64" ]
      cpu = if arm then "arm64" else "amd64"
      platform = if S.isMac host then "darwin" else "linux"
      version = "1.0.0-beta.21"
  unless (S.isMac host || host == "linux") (throw "Dagger bootstrap supports Linux/macOS")
  expected <- case platform <> "/" <> cpu of
    "darwin/arm64" -> pure "781f4862c022a19b9a4c498b59e1d9cfda1ee3cdc8ce210e4545bd8842a7416d"
    "darwin/amd64" -> pure "fdbe191c965d240ee9cc90719fa9dca1586805cbb89b0a0087314a84a0937354"
    "linux/arm64" -> pure "ba9d55638636918a3404313511bceba5cc0af22f487c1c1b6a5ba60ce70a61bb"
    "linux/amd64" -> pure "c5ac777a5c99089869660943381136e22177b59a46412e310c2fef594760a041"
    _ -> throw "Unsupported Dagger bootstrap architecture"
  temporary <- S.requiredEnv "RUNNER_TEMP"
  let prefix = temporary <> "/fluentci-dagger"
      archive = prefix <> "/dagger.tar.gz"
      url = "https://dl.dagger.io/dagger/releases/" <> version <> "/dagger_v" <> version <> "_" <> platform <> "_" <> cpu <> ".tar.gz"
  S.run [ "mkdir", "-p", prefix ]
  S.run [ "curl", "--fail", "--location", "--output", archive, url ]
  actual <- String.trim <$> (P.sha256 (P.file archive) >>= S.unwrap)
  unless (actual == expected) (throw "Dagger archive checksum mismatch")
  S.run [ "tar", "-xzf", archive, "-C", prefix, "dagger" ]
  reported <- S.captureIn "." [ prefix <> "/dagger", "version" ]
  unless (String.contains (String.Pattern version) reported) (throw "Dagger version mismatch")
  path <- S.requiredEnv "PATH"
  destination <- S.requiredEnv "GITHUB_ENV"
  previous <- S.readText destination
  S.writeText destination (previous <> "PATH=" <> prefix <> ":" <> path <> "\n")
