module CI.Legacy where

import Prelude
import CI.Build as Build
import CI.Graph as G
import CI.Toolchain as T
import Data.Foldable (foldl, foldM)
import Data.String as S
import Dagger (Client)
import Dagger.Container as C
import Dagger.Query as Q
import Dagger.Types (Container, Directory)
import Effect.Aff (Aff)

sdl :: Container -> Container
sdl ctr =
  let built = ctr # Build.audioDependencies # T.apt [ "libusb-dev", "libsdl2-dev", "libfreetype6-dev" ]
        # Build.web # G.exec [ "mkdir", "-p", "/src/build" ]
        # G.at "/src/build" [ "../tools/configure", "--target=sdlapp", "--type=N", "--lcdwidth=320", "--lcdheight=240", "--prefix=/usr/local" ]
        # G.at "/src/build" [ "make", "lib", "-j4" ]
        # G.exec [ "cargo", "build", "--locked", "-p", "rockbox-cli", "--release" ]
        # G.exec [ "cargo", "build", "--locked", "-p", "rockbox-server", "--release" ]
        # G.exec [ "cargo", "build", "--locked", "-p", "rockbox", "--release" ]
        # G.at "/src/zig" [ "zig", "build" ]
        # G.at "/src/build" [ "make", "install" ]
  in built # C.withFile "/usr/local/bin/rockbox" (C.file "/src/target/release/rockbox" built)
    # C.withFile "/usr/local/bin/rockboxd" (C.file "/src/zig/zig-out/bin/rockboxd" built)
    # C.withDirectory "/usr/local/share/rockbox" (C.directory "/src/assets" built)

release :: String -> String -> Container -> Directory
release arch tag ctr =
  let built = sdl ctr
  in built
    # G.archive ("rockbox_" <> tag <> "_" <> arch <> "-linux.tar.gz") "/usr/local/bin" [ "rockbox" ]
    # G.archive ("rockboxd_" <> tag <> "_" <> arch <> "-linux.tar.gz") "/usr/local/bin" [ "rockboxd" ]
    # G.archive ("rockbox-codecs-" <> tag <> "-" <> arch <> "-linux.tar.gz") "/usr/local/lib/rockbox" [ "." ]
    # G.archive ("rockbox-assets-" <> tag <> "-" <> arch <> "-linux.tar.gz") "/usr/local/share/rockbox" [ "." ]
    # C.directory "/out"

download :: Client -> String -> String -> String -> Container -> Container
download client tag arch debArch ctr = foldl fetch ctr
  [ { name: "rockbox_" <> tag <> "_" <> arch <> "-linux.tar.gz", path: "bin" }
  , { name: "rockboxd_" <> tag <> "_" <> arch <> "-linux.tar.gz", path: "bin" }
  , { name: "rockbox-codecs-" <> tag <> "-" <> arch <> "-linux.tar.gz", path: "lib/rockbox" }
  , { name: "rockbox-assets-" <> tag <> "-" <> arch <> "-linux.tar.gz", path: "share/rockbox" }
  ]
  where
  fetch current item =
    let url = "https://github.com/tsirysndr/rockboxd/releases/download/" <> tag <> "/" <> item.name
        dest = "/releases/" <> debArch <> "/usr/local/" <> item.path
    in current # C.withFile ("/tmp/" <> item.name) (Q.http url client)
      # C.withFile ("/tmp/" <> item.name <> ".sha256") (Q.http (url <> ".sha256") client)
      # G.at "/tmp" [ "sha256sum", "-c", item.name <> ".sha256" ]
      # G.exec [ "mkdir", "-p", dest ] # G.exec [ "tar", "-xzf", "/tmp/" <> item.name, "-C", dest ]

version :: String -> String
version tag = if S.take 1 tag == "v" then S.drop 1 tag else tag

deb :: Client -> String -> Container -> Aff Directory
deb client tag ctr = do
  built <- foldM (\current platform -> do
    let downloaded = download client tag platform.arch platform.deb current
        staged = downloaded # C.withoutDirectory "/pkg/deb"
          # C.withDirectory "/pkg/deb" (C.directory ("/src/dist/debian/" <> platform.deb) downloaded)
          # C.withDirectory "/pkg/deb/usr/local" (C.directory ("/releases/" <> platform.deb <> "/usr/local") downloaded)
    control <- G.replaceFile "/pkg/deb/DEBIAN/control" (G.replaceLine "Depends:" " libsdl2-2.0-0, libunwind8, libasound2, libdbus-1-3") staged
    Build.packageDebNamed (version tag) tag platform.deb control) ctr
    [ { arch: "x86_64", deb: "amd64" }, { arch: "aarch64", deb: "arm64" } ]
  pure (C.directory "/out" built)

rpm :: Client -> String -> Container -> Aff Directory
rpm client tag ctr = do
  let staged = download client tag "x86_64" "amd64" ctr
        # C.withDirectory "/pkg/rpm" (C.directory "/releases/amd64" (download client tag "x86_64" "amd64" ctr))
        # C.withNewFile "/pkg/rpm/rockbox.spec" ("Name: rockbox\nVersion: " <> version tag <> "\nRelease: 1\nSummary: Rockbox audio player and daemon\nLicense: GPL-2.0\nBuildArch: x86_64\nRequires: SDL2, libunwind, alsa-lib, dbus-libs\n%description\nRockbox SDL player, daemon, codecs and assets.\n%install\nmkdir -p %{buildroot}/usr\ncp -a %{_sourcedir}/amd64/usr/local %{buildroot}/usr/\n%files\n/usr/local/bin/*\n/usr/local/lib/rockbox\n/usr/local/share/rockbox\n")
  built <- Build.packageRpm (version tag) staged
  pure (C.directory "/out" built)
