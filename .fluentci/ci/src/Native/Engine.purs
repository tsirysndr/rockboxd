module Native.Engine where

import Prelude
import CI.Support as S
import Data.Argonaut.Core as J
import Foreign.Object as Object
import FluentCI.Host.Files as Files
import Native.Types
import Data.Array as A
import Data.Either (Either(..), either)
import Data.Foldable (foldM, traverse_)
import Data.Maybe (Maybe(..), fromMaybe)
import Data.String as String
import Data.String.CodeUnits as CU
import Data.String.Regex as Regex
import Data.String.Regex.Flags (noFlags)
import Effect (Effect)
import Effect.Class (liftEffect)
import Effect.Exception (catchException, throw, throwException, try)
import Effect.Ref as Ref
import Effect.Unsafe (unsafePerformEffect)
import FluentCI.Host.Plan as H
import FluentCI.Plugin as P

-- Only memo-cell allocation is hidden behind the pure recipe constructors.
-- All filesystem/process operations occur explicitly in Effect/Effect.
node :: Maybe Container -> Step -> Container
node parent step = unsafePerformEffect do
  result <- Ref.new Nothing
  pure (Container { parent, step, result })

resources :: Ref.Ref { containers :: Array String, images :: Array String, temporary :: Array String }
resources = unsafePerformEffect (Ref.new { containers: [], images: [], temporary: [] })

temporary :: Effect String
temporary = do
  path <- String.trim <$> S.captureIn "." [ "mktemp", "-d" ]
  Ref.modify_ (\old -> old { temporary = old.temporary <> [ path ] }) resources
  pure path

cleanup :: Effect Unit
cleanup = do
  owned <- Ref.read resources
  let best command = catchException (const (pure unit)) (S.run command)
  traverse_ (\id -> best [ "docker", "rm", "-f", id ]) (A.reverse owned.containers)
  traverse_ (\id -> best [ "docker", "image", "rm", "-f", id ]) (A.reverse owned.images)
  traverse_ (\path -> best [ "rm", "-rf", "--", path ]) owned.temporary
  Ref.write { containers: [], images: [], temporary: [] } resources

runJob :: Effect Unit -> Effect Unit
runJob job = do
  result <- try job
  cleanup
  either throwException pure result

join :: String -> String -> String
join root path = if String.take 1 path == "/" then path else root <> "/" <> path

mkdir :: String -> Effect Unit
mkdir path = S.run [ "mkdir", "-p", path ]

copyFile :: String -> String -> Effect Unit
copyFile source destination = do
  parent <- String.trim <$> S.captureIn "." [ "dirname", destination ]
  mkdir parent
  S.run [ "cp", "-p", source, destination ]

copyTree :: String -> String -> DirectoryOpts -> Effect Unit
copyTree source destination opts = do
  scratch <- temporary
  let archive = scratch <> "/tree.tar"
  mkdir destination
  -- tar preserves executable modes and symlinks; exclusions apply before any
  -- checkout is copied into a container (including secrets and generated SDKs).
  S.run ([ "tar", "-C", source ] <> A.concatMap (\p -> [ "--exclude=" <> p, "--exclude=./" <> p ]) (fromMaybe [] opts.exclude) <> [ "-cf", archive, "." ])
  S.run [ "tar", "-C", destination, "-xf", archive ]

materializeFile :: File -> Effect String
materializeFile = case _ of
  LocalFile path -> pure path
  DirectoryFile directory path -> join <$> materializeDirectory directory <*> pure path
  Download url -> do
    root <- temporary
    let path = root <> "/download"
    S.run [ "curl", "--fail", "--location", "--retry", "3", "--output", path, url ]
    pure path
  ContainerFile recipe path -> do
    running <- realize recipe
    root <- temporary
    let destination = root <> "/" <> S.basename path
    S.run [ "docker", "cp", running.id <> ":" <> join running.cwd path, destination ]
    pure destination

