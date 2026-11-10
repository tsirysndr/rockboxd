module Native.Container where
import Effect (Effect)
import Prelude
import Native.Images as Images
import Native.Types
import Native.Engine as E
import Data.Maybe (Maybe(..))
import Effect.Class (liftEffect)

defaultWithExecOpts :: ExecOpts
defaultWithExecOpts = { redirectStdout: Nothing, expect: Nothing }
defaultWithDirectoryOpts :: DirectoryOpts
defaultWithDirectoryOpts = { exclude: Nothing }
from :: String -> Container -> Container
from image (Container old) = case old.step of
  Image _ platform -> E.node Nothing (Image (Images.resolve image) platform)
  _ -> E.node Nothing (Image (Images.resolve image) "linux/amd64")
withExec :: Array String -> Container -> Container
withExec argv = withExecWith argv defaultWithExecOpts
withExecWith :: Array String -> ExecOpts -> Container -> Container
withExecWith argv opts ctr = E.node (Just ctr) (Execute argv opts)
withWorkdir :: String -> Container -> Container
withWorkdir path ctr = E.node (Just ctr) (Workdir path)
withEnvVariable :: String -> String -> Container -> Container
withEnvVariable name value ctr = E.node (Just ctr) (SetEnv name value)
withoutEnvVariable :: String -> Container -> Container
withoutEnvVariable name ctr = E.node (Just ctr) (UnsetEnv name)
withMountedCache :: String -> String -> Container -> Container
withMountedCache path name ctr = E.node (Just ctr) (Cache path name)
withDirectory :: String -> Directory -> Container -> Container
withDirectory path dir = withDirectoryWith path dir defaultWithDirectoryOpts
withDirectoryWith :: String -> Directory -> DirectoryOpts -> Container -> Container
withDirectoryWith path dir opts ctr = E.node (Just ctr) (CopyDirectory path dir opts)
withFile :: String -> File -> Container -> Container
withFile path file ctr = E.node (Just ctr) (CopyFile path file)
withNewFile :: String -> String -> Container -> Container
withNewFile path content ctr = E.node (Just ctr) (WriteFile path content)
withEntrypoint :: Array String -> Container -> Container
withEntrypoint argv ctr = E.node (Just ctr) (Entrypoint argv)
withExposedPort :: Int -> Container -> Container
withExposedPort port ctr = E.node (Just ctr) (Expose port)
directory :: String -> Container -> Directory
directory path ctr = ContainerDirectory ctr path
file :: String -> Container -> File
file path ctr = ContainerFile ctr path
stdout :: Container -> Effect String
stdout ctr = liftEffect (_.output <$> E.realize ctr)
stderr :: Container -> Effect String
stderr ctr = liftEffect (_.errors <$> E.realize ctr)
exitCode :: Container -> Effect Int
exitCode ctr = liftEffect (_.code <$> E.realize ctr)
sync :: Container -> Effect Container
sync ctr = liftEffect (E.realize ctr) $> ctr
exists :: String -> Container -> Effect Boolean
exists path ctr = do
  code <- exitCode (withExecWith [ "test", "-e", path ] (defaultWithExecOpts { expect = Just ReturnTypeAny }) ctr)
  pure (code == 0)
withoutDirectory :: String -> Container -> Container
withoutDirectory path = withExec [ "rm", "-rf", path ]
