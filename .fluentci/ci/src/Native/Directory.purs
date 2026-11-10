module Native.Directory where
import Effect (Effect)
import Prelude
import Native.Types
import Native.Engine as E
import CI.Support as S
import Data.Array as A
import Data.Maybe (Maybe(..))
import Data.String as String
import Effect.Class (liftEffect)

defaultWithDirectoryOpts :: DirectoryOpts
defaultWithDirectoryOpts = { exclude: Nothing }
defaultExistsOpts :: { expectedType :: Maybe ExistsType }
defaultExistsOpts = { expectedType: Nothing }
directory :: String -> Directory -> Directory
directory path base = Subdirectory base path
file :: String -> Directory -> File
file path base = DirectoryFile base path
withDirectoryWith :: String -> Directory -> DirectoryOpts -> Directory -> Directory
withDirectoryWith path added opts base = AddDirectory base path added opts
withFile :: String -> File -> Directory -> Directory
withFile path added base = AddFile base path added
withNewFile :: String -> String -> Directory -> Directory
withNewFile path content base = NewFile base path content
entries :: Directory -> Effect (Array String)
entries dir = liftEffect do
  root <- E.materializeDirectory dir
  text <- S.captureIn "." [ "find", root, "-mindepth", "1", "-maxdepth", "1" ]
  pure (map S.basename (A.filter (_ /= "") (String.split (String.Pattern "\n") (String.trim text))))
exists :: String -> Directory -> Effect Boolean
exists path = existsWith path defaultExistsOpts
existsWith :: String -> { expectedType :: Maybe ExistsType } -> Directory -> Effect Boolean
existsWith path opts dir = liftEffect do
  root <- E.materializeDirectory dir
  text <- S.captureIn "." ([ "find", root ] <> (case opts.expectedType of
    Just ExistsTypeRegularType -> [ "-type", "f" ]
    Nothing -> []))
  pure (A.elem (E.join root path) (String.split (String.Pattern "\n") (String.trim text)))
glob :: String -> Directory -> Effect (Array String)
glob = E.glob
withDirectory :: String -> Directory -> Directory -> Directory
withDirectory path added = withDirectoryWith path added defaultWithDirectoryOpts
