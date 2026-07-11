const std = @import("std");

// Although this function looks imperative, it does not perform the build
// directly and instead it mutates the build graph (`b`) that will be then
// executed by an external runner. The functions in `std.Build` implement a DSL
// for defining build steps and express dependencies between them, allowing the
// build runner to parallelize the build automatically (and the cache system to
// know when a step doesn't need to be re-run).
// Codec names used by both the headless executable and the embeddable lib.
const codec_names = [_][]const u8{
    "a52",        "a52_rm",    "aac",     "aac_bsf",
    "adx",        "aiff",      "alac",    "ape",
    "atrac3_oma", "atrac3_rm", "au",      "cook",
    "flac",       "mod",       "mpa",     "mpc",
    "opus",       "raac",      "shorten", "smaf",
    "speex",      "tta",       "vorbis",  "vox",
    "wav",        "wav64",     "wavpack", "wma",
    "wmapro",
};
const lib_names = [_][]const u8{
    "liba52",        "libalac",   "libasap",  "libasf",
    "libatrac",      "libcook",   "libdemac", "libfaad",
    "libffmpegFLAC", "libm4a",    "libmad",   "libmusepack",
    "libopus",       "libpcm",    "librm",    "libspc",
    "libspeex",      "libtremor", "libtta",   "libwavpack",
    "libwma",        "libwmapro",
};

