module Native.Query where
import Prelude
import CI.Support as S
import FluentCI.Plugin as P
import Effect.Unsafe (unsafePerformEffect)
import Native.Types
import Native.Engine as E
import Data.Maybe (Maybe(..), fromMaybe)

defaultContainerOpts :: { platform :: Maybe Platform }
defaultContainerOpts = { platform: Nothing }
container :: Client -> Container
container = containerWith defaultContainerOpts
containerWith :: { platform :: Maybe Platform } -> Client -> Container
containerWith opts _ = case fromMaybe (nativePlatform unit) opts.platform of
  Platform arch -> E.node Nothing (Image "" arch)
directory :: Client -> Directory
directory _ = EmptyDirectory
cacheVolume :: String -> Client -> String
cacheVolume name _ = name
http :: String -> Client -> File
http url _ = Download url

nativePlatform :: Unit -> Platform
nativePlatform _ = unsafePerformEffect do
  arch <- P.getArch >>= S.unwrap
  pure (Platform (if arch == "aarch64" || arch == "arm64" then "linux/arm64" else "linux/amd64"))
