module Release.Common where

import Effect (Effect)
import Prelude
import Data.Array as A
import Data.String as S
import Native (Client)
import Native.Container as C
import Native.Query as Q
import Native.Types (Container)
import Effect.Exception (error, throwException)

ensure :: Boolean -> String -> Effect Unit
ensure condition message = unless condition (throwException (error message))

filename :: String -> Effect Unit
filename name = ensure (name /= "" && name /= "." && name /= ".." && not (S.contains (S.Pattern "/") name) && not (S.contains (S.Pattern "\n") name) && not (S.contains (S.Pattern "\r") name)) "Expected a nonempty filename, without directories or line breaks"

relative :: String -> Effect Unit
relative path = ensure (path /= "" && S.take 1 path /= "/" && not (A.elem ".." (S.split (S.Pattern "/") path))) "Expected a relative path without parent traversal"

base :: Client -> Container
base client = Q.container client # C.from "ubuntu:24.04" # C.withWorkdir "/work"

apt :: Array String -> Container -> Container
apt packages = C.withExec [ "apt-get", "update" ] >>> C.withExec ([ "apt-get", "install", "-y", "--no-install-recommends" ] <> packages)
