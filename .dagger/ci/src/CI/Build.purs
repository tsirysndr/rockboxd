module CI.Build where

import Release.Deb as Deb
import Release.Rpm as Rpm
import Prelude
import CI.Graph as G
import CI.Firmware as Firmware
import CI.Toolchain as T
import Data.String as S
import Data.Foldable (foldl)
import Dagger (Client)
import Dagger.Container as C
import Dagger.Directory as D
import Dagger.Query as Q
import Dagger.Types (Container, Directory)
import Effect.Aff (Aff)

web :: Container -> Container
web = G.at "/src/webui/rockbox" [ "deno", "install" ]
  >>> G.at "/src/webui/rockbox" [ "deno", "task", "build" ]
  >>> G.at "/src/crates/s3/s3webui" [ "bun", "install", "--frozen-lockfile" ]
  >>> G.at "/src/crates/s3/s3webui" [ "bun", "run", "build" ]

audioDependencies :: Container -> Container
audioDependencies = T.apt [ "libunwind-dev", "libasound2-dev", "libdbus-1-dev", "protobuf-compiler" ]

desktopDependencies :: Container -> Container
desktopDependencies = T.apt [ "libfdk-aac-dev", "libfreetype6-dev", "libfontconfig1-dev", "libxkbcommon-dev", "libxkbcommon-x11-dev", "libxcb1-dev", "libxcb-render0-dev", "libxcb-shape0-dev", "libxcb-xfixes0-dev" ]

headless :: String -> Container -> Aff Container
headless arch = Firmware.headless arch <<< web <<< audioDependencies

cli :: Container -> Container
cli = G.exec [ "cargo", "build", "-p", "rockbox", "--release" ]

stageDesktop :: Container -> Container
stageDesktop container =
  let built = container # desktopDependencies # G.at "/src/desktop" [ "cargo", "build", "--release", "--locked" ]
  in built
    # C.withFile "/stage/desktop/usr/bin/rockbox-desktop" (C.file "/src/desktop/target/release/rockbox-desktop" built)
    # C.withFile "/stage/desktop/usr/share/pixmaps/rockbox-desktop.png" (C.file "/src/macos/Rockbox/Assets.xcassets/AppIcon.appiconset/256.png" built)
    # C.withNewFile "/stage/desktop/usr/share/applications/rockbox-desktop.desktop" "[Desktop Entry]\nName=Rockbox\nComment=Modern audio player with multi-room support\nExec=rockbox-desktop\nIcon=rockbox-desktop\nTerminal=false\nType=Application\nCategories=AudioVideo;Audio;Player;\n"

stageBinaries :: Container -> Container
stageBinaries ctr = ctr
  # C.withFile "/stage/bin/rockbox" (C.file "/src/target/release/rockbox" ctr)
  # C.withFile "/stage/bin/rockboxd" (C.file "/src/zig/zig-out/bin/rockboxd" ctr)

embeddedArchive :: String -> Container -> Container
embeddedArchive name ctr = ctr
  # C.withFile "/stage/embedded/lib/librockboxd.a" (C.file "/src/zig/zig-out/lib/librockboxd.a" ctr)
  # C.withFile "/stage/embedded/include/rockboxd.h" (C.file "/src/include/rockboxd.h" ctr)
  # G.archive name "/stage/embedded" [ "lib", "include" ]

withTypesense :: Client -> String -> Container -> Container
withTypesense client arch = C.withFile "/tmp/typesense.tar.gz"
  (Q.http ("https://dl.typesense.org/releases/30.1/typesense-server-30.1-linux-" <> arch <> ".tar.gz") client)
  >>> G.exec [ "mkdir", "-p", "/tmp/typesense" ]
  >>> G.exec [ "tar", "-xzf", "/tmp/typesense.tar.gz", "-C", "/tmp/typesense" ]

packageDeb :: String -> String -> Container -> Aff Container
packageDeb version = packageDebNamed version version

