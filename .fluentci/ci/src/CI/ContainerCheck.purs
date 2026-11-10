module CI.ContainerCheck where

import Prelude
import CI.Support as S
import Native.Container as C
import Native.Directory as D
import Native.Engine as E
import Native.File as F
import Native.Query as Q
import Native.Types (Directory(..), ReturnType(..))
import Data.Maybe (Maybe(..))
import Effect (Effect)
import Effect.Class (liftEffect)
import Effect.Exception (throw)

-- A tiny real-container contract check: no application compilation or Nix.
check :: Effect Unit
check = E.runJob do
  root <- liftEffect E.temporary
  liftEffect do
    S.run [ "mkdir", "-p", root <> "/source/firmware/target" ]
    S.writeText (root <> "/source/.env") "excluded secret\n"
    S.writeText (root <> "/source/firmware/target/keep.h") "source code\n"
  let base = Q.container unit # C.from "alpine:3.23" # C.withWorkdir "/work"
        # C.withDirectoryWith "/input" (LocalDirectory (root <> "/source")) (C.defaultWithDirectoryOpts { exclude = Just [ "target", ".env" ] })
        # C.withEnvVariable "LITERAL" "spaces; $(false) $HOME"
      branch value = base # C.withExecWith [ "printf", "%s", value ] (C.defaultWithExecOpts { redirectStdout = Just "/work/result" })
      left = branch "left; $(false)"
      right = branch "right"
  a <- F.contents (C.file "/work/result" left)
  b <- F.contents (C.file "/work/result" right)
  again <- F.contents (C.file "/work/result" left)
  environment <- C.stdout (base # C.withExec [ "printenv", "LITERAL" ])
  kept <- D.exists "firmware/target/keep.h" (C.directory "/input" base)
  excluded <- D.exists ".env" (C.directory "/input" base)
  status <- C.exitCode (base # C.withExecWith [ "false" ] (C.defaultWithExecOpts { expect = Just ReturnTypeAny }))
  liftEffect do
    unless (a == "left; $(false)" && again == a && b == "right") (throw "Container branches changed one another")
    unless (environment == "spaces; $(false) $HOME\n") (throw "Environment was interpreted as shell code")
    unless (kept && not excluded && status /= 0) (throw "Source filtering or exit status handling failed")
