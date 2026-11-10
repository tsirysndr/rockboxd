module CI.Base where

import Effect (Effect)
import Prelude
import Data.Maybe (Maybe(..))
import Native (Client)
import Native.Container as Container
import Native.Directory as Directory
import Native.Query as Query
import Native.Types (Container, Directory, Platform(..))
import Effect.Exception (error, throwException)

-- Keep generated CI code, host caches, and secrets out of build inputs.
-- Never use **/target: firmware/target is part of Rockbox's source tree.
sourceExcludes :: Array String
sourceExcludes = [ ".ci-tooling", ".fluentci", ".git", ".dagger", ".devbox", ".github", "target", "build", "tools/bmp2rb", "tools/codepages", "tools/convbdf", "tools/mkboot", "tools/rdf2binary", "tools/uclpack", "tools/scramble", "desktop/target", "desktop-gpui/target", "bindings/**/target", "**/.zig-cache", "**/zig-out", "build-headless", "build-lib", "build-armhf", "build-android-arm64", "**/node_modules", "**/.env", "**/.env.*", "artifacts", "release-source", "dist/build", "dist/rpmbuild", "result" ]

sourceTree :: Client -> Directory -> Directory
sourceTree client source = Query.directory client
  # Directory.withDirectoryWith "." source
      (Directory.defaultWithDirectoryOpts
        { exclude = Just sourceExcludes })

platform :: String -> Effect Platform
platform value = case value of
  "linux/amd64" -> pure (Platform value)
  "linux/arm64" -> pure (Platform value)
  _ -> throwException (error "Supported platforms: linux/amd64 and linux/arm64")

container :: Client -> Platform -> String -> Container
container client arch image = Query.containerWith (Query.defaultContainerOpts { platform = Just arch }) client
  # Container.from image
