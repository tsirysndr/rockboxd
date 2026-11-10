module Native.Types where

import Prelude
import Data.Maybe (Maybe)
import Effect.Ref (Ref)

-- Immutable recipes interpreted by the local Docker CLI. Checkpoints belong to
-- one invocation and are disposed even when a build fails.
type Client = Unit
newtype Platform = Platform String
data ExistsType = ExistsTypeRegularType
data ReturnType = ReturnTypeAny
type ExecOpts = { redirectStdout :: Maybe String, expect :: Maybe ReturnType }
type DirectoryOpts = { exclude :: Maybe (Array String) }
type Environment = Array { name :: String, value :: String }
type Mount = { path :: String, name :: String }
type Running = { id :: String, platform :: String, cwd :: String, env :: Environment, removed :: Array String, mounts :: Array Mount, output :: String, errors :: String, code :: Int }
data Container = Container { parent :: Maybe Container, step :: Step, result :: Ref (Maybe Running) }
data Step = Image String String | Workdir String | SetEnv String String | UnsetEnv String | Cache String String | Execute (Array String) ExecOpts | CopyDirectory String Directory DirectoryOpts | CopyFile String File | WriteFile String String | Entrypoint (Array String) | Expose Int
data Directory = LocalDirectory String | ContainerDirectory Container String | EmptyDirectory | Subdirectory Directory String | AddDirectory Directory String Directory DirectoryOpts | AddFile Directory String File | NewFile Directory String String
data File = LocalFile String | ContainerFile Container String | DirectoryFile Directory String | Download String