materializeDirectory :: Directory -> Effect String
materializeDirectory = case _ of
  LocalDirectory path -> S.absolute path
  EmptyDirectory -> temporary
  Subdirectory directory path -> join <$> materializeDirectory directory <*> pure path
  ContainerDirectory recipe path -> do
    running <- realize recipe
    root <- temporary
    S.run [ "docker", "cp", running.id <> ":" <> join running.cwd path <> "/.", root ]
    pure root
  AddDirectory base path source opts -> do
    root <- temporary
    previous <- materializeDirectory base
    copyTree previous root { exclude: Nothing }
    added <- materializeDirectory source
    copyTree added (join root path) opts
    pure root
  AddFile base path file -> do
    root <- temporary
    previous <- materializeDirectory base
    copyTree previous root { exclude: Nothing }
    source <- materializeFile file
    copyFile source (join root path)
    pure root
  NewFile base path content -> do
    root <- temporary
    previous <- materializeDirectory base
    copyTree previous root { exclude: Nothing }
    parent <- String.trim <$> S.captureIn "." [ "dirname", join root path ]
    mkdir parent
    S.writeText (join root path) content
    pure root

-- Resolve copied inputs first. They can checkpoint an ancestor, so the main
-- recipe then resumes from that checkpoint instead of rebuilding its prefix.
dependencies :: Step -> Effect Unit
dependencies = case _ of
  CopyDirectory _ directory _ -> prepareDirectory directory
  CopyFile _ file -> prepareFile file
  _ -> pure unit
  where
  prepareFile = case _ of
    ContainerFile ctr _ -> void (realize ctr)
    DirectoryFile dir _ -> prepareDirectory dir
    _ -> pure unit
  prepareDirectory = case _ of
    ContainerDirectory ctr _ -> void (realize ctr)
    Subdirectory dir _ -> prepareDirectory dir
    AddDirectory base _ dir _ -> prepareDirectory base *> prepareDirectory dir
    AddFile base _ file -> prepareDirectory base *> prepareFile file
    NewFile base _ _ -> prepareDirectory base
    _ -> pure unit

pending :: Container -> Effect { base :: Maybe Running, steps :: Array Step }
pending (Container recipe) = do
  cached <- Ref.read recipe.result
  case cached of
    Just running -> pure { base: Just running, steps: [] }
    Nothing -> case recipe.parent of
      Nothing -> pure { base: Nothing, steps: [ recipe.step ] }
      Just parent -> do
        prefix <- pending parent
        pure (prefix { steps = prefix.steps <> [ recipe.step ] })

realize :: Container -> Effect Running
realize ctr@(Container recipe) = do
  cached <- Ref.read recipe.result
  case cached of
    Just running -> pure running
    Nothing -> do
      before <- pending ctr
      traverse_ dependencies before.steps
      plan <- pending ctr
      case plan.steps of
        [] -> case plan.base of
          Just running -> pure running
          Nothing -> throw "Empty container recipe"
        steps -> do
          let mounts = A.foldl (\acc step -> case step of
                Cache path name -> A.filter (\entry -> entry.path /= path) acc <> [ { path, name } ]
                _ -> acc) (maybeMounts plan.base) steps
          result <- case A.unsnoc steps of
            Nothing -> throw "Empty container recipe"
            Just { init, last } -> case recipe.parent of
              Nothing -> start plan.base mounts steps >>= (\running -> execute running last)
              Just (Container parent) -> do
                prefix <- if A.null init then case plan.base of
                  Just running -> pure running
                  Nothing -> throw "Missing container prefix"
                else do
                  initial <- start plan.base mounts steps
                  foldM execute initial init
                Ref.write (Just prefix) parent.result
                child <- start (Just prefix) mounts [ last ]
                execute child last
          Ref.write (Just result) recipe.result
          pure result
  where
  maybeMounts = case _ of
    Nothing -> []
    Just running -> running.mounts

