module CI.Image where

import Prelude
import CI.Base as Base
import Data.Maybe (Maybe(..))
import Dagger (Client)
import Dagger.Container as Container
import Dagger.Directory as Directory
import Dagger.Query as Query
import Dagger.Types (Directory, File, Platform(..))
import Effect.Aff (Aff)

-- Both variants are built by Dagger; registry credentials stay on the publisher.
archive :: Client -> { source :: Directory, tag :: String } -> Aff File
archive client input = do
  let
    source = Base.sourceTree client input.source
    variant platform = Directory.dockerBuildWith
      (Directory.defaultDockerBuildOpts
        { dockerfile = Just "Dockerfile"
        , platform = Just (Platform platform)
        , buildArgs = Just [ { name: "TAG", value: input.tag } ]
        }) source
  pure (Container.asTarballWith
    (Container.defaultAsTarballOpts
      { platformVariants = Just [ variant "linux/amd64", variant "linux/arm64" ] })
    (Query.container client))
