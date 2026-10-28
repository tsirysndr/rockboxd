module CI.Graph where

import Release.Sha256 as Sha256
import Release.TarGz as TarGz
import Prelude
import Data.Array as Array
import Data.Foldable (foldl, foldM)
import Data.Maybe (Maybe(..))
import Data.String as String
import Dagger (Client)
import Dagger.Container as C
import Dagger.Directory as D
import Dagger.File as F
import Dagger.Query as Q
import Dagger.Types (Container, Directory)
import Effect.Aff (Aff, throwError)
import Effect.Exception (error)

-- Commands are argv vectors, never programs interpreted by a shell.
exec :: Array String -> Container -> Container
exec = C.withExec

at :: String -> Array String -> Container -> Container
at path args container = container # C.withWorkdir path # exec args # C.withWorkdir "/src"

capture :: String -> Array String -> Container -> Container
capture path args = C.withExecWith args (C.defaultWithExecOpts { redirectStdout = Just path })

input :: String -> Array String -> Container -> Container
input value args = C.withExecWith args (C.defaultWithExecOpts { stdin = Just value })

ensure :: Boolean -> String -> Aff Unit
ensure condition message = unless condition (throwError (error message))

lines :: String -> Array String
lines = Array.filter (_ /= "") <<< String.split (String.Pattern "\n") <<< String.trim

replaceFile :: String -> (String -> String) -> Container -> Aff Container
replaceFile path transform container = do
  contents <- F.contents (C.file path container)
  pure (C.withNewFile path (transform contents) container)

replaceLine :: String -> String -> String -> String
replaceLine prefix value text = String.joinWith "\n" (map change (String.split (String.Pattern "\n") text))
  where
  change line = if String.take (String.length prefix) line == prefix then prefix <> value else line

files :: Client -> Container -> Array String -> Directory
files client container paths = foldl (\out path -> D.withFile path (C.file ("/src/" <> path) container) out) (Q.directory client) paths

globFiles :: Client -> Container -> String -> Array String -> Aff Directory
globFiles client container root patterns = do
  paths <- foldM (\acc pattern -> do
    matches <- D.glob pattern (C.directory root container)
    ensure (not (Array.null matches)) ("Missing artifacts: " <> pattern)
    pure (acc <> matches)) [] patterns
  pure (foldl (\out path -> D.withFile path (C.file (root <> "/" <> path) container) out) (Q.directory client) paths)

checksum :: String -> Container -> Container
checksum = Sha256.checksum

archive :: String -> String -> Array String -> Container -> Container
archive = TarGz.archive

report :: Client -> String -> Container -> Aff Directory
report client label container = do
  -- Force the terminal graph node before returning a successful test report.
  _ <- C.sync container
  pure (D.withNewFile (".dagger-results/" <> label <> ".txt") (label <> " passed\n") (Q.directory client))