start :: Maybe Running -> Array Mount -> Array Step -> Effect Running
start previous mounts steps = do
  let source = fromMaybe { image: "", platform: "" } (A.head (A.mapMaybe (case _ of
        Image image platform -> Just { image, platform }
        _ -> Nothing) steps))
  base <- case previous of
    Nothing -> pure { image: source.image, platform: source.platform, cwd: "/", env: [], removed: [], output: "", errors: "", code: 0 }
    Just running -> do
      image <- String.trim <$> S.captureIn "." [ "docker", "commit", running.id ]
      Ref.modify_ (\old -> old { images = old.images <> [ image ] }) resources
      pure { image, platform: running.platform, cwd: running.cwd, env: running.env, removed: running.removed, output: running.output, errors: running.errors, code: running.code }
  when (base.image == "") (throw "Container requires an image")
  id <- String.trim <$> S.captureIn "." ([ "docker", "create", "--platform", base.platform, "--entrypoint", "/bin/sleep" ]
    <> A.concatMap (\mount -> [ "--mount", "type=volume,source=" <> volumeName mount.name <> ",target=" <> mount.path ]) mounts
    <> [ base.image, "infinity" ])
  Ref.modify_ (\old -> old { containers = old.containers <> [ id ] }) resources
  S.run [ "docker", "start", id ]
  pure { id, platform: base.platform, cwd: base.cwd, env: base.env, removed: base.removed, mounts, output: base.output, errors: base.errors, code: base.code }

volumeName :: String -> String
volumeName name = "fluentci-" <> CU.fromCharArray (map (\c -> if A.elem c [ '/', ':', ' ' ] then '-' else c) (CU.toCharArray name))

