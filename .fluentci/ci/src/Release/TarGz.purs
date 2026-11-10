module Release.TarGz where

import Effect (Effect)
import Prelude
import Release.Common as Common
import Release.Sha256 as Sha256
import Data.Foldable (traverse_)
import Native (Client)
import Native.Container as C
import Native.Types (Container, Directory, File)

-- Compatibility helper for existing graphs with an /out directory.
archive :: String -> String -> Array String -> Container -> Container
archive name root members = C.withExec ([ "tar", "-czf", "/out/" <> name, "-C", root ] <> members)
  >>> C.withWorkdir "/out" >>> Sha256.checksum name >>> C.withWorkdir "/src"

create :: Client -> { source :: Directory, name :: String, members :: Array String } -> Effect File
create client { source, name, members } = do
  Common.filename name
  Common.ensure (members /= []) "Archive members must not be empty"
  traverse_ Common.relative members
  pure $ Common.base client # C.withDirectory "/input" source
    # C.withExec ([ "tar", "-czf", "/work/" <> name, "-C", "/input", "--" ] <> members)
    # C.file ("/work/" <> name)
