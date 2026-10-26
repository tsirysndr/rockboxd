module CI.Bindings where

import Prelude
import CI.Graph as G
import Data.Argonaut.Core as J
import Data.Argonaut.Parser (jsonParser)
import Data.Either (Either(..))
import Data.Foldable (foldM)
import Data.Maybe (Maybe(..))
import Dagger.Container as C
import Dagger.Directory as D
import Dagger.File as F
import Dagger.Types (Container)
import Effect.Aff (Aff, throwError)
import Effect.Exception (error)
import Foreign.Object as O

type Target = { name :: String, library :: String, gem :: String, wheel :: String }

targets :: Array Target
targets =
  [ { name: "darwin-arm64", library: "librockbox_ffi.dylib", gem: "arm64-darwin", wheel: "macosx_11_0_arm64" }
  , { name: "darwin-x64", library: "librockbox_ffi.dylib", gem: "x86_64-darwin", wheel: "macosx_10_12_x86_64" }
  , { name: "linux-x64", library: "librockbox_ffi.so", gem: "x86_64-linux", wheel: "manylinux_2_38_x86_64" }
  , { name: "linux-arm64", library: "librockbox_ffi.so", gem: "aarch64-linux", wheel: "manylinux_2_38_aarch64" }
  , { name: "freebsd-x64", library: "librockbox_ffi.so", gem: "x86_64-freebsd", wheel: "freebsd_13_amd64" }
  , { name: "netbsd-x64", library: "librockbox_ffi.so", gem: "x86_64-netbsd", wheel: "netbsd_10_amd64" }
  ]

artifact :: Target -> String
artifact target = "/src/artifacts/librockbox_ffi-" <> target.name <> "/" <> target.library

available :: (Container -> Target -> Aff Container) -> Container -> Aff Container
available build ctr = foldM (\current target -> do
  exists <- C.exists (artifact target) current
  if exists then build current target else pure current) ctr targets

ruby :: String -> Container -> Aff Container
ruby version ctr = do
  let base = ctr
        # C.withNewFile "/src/bindings/ruby/lib/rockbox_ffi/version.rb" ("module RockboxFFI\n  VERSION = \"" <> version <> "\"\nend\n")
        # G.exec [ "mkdir", "-p", "/src/bindings/ruby/pkg" ]
  built <- available (\current target -> pure $ current
    # G.exec [ "rm", "-rf", "/src/bindings/ruby/vendor" ]
    # C.withFile ("/src/bindings/ruby/vendor/" <> target.library) (C.file (artifact target) current)
    # C.withEnvVariable "ROCKBOX_GEM_PLATFORM" target.gem
    # G.at "/src/bindings/ruby" [ "gem", "build", "rockbox_ffi.gemspec", "-o", "pkg/rockbox_ffi-" <> version <> "-" <> target.gem <> ".gem" ]) base
  pure $ built # G.exec [ "rm", "-rf", "/src/bindings/ruby/vendor" ]
    # C.withoutEnvVariable "ROCKBOX_GEM_PLATFORM"
    # G.at "/src/bindings/ruby" [ "gem", "build", "rockbox_ffi.gemspec", "-o", "pkg/rockbox_ffi-" <> version <> ".gem" ]


python :: String -> Container -> Aff Container
python version ctr = do
  stamped <- G.replaceFile "/src/bindings/python/pyproject.toml" (G.replaceLine "version = " ("\"" <> version <> "\"")) ctr
  let base = stamped # G.exec [ "pip", "install", "build", "wheel" ]
        # G.at "/src/bindings/python" [ "python", "-m", "build", "--sdist", "--outdir", "wheelhouse" ]
  built <- available (\current target -> do
    let staged = current
          # G.exec [ "rm", "-rf", "/src/bindings/python/src/rockbox_ffi/_lib", "/src/bindings/python/dist" ]
          # C.withFile ("/src/bindings/python/src/rockbox_ffi/_lib/" <> target.library) (C.file (artifact target) current)
          # G.at "/src/bindings/python" [ "python", "-m", "build", "--wheel", "--outdir", "dist" ]
    wheels <- D.glob "*.whl" (C.directory "/src/bindings/python/dist" staged)
    let tagged = G.at "/src/bindings/python" ([ "python", "-m", "wheel", "tags", "--platform-tag", target.wheel, "--remove" ] <> map ("dist/" <> _) wheels) staged
    pure (C.withDirectory "/src/bindings/python/wheelhouse" (C.directory "/src/bindings/python/dist" tagged) tagged)) base
  pure (G.exec [ "rm", "-rf", "/src/bindings/python/src/rockbox_ffi/_lib" ] built)

stampNpm :: String -> String -> Container -> Aff Container
stampNpm version path ctr = do
  content <- D.file path (C.directory "/src/bindings/typescript" ctr) # F.contents
  value <- case jsonParser content of
    Left message -> throwError (error message)
    Right json -> case J.toObject json of
      Nothing -> throwError (error ("Expected a JSON object in " <> path))
      Just object -> pure object
  let
    stamped = O.insert "version" (J.fromString version) value
    dependencies = case O.lookup "optionalDependencies" value >>= J.toObject of
      Nothing -> stamped
      Just deps -> O.insert "optionalDependencies" (J.fromObject (map (const (J.fromString version)) deps)) stamped
  pure (C.withNewFile ("/src/bindings/typescript/" <> path) (J.stringify (J.fromObject dependencies) <> "\n") ctr)

npm :: String -> Container -> Aff Container
npm version ctr = do
  main <- stampNpm version "package.json" ctr
  packages <- D.glob "npm/*/package.json" (C.directory "/src/bindings/typescript" main)
  stamped <- foldM (\current path -> stampNpm version path current) main packages
  let built = stamped # G.at "/src/bindings/typescript" [ "bun", "install" ]
        # G.at "/src/bindings/typescript" [ "bun", "run", "build" ]
        # G.exec [ "mkdir", "-p", "/src/bindings/typescript/tarballs" ]
  packed <- available (\current target -> pure $ current
    # C.withFile ("/src/bindings/typescript/npm/" <> target.name <> "/" <> target.library) (C.file (artifact target) current)
    # G.at ("/src/bindings/typescript/npm/" <> target.name) [ "npm", "pack", "--pack-destination", "/src/bindings/typescript/tarballs" ]) built
  pure (G.at "/src/bindings/typescript" [ "npm", "pack", "--pack-destination", "/src/bindings/typescript/tarballs" ] packed)