dockerExec :: Running -> Array String -> Effect { output :: String, errors :: String, code :: Int }
dockerExec running argv = do
  scratch <- temporary
  let task = H.task "container" "docker" "." # H.withArgs
        ([ "exec", "--workdir", running.cwd ] <> A.concatMap (\item -> [ "--env", item.name <> "=" <> item.value ]) running.env
          <> [ running.id, "env" ] <> map ("--unset=" <> _) running.removed <> argv)
  result <- P.runPlan (scratch <> "/run") (H.plan [ task # H.allowFailure true ] # H.withInheritedEnv true) >>= S.unwrap
  case A.head result.tasks of
    Nothing -> throw "Docker command returned no result"
    Just completed -> do
      output <- readLog completed.stdout
      errors <- readLog completed.stderr
      pure { output, errors, code: fromMaybe 1 completed.exit_code }

execute :: Running -> Step -> Effect Running
execute running = case _ of
  Image _ _ -> pure running
  Workdir path -> do
    _ <- checked running [ "mkdir", "-p", path ]
    pure (running { cwd = join running.cwd path })
  SetEnv name value -> pure (running { env = A.filter (\item -> item.name /= name) running.env <> [ { name, value } ], removed = A.filter (_ /= name) running.removed })
  UnsetEnv name -> pure (running { env = A.filter (\item -> item.name /= name) running.env, removed = running.removed <> [ name ] })
  Cache _ _ -> pure running
  Execute argv opts -> do
    result <- dockerExec running argv
    case opts.expect of
      Nothing -> when (result.code /= 0) (throw ("Container command failed: " <> String.joinWith " " argv <> "\n" <> result.errors <> result.output))
      Just ReturnTypeAny -> pure unit
    case opts.redirectStdout of
      Nothing -> pure unit
      Just path -> writeContainer running path result.output
    pure (running { output = result.output, errors = result.errors, code = result.code })
  CopyFile path file -> do
    source <- materializeFile file
    let destination = join running.cwd path
    parent <- String.trim <$> S.captureIn "." [ "dirname", destination ]
    _ <- checked running [ "mkdir", "-p", parent ]
    S.run [ "docker", "cp", source, running.id <> ":" <> destination ]
    pure running
  CopyDirectory path directory opts -> do
    source <- materializeDirectory directory
    filtered <- temporary
    copyTree source filtered opts
    let destination = join running.cwd path
    _ <- checked running [ "mkdir", "-p", destination ]
    S.run [ "docker", "cp", filtered <> "/.", running.id <> ":" <> destination ]
    pure running
  WriteFile path content -> writeContainer running path content *> pure running
  Entrypoint _ -> pure running
  Expose _ -> pure running

checked :: Running -> Array String -> Effect String
checked running argv = do
  result <- dockerExec running argv
  when (result.code /= 0) (throw (result.errors <> result.output))
  pure result.output

writeContainer :: Running -> String -> String -> Effect Unit
writeContainer running path content = do
  root <- temporary
  let file = root <> "/content"
      destination = join running.cwd path
  S.writeText file content
  parent <- String.trim <$> S.captureIn "." [ "dirname", destination ]
  _ <- checked running [ "mkdir", "-p", parent ]
  S.run [ "docker", "cp", file, running.id <> ":" <> destination ]

exportDirectory :: String -> Directory -> Effect Unit
exportDirectory destination directory = liftEffect do
  source <- materializeDirectory directory
  copyTree source destination { exclude: Nothing }

globRegex :: String -> String
globRegex pattern = "^" <> go (CU.toCharArray pattern) <> "$"
  where
  go chars = case A.uncons chars of
    Nothing -> ""
    Just { head: '*', tail } -> case A.uncons tail of
      Just { head: '*', tail: rest } -> case A.uncons rest of
        Just { head: '/', tail: more } -> "(?:.*/)?" <> go more
        _ -> ".*" <> go rest
      _ -> "[^/]*" <> go tail
    Just { head: '?', tail } -> "[^/]" <> go tail
    Just { head, tail } -> (if A.elem head [ '.', '+', '(', ')', '[', ']', '{', '}', '^', '$', '|', '\\' ] then "\\" else "") <> CU.singleton head <> go tail

glob :: String -> Directory -> Effect (Array String)
glob pattern directory = liftEffect do
  root <- materializeDirectory directory
  paths <- S.files root
  expression <- either (throw <<< show) pure (Regex.regex (globRegex pattern) noFlags)
  pure (A.filter (Regex.test expression) (map (String.drop (String.length root + 1)) paths))

-- The OCI archive keeps both architectures and runtime metadata. The generated
-- Dockerfile only assembles already-built files; build logic stays in PureScript.
exportImage :: String -> Array { arch :: String, container :: Container } -> Effect Unit
exportImage destination variants = liftEffect do
  context <- temporary
  traverse_ (\variant -> do
    running <- realize variant.container
    let root = context <> "/" <> variant.arch
        archive = context <> "/" <> variant.arch <> ".tar"
    mkdir root
    S.run [ "docker", "export", "--output", archive, running.id ]
    S.run [ "tar", "-C", root, "-xf", archive ]
    S.run [ "rm", "--", archive ]) variants
  S.writeText (context <> "/Dockerfile")
    ("ARG TARGETARCH\n" <> String.joinWith "" (map (\variant -> "FROM scratch AS runtime-" <> variant.arch <> "\nCOPY " <> variant.arch <> "/ /\n") variants)
      <> "FROM runtime-${TARGETARCH}\nENV PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\nENTRYPOINT [\"/usr/local/bin/entrypoint.sh\"]\nEXPOSE 6061 6062 6063 6600 7882 1704 1705 1780 4533 8096\n")
  S.run [ "docker", "buildx", "build", "--platform", String.joinWith "," (map (\variant -> "linux/" <> variant.arch) variants), "--output", "type=oci,dest=" <> destination, context ]

readExact :: String -> Effect String
readExact path = do
  directory <- String.trim <$> S.captureIn "." [ "dirname", path ]
  task <- S.taskResult (Files.readText "read" (S.basename path) 1048576 directory)
  case task.file_result >>= J.toString of
    Nothing -> throw "Native read_text returned no text"
    Just value -> pure value

-- Build logs may exceed the native read_text limit. They are already streamed
-- by the host; retain a bounded tail for diagnostics and short query results.
readLog :: String -> Effect String
readLog path = do
  directory <- String.trim <$> S.captureIn "." [ "dirname", path ]
  task <- S.taskResult (Files.stat "stat" (S.basename path) directory)
  let size = task.file_result >>= J.toObject >>= Object.lookup "size" >>= J.toNumber
  if fromMaybe 0.0 size > 1048576.0 then S.captureIn "." [ "tail", "-c", "1048576", path ] else readExact path
