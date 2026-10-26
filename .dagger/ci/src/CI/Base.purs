module CI.Base where

import Prelude
import Data.Maybe (Maybe(..))
import Dagger (Client)
import Dagger.Container as Container
import Dagger.Directory as Directory
import Dagger.Query as Query
import Dagger.Types (Container, Directory, Platform(..))
import Effect.Aff (Aff, throwError)
import Effect.Exception (error)

-- Keep generated CI code, host caches, and secrets out of build inputs.
-- Never use **/target: firmware/target is part of Rockbox's source tree.
sourceExcludes :: Array String
sourceExcludes = [ ".git", ".dagger", ".devbox", ".github", "target", "build", "tools/bmp2rb", "tools/codepages", "tools/convbdf", "tools/mkboot", "tools/rdf2binary", "tools/uclpack", "tools/scramble", "desktop/target", "desktop-gpui/target", "bindings/**/target", "**/.zig-cache", "**/zig-out", "build-headless", "build-lib", "build-armhf", "build-android-arm64", "**/node_modules", "**/.env", "**/.env.*", "artifacts", "release-source", "dist/build", "dist/rpmbuild", "result" ]

sourceTree :: Client -> Directory -> Directory
sourceTree client source = Query.directory client
  # Directory.withDirectoryWith "." source
      (Directory.defaultWithDirectoryOpts
        { exclude = Just sourceExcludes })

platform :: String -> Aff Platform
platform value = case value of
  "linux/amd64" -> pure (Platform value)
  "linux/arm64" -> pure (Platform value)
  _ -> throwError (error "Supported platforms: linux/amd64 and linux/arm64")

container :: Client -> Platform -> String -> Container
container client arch image = Query.containerWith (Query.defaultContainerOpts { platform = Just arch }) client
  # Container.from image
