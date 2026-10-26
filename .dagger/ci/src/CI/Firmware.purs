module CI.Firmware where

import Prelude
import CI.Graph as G
import Data.Array as A
import Data.Foldable (foldl)
import Data.String as S
import Dagger.Container as C
import Dagger.Directory as D
import Dagger.Types (Container)
import Effect.Aff (Aff)

codecNames :: Array String
codecNames = S.split (S.Pattern " ") "a52 a52_rm aac aac_bsf adx aiff alac ape atrac3_oma atrac3_rm au cook flac mod mpa mpc opus raac shorten smaf speex tta vorbis vox wav wav64 wavpack wma wmapro"

oggSymbols :: Array String
oggSymbols = S.split (S.Pattern " ") "ogg_packet_clear ogg_page_bos ogg_page_checksum_set ogg_page_continued ogg_page_eos ogg_page_granulepos ogg_page_packets ogg_page_pageno ogg_page_serialno ogg_page_version ogg_stream_check ogg_stream_clear ogg_stream_destroy ogg_stream_eos ogg_stream_flush ogg_stream_flush_fill ogg_stream_init ogg_stream_iovecin ogg_stream_packetin ogg_stream_packetout ogg_stream_packetpeek ogg_stream_pagein ogg_stream_pageout ogg_stream_pageout_fill ogg_stream_reset ogg_stream_reset_serialno ogg_sync_buffer ogg_sync_check ogg_sync_clear ogg_sync_destroy ogg_sync_init ogg_sync_pageout ogg_sync_pageseek ogg_sync_reset ogg_sync_wrote"

-- Make remains the project's compiler driver. CI's branching and archive work
-- happen here; there is no build-headless.sh or shell pipeline invocation.
staticLibraries :: Boolean -> Container -> Aff Container
staticLibraries cross container = do
  let
    root = if cross then "/src/build-armhf" else "/src/build-headless"
    target = if cross then "208" else "206"
    prefix = if cross then "arm-linux-gnueabihf-" else ""
    cc = if cross then prefix <> "gcc" else "cc"
    flags = if cross then [ "-march=armv6", "-marm", "-mfpu=vfp", "-mfloat-abi=hard" ] else []
    clean = container # G.exec [ "rm", "-rf", root ] # G.exec [ "mkdir", "-p", root ]
    configured = clean # C.withWorkdir root # G.input (target <> "\n\n\nN\n") [ "../tools/configure" ]
  ncpu <- S.trim <$> C.stdout (G.exec [ "nproc" ] configured)
  let
    built = configured
      # G.exec [ "make", "-j" <> ncpu, "lib" ]
      # G.exec [ "make", "-j" <> ncpu, root <> "/lib/rbcodec/codecs/libcodec.a" ]
      # C.withWorkdir "/src"
      # G.exec ([ cc, "-c", "-O2", "-std=gnu11" ] <> flags <> [ "scripts/isoc23-compat.c", "-o", "/tmp/isoc23-compat.o" ])
    shim = (if cross then G.exec [ prefix <> "objcopy", "--remove-section=.ARM.attributes", "/tmp/isoc23-compat.o" ] built else built)
      # G.exec [ prefix <> "ar", "r", root <> "/firmware/libfirmware.a", "/tmp/isoc23-compat.o" ]
    codecs = root <> "/lib/rbcodec/codecs"
    extract ctr name = ctr
      # G.exec [ "mkdir", "-p", codecs <> "/codec-objects/" <> name ]
      # G.at (codecs <> "/codec-objects/" <> name) [ "ar", "x", codecs <> "/" <> name <> ".a" ]
    objects = foldl extract shim codecNames
    renameArgs = A.concatMap (\sym -> [ "--redefine-sym", sym <> "=libopus_" <> sym ]) oggSymbols
    -- GNU objcopy applies the transformation to every member of an archive.
    renamed = objects
      # G.exec ([ prefix <> "objcopy" ] <> renameArgs <> [ codecs <> "/libopus.a" ])
      # G.exec ([ prefix <> "objcopy" ] <> renameArgs <> [ codecs <> "/codec-objects/opus/opus.o" ])
  pure renamed

