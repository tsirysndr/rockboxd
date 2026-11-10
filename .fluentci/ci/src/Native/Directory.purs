module Native.Directory where
import Effect (Effect)
import Prelude
import Native.Types
import Native.Engine as E
import CI.Support as S
import Data.Argonaut.Core as J
import Foreign.Object as Object
import FluentCI.Host.Files as Files
import Effect.Exception (throw)
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
  result <- S.taskResult (Files.exists "exists" path root)
  case result.file_result >>= J.toBoolean of
    Nothing -> throw "Native file existence query returned no boolean"
    Just false -> pure false
    Just true -> case opts.expectedType of
      Nothing -> pure true
      Just ExistsTypeRegularType -> do
        metadata <- S.taskResult (Files.stat "stat" path root)
        pure ((metadata.file_result >>= J.toObject >>= Object.lookup "kind" >>= J.toString) == Just "file")

glob :: String -> Directory -> Effect (Array String)
glob = E.glob
withDirectory :: String -> Directory -> Directory -> Directory
withDirectory path added = withDirectoryWith path added defaultWithDirectoryOpts
