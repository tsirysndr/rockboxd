module CI.Format where

import Effect (Effect)
import Prelude
import CI.Graph as G
import Data.Argonaut.Decode (decodeJson)
import Data.Argonaut.Parser (jsonParser)
import Data.Array as A
import Data.Either (Either(..))
import Data.Maybe (Maybe(..))
import Data.String as S
import Native (Client)
import Native.Container as C
import Native.Types (Container, Directory)
import Effect.Exception (error, throwException)

type Metadata =
  { workspace_root :: String
  , workspace_members :: Array String
  , packages :: Array { id :: String, name :: String, manifest_path :: String }
  }

-- Cargo auto-enrolls local path dependencies as workspace members, including
-- Deno's crates. Select by manifest location, not by package-name conventions:
-- Rockbox also owns packages with names such as netstream.
packages :: Metadata -> Array String
packages metadata = map _.name (A.filter owned metadata.packages)
  where
  owned package = A.elem package.id metadata.workspace_members && case S.stripPrefix (S.Pattern (metadata.workspace_root <> "/")) package.manifest_path of
    Nothing -> false
    Just path -> not (S.contains (S.Pattern "../") path)
      && not (startsWith "deno/" path || startsWith "rmpc/" path)
  startsWith prefix path = S.take (S.length prefix) path == prefix

command :: Array String -> Array String
command names = [ "cargo", "fmt", "--check" ] <> A.concatMap (\name -> [ "--package", name ]) names

check :: Client -> Container -> Effect Directory
check client container = do
  raw <- C.stdout (G.exec [ "cargo", "metadata", "--no-deps", "--format-version", "1", "--offline" ] container)
  metadata <- case jsonParser raw of
    Left message -> throwException (error ("Invalid Cargo metadata: " <> message))
    Right json -> case decodeJson json of
      Left message -> throwException (error ("Invalid Cargo workspace metadata: " <> show message))
      Right value -> pure value
  let names = packages metadata
  G.ensure (not (A.null names)) "No Rockbox workspace packages selected for formatting"
  G.report client "format" (G.exec (command names) container)