headless :: String -> Container -> Aff Container
headless arch container = do
  built <- staticLibraries false container
  pure $ built
    # G.exec [ "cargo", "build", "--release", "--features", "cpal-sink", "-p", "rockbox-cli" ]
    # G.exec [ "cargo", "build", "--release", "-p", "rockbox-server" ]
    # G.at "/src/zig" ([ "zig", "build", "-Dheadless=true", "-Doptimize=ReleaseFast" ] <> if arch == "x86_64" then [ "-Dcpu=x86_64" ] else [])

armhf :: Container -> Aff Container
armhf container = do
  built <- staticLibraries true container
  let
    libs = foldl (\ctr name -> ctr
      # G.exec [ "cp", "-L", "/usr/lib/arm-linux-gnueabihf/lib" <> name <> ".so", "/src/build-armhf/syslibs/lib" <> name <> ".so" ]
      # G.exec [ "arm-linux-gnueabihf-objcopy", "--remove-section=.ARM.attributes", "/src/build-armhf/syslibs/lib" <> name <> ".so" ])
      (G.exec [ "mkdir", "-p", "/src/build-armhf/syslibs" ] built) [ "dbus-1", "asound", "unwind" ]
  pure $ libs
    # C.withoutEnvVariable "RUSTFLAGS"
    # C.withEnvVariable "CARGO_TARGET_ARM_UNKNOWN_LINUX_GNUEABIHF_LINKER" "arm-linux-gnueabihf-gcc"
    # C.withEnvVariable "CC_arm_unknown_linux_gnueabihf" "arm-linux-gnueabihf-gcc"
    # C.withEnvVariable "CXX_arm_unknown_linux_gnueabihf" "arm-linux-gnueabihf-g++"
    # G.exec [ "cargo", "build", "--release", "--target", "arm-unknown-linux-gnueabihf", "--features", "fts5,alsa-sink", "-p", "rockbox-cli" ]
    # G.exec [ "cargo", "build", "--release", "--target", "arm-unknown-linux-gnueabihf", "--features", "fts5", "-p", "rockbox-server" ]
    # G.at "/src/zig" [ "zig", "build", "-Dtarget=arm-linux-gnueabihf", "-Dcpu=arm1176jzf_s", "-Dheadless=true", "-Dfw-dir=../build-armhf", "-Drust-triple=arm-unknown-linux-gnueabihf", "-Dsyslibs-dir=../build-armhf/syslibs", "-Doptimize=ReleaseFast" ]
    # G.exec [ "arm-linux-gnueabihf-readelf", "-A", "/src/zig/zig-out/bin/rockboxd" ]

embedded :: String -> Container -> Aff Container
embedded arch container = do
  let
    built = container
      # G.exec [ "cargo", "build", "--release", "--locked", "-p", "rockbox-embed" ]
      # G.at "/src/zig" ([ "zig", "build", "lib", "-Doptimize=ReleaseFast" ] <> if arch == "x86_64" then [ "-Dcpu=x86_64" ] else [])
      -- Reuse the binary archive parser, not the shell orchestration around it.
      # G.exec [ "python3", "/src/scripts/flatten-archive.py", "/src/zig/zig-out/lib/librockboxd.a", "/tmp/flat-objects" ]
  objects <- D.glob "**/*.o" (C.directory "/tmp/flat-objects" built)
  G.ensure (not (A.null objects)) "No embedded archive objects extracted"
  let packed = built # G.exec ([ "ar", "rcs", "/tmp/librockboxd.a" ] <> map ("/tmp/flat-objects/" <> _) objects)
      flat = packed # C.withFile "/src/zig/zig-out/lib/librockboxd.a" (C.file "/tmp/librockboxd.a" packed)
  symbols <- C.stdout (G.exec [ "nm", "--defined-only", "/src/zig/zig-out/lib/librockboxd.a" ] flat)
  G.ensure (S.contains (S.Pattern "rb_daemon_start") symbols) "rb_daemon_start missing from embedded archive"
  pure flat
