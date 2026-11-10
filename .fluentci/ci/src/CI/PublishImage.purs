module CI.PublishImage where

import Prelude
import CI.Support as S
import Data.Foldable (traverse_)
import Data.Array as A
import Data.String as String
import Data.String.CodeUnits as CU
import Effect (Effect)
import Effect.Exception (throw)

publish :: { directory :: String, tag :: String, registries :: Array String } -> Effect Unit
publish args = do
  unless (args.tag /= "" && String.take 1 args.tag /= "-" && A.all (\c ->
    (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || A.elem c [ '.', '_', '-' ]) (CU.toCharArray args.tag))
    (throw "Invalid image tag")
  home <- S.requiredEnv "HOME"
  S.run [ "sudo", "apt-get", "update" ]
  S.run [ "sudo", "apt-get", "install", "-y", "skopeo" ]
  traverse_ (\registry -> do
    unless (registry /= "" && not (String.contains (String.Pattern " ") registry) && String.take 1 registry /= "-") (throw "Invalid registry repository")
    let reference arch = registry <> ":" <> args.tag <> "-" <> arch
    traverse_ (\arch -> S.run [ "skopeo", "copy", "--all", "--authfile", home <> "/.docker/config.json",
      "oci-archive:" <> args.directory <> "/rockbox-image-" <> arch <> ".tar", "docker://" <> reference arch ]) [ "amd64", "arm64" ]
    traverse_ (\tag -> S.run [ "docker", "buildx", "imagetools", "create", "--tag", registry <> ":" <> tag,
      reference "amd64", reference "arm64" ]) (A.nub [ args.tag, "latest" ])) args.registries