packageDebNamed :: String -> String -> String -> Container -> Aff Container
packageDebNamed metadataVersion version arch ctr = do
  let root = "/pkg/deb"
  stamped <- G.replaceFile (root <> "/DEBIAN/control") (G.replaceLine "Version:" (" " <> metadataVersion)) ctr
  pure $ stamped
    # Deb.buildIn root ("/out/rockbox_" <> version <> "_" <> arch <> ".deb")
    # C.withWorkdir "/out" # G.checksum ("rockbox_" <> version <> "_" <> arch <> ".deb") # C.withWorkdir "/src"

packageRpm :: String -> Container -> Aff Container
packageRpm version ctr = do
  stamped <- G.replaceFile "/pkg/rpm/rockbox.spec" (G.replaceLine "Version:" (" " <> version)) ctr
  let built = stamped
        # C.withDirectory "/tmp/rpmbuild/SOURCES/amd64" (C.directory "/pkg/rpm" stamped)
        # Rpm.buildIn "/pkg/rpm/rockbox.spec" "x86_64"
  paths <- D.glob "**/*.rpm" (C.directory "/tmp/rpmbuild/RPMS" built)
  G.ensure (paths /= []) "rpmbuild produced no packages"
  pure (foldl (\out path ->
    let name = basename path
    in out # C.withFile ("/out/" <> name) (C.file ("/tmp/rpmbuild/RPMS/" <> path) built)
      # C.withWorkdir "/out" # G.checksum name # C.withWorkdir "/src") built paths)

nativeRelease :: Client -> String -> String -> Container -> Aff Directory
nativeRelease client arch version ctr = do
  head <- headless arch ctr
  lib <- Firmware.embedded arch (cli head)
  let
    amd = arch == "x86_64"
    debArch = if amd then "amd64" else "arm64"
    archiveArch = if amd then "amd64" else "aarch64"
    app = (if amd then stageDesktop lib else lib) # withTypesense client debArch # T.apt [ "dpkg-dev", "rpm" ]
    staged = app # stageBinaries
      # C.withDirectory "/pkg/deb" (C.directory ("/src/dist/debian/" <> debArch) app)
      # C.withDirectory "/pkg/deb/usr/local/bin" (C.directory "/stage/bin" (stageBinaries app))
      # C.withFile "/pkg/deb/usr/local/bin/typesense-server" (C.file "/tmp/typesense/typesense-server" app)
    desktop = if amd then staged # C.withDirectory "/pkg/deb/usr" (C.directory "/stage/desktop/usr" app) else staged
    archives = desktop
      # G.archive ("rockbox_" <> version <> "_" <> archiveArch <> "-linux.tar.gz") "/stage/bin" [ "rockbox", "rockboxd" ]
      # embeddedArchive ("librockboxd_" <> version <> "_" <> archiveArch <> "-linux.tar.gz")
  deb <- packageDeb version debArch archives
  result <- if amd then packageRpm version (deb
    # C.withDirectory "/pkg/rpm" (C.directory "/src/dist/rpm/amd64" deb)
    # C.withDirectory "/pkg/rpm/usr" (C.directory "/pkg/deb/usr" deb)) else pure deb
  pure (C.directory "/out" result)

armhfRelease :: String -> Container -> Aff Directory
armhfRelease version ctr = do
  built <- Firmware.armhf (web ctr)
  let staged = built # T.apt [ "dpkg-dev" ]
        # C.withDirectory "/pkg/deb" (C.directory "/src/dist/debian/armhf" built)
        # C.withFile "/pkg/deb/usr/local/bin/rockboxd" (C.file "/src/zig/zig-out/bin/rockboxd" built)
        # G.archive ("rockbox_" <> version <> "_armhf-linux.tar.gz") "/src/zig/zig-out/bin" [ "rockboxd" ]
  packaged <- packageDeb version "armhf" staged
  pure (C.directory "/out" packaged)

-- Keep filenames independent from shell word splitting and glob expansion.
basename :: String -> String
basename path = foldl (\_ part -> part) "" (S.split (S.Pattern "/") path)
