module CI.Android where

import Prelude
import CI.Build as Build
import CI.Graph as G
import Data.Array as A
import Data.Int as Int
import Data.Maybe (fromMaybe)
import Data.String as S
import Dagger.Container as C
import Dagger.Directory as D
import Dagger.Types (Container)
import Effect.Aff (Aff)

prepare :: Container -> Aff Container
prepare ctr = do
  versions <- D.entries (C.directory "/opt/android-sdk/ndk" ctr)
  G.ensure (not (A.null versions)) "Supply an Android SDK with a side-by-side Linux NDK"
  let
    numeric = map (fromMaybe 0 <<< Int.fromString) <<< S.split (S.Pattern ".")
    version = fromMaybe "" (A.last (A.sortBy (\a b -> compare (numeric a) (numeric b)) versions))
    path = "/opt/android-sdk/ndk/" <> version
  pure $ ctr # C.withEnvVariable "ANDROID_HOME" "/opt/android-sdk"
    # C.withEnvVariable "ANDROID_SDK_ROOT" "/opt/android-sdk"
    # C.withEnvVariable "ANDROID_NDK_HOME" path # C.withEnvVariable "ANDROID_NDK_ROOT" path
    # C.withEnvVariable "JAVA_HOME" "/usr/lib/jvm/java-17-openjdk-amd64"
    # C.withEnvVariable "RUSTFLAGS" "-A warnings"

firmware :: Container -> Aff Container
firmware ctr = do
  let configured = Build.web ctr
        # G.exec [ "mkdir", "-p", "/src/build-android-arm64" ]
        # C.withEnvVariable "ANDROID_TARGET_ABI" "arm64-v8a" # C.withEnvVariable "ANDROID_API_LEVEL" "26"
        # C.withEnvVariable "ARG_LCDWIDTH" "320" # C.withEnvVariable "ARG_LCDHEIGHT" "480"
        # C.withWorkdir "/src/build-android-arm64" # G.input "205\n" [ "../tools/configure" ]
  ncpu <- S.trim <$> C.stdout (G.exec [ "nproc" ] configured)
  pure $ configured # G.exec [ "make", "-j" <> ncpu, "lib" ] # C.withWorkdir "/src"

native :: String -> Container -> Container
native features = G.exec [ "cargo", "ndk", "-t", "arm64-v8a", "--platform", "26", "-o", "/src/expo/modules/rockbox-rpc/android/src/main/jniLibs", "build", "-p", "rockbox-expo", "--features", features, "--release" ]

apk :: String -> String -> Container -> Aff Container
apk features architectures ctr = do
  G.ensure (A.all (\abi -> A.elem abi [ "arm64-v8a", "armeabi-v7a", "x86_64" ]) (S.split (S.Pattern ",") architectures)) "Unsupported Android ABI"
  built <- firmware ctr
  pure $ built
    # G.at "/src/expo" [ "bun", "install" ]
    # G.at "/src/expo" [ "bunx", "expo", "prebuild", "--platform", "android", "--no-install" ]
    # C.withNewFile "/src/expo/android/local.properties" "sdk.dir=/opt/android-sdk\n"
    # native features
    # G.at "/src/expo/android" [ "./gradlew", "assembleRelease", "-PreactNativeArchitectures=" <> architectures, "--no-daemon", "--stacktrace" ]
