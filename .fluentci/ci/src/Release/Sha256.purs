module Release.Sha256 where

import Effect (Effect)
import Prelude
import Release.Common as Common
import Data.Maybe (Maybe(..))
import Native (Client)
import Native.Container as C
import Native.Types (Container, File)

checksum :: String -> Container -> Container
checksum path = C.withExecWith [ "sha256sum", path ] (C.defaultWithExecOpts { redirectStdout = Just (path <> ".sha256") })

generate :: Client -> { file :: File, name :: String } -> Effect File
generate client { file, name } = do
  Common.filename name
  pure $ Common.base client # C.withFile ("/work/" <> name) file
    # checksum ("./" <> name) # C.file (name <> ".sha256")
