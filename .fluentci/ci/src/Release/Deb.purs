module Release.Deb where

import Effect (Effect)
import Prelude
import Release.Common as Common
import Native (Client)
import Native.Container as C
import Native.Types (Container, Directory, File)

buildIn :: String -> String -> Container -> Container
buildIn root output = C.withExec [ "dpkg-deb", "--build", "--root-owner-group", root, output ]

build :: Client -> { root :: Directory, name :: String } -> Effect File
build client { root, name } = do
  Common.filename name
  pure $ Common.base client # Common.apt [ "dpkg-dev" ]
    # C.withDirectory "/package" root # buildIn "/package" ("/work/" <> name)
    # C.file ("/work/" <> name)