pub fn build(b: *std.Build) void {
    // Standard target options allow the person running `zig build` to choose
    // what target to build for. Here we do not override the defaults, which
    // means any target is allowed, and the default is native. Other options
    // for restricting supported target set are available.
    const target = b.standardTargetOptions(.{});
    // Standard optimization options allow the person running `zig build` to select
    // between Debug, ReleaseSafe, ReleaseFast, and ReleaseSmall. Here we do not
    // set a preferred release mode, allowing the user to decide how to optimize.
    const optimize = b.standardOptimizeOption(.{});

    // -Dheadless=true: link against build-headless/ (no SDL, cpal PCM sink,
    // statically-linked codecs). Requires:
    //   cd build-headless && make lib
    //   cargo build --release --features cpal-sink -p rockbox-cli
    const headless = b.option(bool, "headless", "Build headless target (no SDL, cpal PCM)") orelse false;
    // -Dfw-dir=../build-armhf: override the firmware build directory.
    // Defaults to build-headless (headless=true) or build-lib (headless=false).
    const fw_dir_opt = b.option([]const u8, "fw-dir", "Firmware build directory override (e.g. ../build-armhf)") orelse "";
    // -Drust-triple=arm-unknown-linux-gnueabihf: Rust target triple used to
    // locate cross-compiled Rust libraries under target/<triple>/release/.
    // Defaults to target/release/ (native build).
    const rust_triple_opt = b.option([]const u8, "rust-triple", "Rust target triple for cross-compiled lib paths (e.g. arm-unknown-linux-gnueabihf)") orelse "";
    // -Dsyslibs-dir=../build-armhf/syslibs: directory containing ARM sysroot
    // .so/.a files (dbus-1, asound, unwind) extracted from the cross toolchain.
    // Required when Zig cross-links for a target whose system libs aren't in
    // the host's default library search paths.
    const syslibs_dir = b.option([]const u8, "syslibs-dir", "Directory with cross-target system .so/.a files (e.g. ../build-armhf/syslibs)") orelse "";

    const fw_dir = if (fw_dir_opt.len > 0) fw_dir_opt
                   else if (headless) "../build-headless"
                   else "../build-lib";
    const rust_lib_dir = if (rust_triple_opt.len > 0)
        b.fmt("../target/{s}/release", .{rust_triple_opt})
    else
        "../target/release";

    // It's also possible to define more custom flags to toggle optional features
    // of this build script using `b.option()`. All defined flags (including
    // target and optimize options) will be listed when running `zig build --help`
    // in this directory.

    // This creates a module, which represents a collection of source files alongside
    // some compilation options, such as optimization mode and linked system libraries.
    // Zig modules are the preferred way of making Zig code available to consumers.
    // addModule defines a module that we intend to make available for importing
    // to our consumers. We must give it a name because a Zig package can expose
    // multiple modules and consumers will need to be able to specify which
    // module they want to access.
    const mod = b.addModule("rockboxd", .{
        // The root source file is the "entry point" of this module. Users of
        // this module will only be able to access public declarations contained
        // in this file, which means that if you have declarations that you
        // intend to expose to consumers that were defined in other files part
        // of this module, you will have to make sure to re-export them from
        // the root file.
        .root_source_file = b.path("src/root.zig"),
        // Later on we'll use this module as the root module of a test executable
        // which requires us to specify a target.
        .target = target,
    });

    // Here we define an executable. An executable needs to have a root module
    // which needs to expose a `main` function. While we could add a main function
    // to the module defined above, it's sometimes preferable to split business
    // business logic and the CLI into two separate modules.
    //
    // If your goal is to create a Zig library for others to use, consider if
    // it might benefit from also exposing a CLI tool. A parser library for a
    // data serialization format could also bundle a CLI syntax checker, for example.
    //
    // If instead your goal is to create an executable, consider if users might
    // be interested in also being able to embed the core functionality of your
    // program in their own executable in order to avoid the overhead involved in
    // subprocessing your CLI tool.
    //
    // If neither case applies to you, feel free to delete the declaration you
    // don't need and to put everything under a single module.
    const exe = b.addExecutable(.{
        .name = "rockboxd",
        .root_module = b.createModule(.{
            // b.createModule defines a new module just like b.addModule but,
            // unlike b.addModule, it does not expose the module to consumers of
            // this package, which is why in this case we don't have to give it a name.
            .root_source_file = b.path("src/main.zig"),
            // Target and optimization levels must be explicitly wired in when
            // defining an executable or library (in the root module), and you
            // can also hardcode a specific target for an executable or library
            // definition if desireable (e.g. firmware for embedded devices).
            .target = target,
            .optimize = optimize,
            // List of modules available for import in source files part of the
            // root module.
            .imports = &.{
                // Here "zig_build" is the name you will use in your source code to
                // import this module (e.g. `@import("zig_build")`). The name is
                // repeated because you are allowed to rename your imports, which
                // can be extremely useful in case of collisions (which can happen
                // importing modules from different packages).
                .{ .name = "rockboxd", .module = mod },
            },
        }),
    });

    exe.root_module.addLibraryPath(.{
        .cwd_relative = rust_lib_dir,
    });
    if (syslibs_dir.len > 0) {
        exe.root_module.addLibraryPath(.{ .cwd_relative = syslibs_dir });
    }

    if (target.result.os.tag == .macos) {
        // Homebrew path differs by architecture: /opt/homebrew on aarch64, /usr/local on x86_64
        if (target.result.cpu.arch == .aarch64) {
            exe.root_module.addLibraryPath(.{ .cwd_relative = "/opt/homebrew/lib" });
        } else {
            exe.root_module.addLibraryPath(.{ .cwd_relative = "/usr/local/lib" });
        }
        exe.root_module.linkFramework("CoreFoundation", .{});
        exe.root_module.linkFramework("Security", .{});
        // FSEvents (notify crate, used by the library filesystem watcher and
        // the S3 server's PUT/DELETE → DB sync path) lives in CoreServices.
        exe.root_module.linkFramework("CoreServices", .{});
        if (headless) {
            // Required by cpal's CoreAudio backend.
            exe.root_module.linkFramework("CoreAudio", .{});
            exe.root_module.linkFramework("AudioUnit", .{});
            exe.root_module.linkFramework("AudioToolbox", .{});
        }
    }

    if (target.result.os.tag == .linux) {
        exe.root_module.linkSystemLibrary("unwind", .{});
        exe.root_module.linkSystemLibrary("dbus-1", .{});
        if (headless) {
            // cpal uses ALSA on Linux by default.
            exe.root_module.linkSystemLibrary("asound", .{});
        }
    }

    if (target.result.os.tag == .freebsd or target.result.os.tag == .netbsd) {
        if (headless) {
            // The BSDs use the direct libasound sink (pcm-alsa.c +
            // rockbox-alsa-sink), not cpal — cpal's ALSA backend is
            // Linux-only. alsa-lib comes from the audio/alsa-lib port.
            exe.root_module.linkSystemLibrary("asound", .{});
        }
    }

    if (target.result.os.tag == .openbsd) {
        if (headless) {
            // OpenBSD uses the direct libsndio sink (pcm-sndio.c +
            // rockbox-sndio-sink); libsndio ships in the base system.
            exe.root_module.linkSystemLibrary("sndio", .{});
        }
    }

    const librockbox = b.path(b.pathJoin(&.{ fw_dir, "librockbox.a" }));
    const libfirmware = b.path(b.pathJoin(&.{ fw_dir, "firmware/libfirmware.a" }));
    const libfixedpoint = b.path(b.pathJoin(&.{ fw_dir, "lib/libfixedpoint.a" }));
    const librbcodec = b.path(b.pathJoin(&.{ fw_dir, "lib/librbcodec.a" }));
    const libskin_parser = b.path(b.pathJoin(&.{ fw_dir, "lib/libskin_parser.a" }));
    const libtlsf = b.path(b.pathJoin(&.{ fw_dir, "lib/libtlsf.a" }));
    const libutf8proc = b.path(b.pathJoin(&.{ fw_dir, "lib/libutf8proc.a" }));
    const librockbox_cli = b.path(b.pathJoin(&.{ rust_lib_dir, "librockbox_cli.a" }));
    const librockbox_server = b.path(b.pathJoin(&.{ rust_lib_dir, "librockbox_server.a" }));

    exe.root_module.addObjectFile(librockbox);
    exe.root_module.addObjectFile(libfirmware);
    exe.root_module.addObjectFile(libfixedpoint);
    exe.root_module.addObjectFile(libskin_parser);
    exe.root_module.addObjectFile(librbcodec);
    exe.root_module.addObjectFile(libtlsf);
    exe.root_module.addObjectFile(libutf8proc);
    // libspeex-voice is only needed for SDL (voice/TTS UI); the headless build
    // uses libspeex.a via lib_names instead — linking both causes duplicate symbols.
    if (!headless) {
        const libspeex_voice = b.path(b.pathJoin(&.{ fw_dir, "lib/rbcodec/codecs/libspeex-voice.a" }));
        exe.root_module.addObjectFile(libspeex_voice);
    }
    exe.root_module.addObjectFile(librockbox_cli);
    exe.root_module.addObjectFile(librockbox_server);

    if (headless) {
        // Statically-linked codecs.  Each codec's per-codec .a contains only
        // <name>.o and <name>-crt0.o (renamed by objcopy --redefine-sym so
        // __header, codec_start, codec_run, codec_main are distinct).
        //
        // We link the extracted .o files directly (not via archive) because
        // Zig's MachO linker does not drive archive scanning from data-section
        // relocations; lc_static_table's pointer entries (-> __header_NAME)
        // would not pull in the codec archive members.
        //
        // scripts/build-headless.sh Step 2.5 extracts these files into
        //   <fw_dir>/lib/rbcodec/codecs/codec-objects/<name>/
        const codec_dir = b.pathJoin(&.{ fw_dir, "lib/rbcodec/codecs" });
        const obj_base = b.pathJoin(&.{ codec_dir, "codec-objects" });
        for (codec_names) |name| {
            const dir = b.pathJoin(&.{ obj_base, name });
            exe.root_module.addObjectFile(b.path(b.pathJoin(&.{ dir, b.fmt("{s}.o", .{name}) })));
            exe.root_module.addObjectFile(b.path(b.pathJoin(&.{ dir, b.fmt("{s}-crt0.o", .{name}) })));
        }

        // libcodec.a provides codec_init, codec_malloc, bs_clz_tab, ff_copy_bits, etc.
        // In CODECS_STATIC mode Make intentionally omits it from the per-codec archives,
        // so we must link it explicitly here.
        exe.root_module.addObjectFile(b.path(b.pathJoin(&.{ codec_dir, "libcodec.a" })));

        // Support libraries referenced by the codec objects above.
        // Passed as archives (lazy scanning) because code references from the
        // directly-linked codec .o files drive archive member inclusion correctly.
        for (lib_names) |lib| {
            exe.root_module.addObjectFile(b.path(b.pathJoin(&.{ codec_dir, b.fmt("{s}.a", .{lib}) })));
        }
        // libopus and libtremor both bundle Ogg framing but with incompatible ABIs
        // (different ogg_stream_state layout, different ogg_stream_pagein signature).
        // build-headless.sh Step 2.6 renames all ogg_* symbols in libopus.a and
        // opus.o to libopus_ogg_* so each codec gets its own implementation.
    } else {
        exe.root_module.linkSystemLibrary("SDL2", .{});
    }

    exe.root_module.link_libc = true;

    // This declares intent for the executable to be installed into the
    // install prefix when running `zig build` (i.e. when executing the default
    // step). By default the install prefix is `zig-out/` but can be overridden
    // by passing `--prefix` or `-p`.
    b.installArtifact(exe);

    // This creates a top level step. Top level steps have a name and can be
    // invoked by name when running `zig build` (e.g. `zig build run`).
    // This will evaluate the `run` step rather than the default step.
    // For a top level step to actually do something, it must depend on other
    // steps (e.g. a Run step, as we will see in a moment).
    const run_step = b.step("run", "Run the app");

    // This creates a RunArtifact step in the build graph. A RunArtifact step
    // invokes an executable compiled by Zig. Steps will only be executed by the
    // runner if invoked directly by the user (in the case of top level steps)
    // or if another step depends on it, so it's up to you to define when and
    // how this Run step will be executed. In our case we want to run it when
    // the user runs `zig build run`, so we create a dependency link.
    const run_cmd = b.addRunArtifact(exe);
    run_step.dependOn(&run_cmd.step);

    // By making the run step depend on the default step, it will be run from the
    // installation directory rather than directly from within the cache directory.
    run_cmd.step.dependOn(b.getInstallStep());

    // This allows the user to pass arguments to the application in the build
    // command itself, like this: `zig build run -- arg1 arg2 etc`
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    // Creates an executable that will run `test` blocks from the provided module.
    // Here `mod` needs to define a target, which is why earlier we made sure to
    // set the releative field.
    const mod_tests = b.addTest(.{
        .root_module = mod,
    });

    // A run step that will run the test executable.
    const run_mod_tests = b.addRunArtifact(mod_tests);

    // Creates an executable that will run `test` blocks from the executable's
    // root module. Note that test executables only test one module at a time,
    // hence why we have to create two separate ones.
    const exe_tests = b.addTest(.{
        .root_module = exe.root_module,
    });

    // A run step that will run the second test executable.
    const run_exe_tests = b.addRunArtifact(exe_tests);

    // A top level step for running all tests. dependOn can be called multiple
    // times and since the two run steps do not depend on one another, this will
    // make the two of them run in parallel.
    const test_step = b.step("test", "Run tests");
    test_step.dependOn(&run_mod_tests.step);
    test_step.dependOn(&run_exe_tests.step);

    // Just like flags, top level steps are also listed in the `--help` menu.
    //
    // The Zig build system is entirely implemented in userland, which means
    // that it cannot hook into private compiler APIs. All compilation work
    // orchestrated by the build system will result in other Zig compiler
    // subcommands being invoked with the right flags defined. You can observe
    // these invocations when one fails (or you pass a flag to increase
    // verbosity) to validate assumptions and diagnose problems.
    //
    // Lastly, the Zig build system is relatively simple and self-contained,
    // and reading its source code will allow you to master it.

    // ── Embeddable static library (always headless/cpal) ──────────────────────
    // Build with:  zig build lib
    // Output:      zig-out/lib/librockboxd.a
    //
    // Prerequisites (same as the headless binary):
    //   cd build-headless && make lib
    //   cargo build --release -p rockbox-embed -p rockbox-server
    //
    // Consumers link: librockboxd.a + system audio libs
    //   macOS: -framework CoreAudio -framework AudioUnit -framework AudioToolbox
    //          -framework CoreFoundation -framework Security -framework CoreServices
    //   Linux: -lasound -lunwind -ldbus-1
    {
        const hw = "../build-headless";

        const embed_lib = b.addLibrary(.{
            .name = "rockboxd",
            .linkage = .static,
            .root_module = b.createModule(.{
                .root_source_file = b.path("src/lib.zig"),
                .target = target,
                .optimize = optimize,
                .imports = &.{
                    .{ .name = "rockboxd", .module = mod },
                },
            }),
        });

        embed_lib.root_module.addLibraryPath(.{ .cwd_relative = "../target/release" });

        if (target.result.os.tag == .macos) {
            if (target.result.cpu.arch == .aarch64) {
                embed_lib.root_module.addLibraryPath(.{ .cwd_relative = "/opt/homebrew/lib" });
            } else {
                embed_lib.root_module.addLibraryPath(.{ .cwd_relative = "/usr/local/lib" });
            }
            embed_lib.root_module.linkFramework("CoreFoundation", .{});
            embed_lib.root_module.linkFramework("Security", .{});
            embed_lib.root_module.linkFramework("CoreServices", .{});
            embed_lib.root_module.linkFramework("CoreAudio", .{});
            embed_lib.root_module.linkFramework("AudioUnit", .{});
            embed_lib.root_module.linkFramework("AudioToolbox", .{});
        }

        if (target.result.os.tag == .linux) {
            embed_lib.root_module.linkSystemLibrary("unwind", .{});
            embed_lib.root_module.linkSystemLibrary("dbus-1", .{});
            embed_lib.root_module.linkSystemLibrary("asound", .{});
        }

        if (target.result.os.tag == .freebsd or target.result.os.tag == .openbsd) {
            embed_lib.root_module.linkSystemLibrary("asound", .{});
        }

        // Firmware archives (headless build)
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ hw, "librockbox.a" })));
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ hw, "firmware/libfirmware.a" })));
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ hw, "lib/libfixedpoint.a" })));
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ hw, "lib/libskin_parser.a" })));
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ hw, "lib/librbcodec.a" })));
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ hw, "lib/libtlsf.a" })));
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ hw, "lib/libutf8proc.a" })));

        // Rust libraries
        embed_lib.root_module.addObjectFile(b.path("../target/release/librockbox_embed.a"));
        embed_lib.root_module.addObjectFile(b.path("../target/release/librockbox_server.a"));

        // Statically-linked codecs (same extraction as the headless executable)
        const ecodec_dir = b.pathJoin(&.{ hw, "lib/rbcodec/codecs" });
        const eobj_base = b.pathJoin(&.{ ecodec_dir, "codec-objects" });
        for (codec_names) |name| {
            const dir = b.pathJoin(&.{ eobj_base, name });
            embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ dir, b.fmt("{s}.o", .{name}) })));
            embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ dir, b.fmt("{s}-crt0.o", .{name}) })));
        }
        embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ ecodec_dir, "libcodec.a" })));
        for (lib_names) |lib| {
            embed_lib.root_module.addObjectFile(b.path(b.pathJoin(&.{ ecodec_dir, b.fmt("{s}.a", .{lib}) })));
        }

        embed_lib.root_module.link_libc = true;

        // Use addInstallArtifact (not installArtifact) so the embed library is
        // NOT added to the default install step. Plain `zig build` must only
        // build the rockboxd executable; `zig build lib` is the explicit entry
        // point. This prevents build-headless.sh from trying to link
        // librockbox_embed.a which is only present when rockbox-embed is built.
        const install_embed = b.addInstallArtifact(embed_lib, .{});

        const lib_step = b.step("lib", "Build the embeddable static library (zig-out/lib/librockboxd.a)");
        if (target.result.os.tag == .macos) {
            // Zig's llvm-ar emits a GNU-format archive; Apple's ld rejects it
            // with "archive member invalid control bits". Repack in-place with
            // libtool -static to produce a BSD-format archive macOS ld accepts.
            const lib_out = b.getInstallPath(.lib, "librockboxd.a");
            const repack = b.addSystemCommand(&.{ "libtool", "-static", "-o", lib_out, lib_out });
            repack.step.dependOn(&install_embed.step);
            lib_step.dependOn(&repack.step);
        } else if (target.result.os.tag == .linux) {
            // Zig packs each input archive as a member of librockboxd.a, so
            // GNU/Linux linkers (including rust-lld) see nested .a / .so
            // members and don't unpack them — symbols like rb_daemon_start
            // end up undefined. Flatten with our helper, mirroring the macOS
            // libtool repack above.
            const lib_out = b.getInstallPath(.lib, "librockboxd.a");
            const flatten = b.addSystemCommand(&.{ "bash", "../scripts/flatten-archive.sh", lib_out });
            flatten.step.dependOn(&install_embed.step);
            lib_step.dependOn(&flatten.step);
        } else {
            lib_step.dependOn(&install_embed.step);
        }
    }
}
