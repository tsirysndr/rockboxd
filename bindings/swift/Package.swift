// swift-tools-version:5.9
import PackageDescription
import Foundation

// Absolute path to the prebuilt Rust static archive, resolved from the manifest
// location so `-force_load`/`-u` work regardless of the linker's cwd. Two
// locations are tried, first hit wins:
//   1. ../../target/release  — the in-repo cargo output (dev builds)
//   2. Libs/                 — a vendored copy (distributable tarball)
// Build the repo copy with `cargo build --release -p rockbox-ffi`.
let ffiArchive: String = {
    let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    let candidates = [
        root.appendingPathComponent("../../target/release/librockbox_ffi.a"),
        root.appendingPathComponent("Libs/librockbox_ffi.a"),
    ].map { $0.standardizedFileURL.path }
    return candidates.first { FileManager.default.fileExists(atPath: $0) } ?? candidates[0]
}()

// Every rb_* entry point the Swift loader resolves via dlsym. We reach them
// through dlsym (never a direct call), so the linker sees no references and
// would drop them from the final image. `-u <symbol>` forces each one in.
// They're spread across Rust codegen units, so a single -u won't transitively
// pull the rest — every symbol must be listed. Pulling by name (rather than
// -force_load'ing the whole archive) also lets the linker resolve the archive
// members on demand, so the two embedded copies of fixedpoint.o (from
// rockbox-dsp and rockbox-metadata) collapse to one instead of colliding.
//
// KEEP IN SYNC with include/rockbox_ffi.h and RockboxFFICore/Loader.swift.
let ffiSymbols = [
    "rb_ffi_abi_version", "rb_string_free", "rb_buffer_free",
    "rb_dsp_new", "rb_dsp_free", "rb_dsp_set_input_frequency", "rb_dsp_flush",
    "rb_dsp_eq_enable", "rb_dsp_set_tone", "rb_dsp_set_tone_cutoffs",
    "rb_dsp_set_surround", "rb_dsp_set_channel_config", "rb_dsp_set_stereo_width",
    "rb_dsp_set_compressor", "rb_dsp_set_replaygain", "rb_dsp_set_replaygain_gains",
    "rb_dsp_set_replaygain_gains_raw", "rb_dsp_set_eq_band", "rb_dsp_set_eq_precut",
    "rb_dsp_process",
    "rb_meta_read_json", "rb_meta_probe",
    "rb_decoder_open", "rb_decoder_free", "rb_decoder_metadata_json",
    "rb_decoder_next_chunk", "rb_decoder_seek_ms", "rb_decoder_elapsed_ms",
    "rb_decoder_finished",
    "rb_player_new", "rb_player_new_with_config", "rb_player_new_with_output",
    "rb_player_free",
    "rb_player_set_queue_json", "rb_player_enqueue", "rb_player_play",
    "rb_player_pause", "rb_player_toggle", "rb_player_stop", "rb_player_next",
    "rb_player_previous", "rb_player_skip_to", "rb_player_seek_ms",
    "rb_player_set_volume", "rb_player_set_balance", "rb_player_set_crossfade",
    "rb_player_set_replaygain",
    "rb_player_volume", "rb_player_balance", "rb_player_sample_rate",
    "rb_player_status_json",
]

// Mach-O prefixes C symbols with an underscore; ELF does not.
func forceUndefined(prefix: String) -> [String] {
    ffiSymbols.flatMap { ["-Xlinker", "-u", "-Xlinker", "\(prefix)\($0)"] }
}

// Linker settings for the statically-linked product: force the entry points
// in, hand the linker the archive as a normal input, and link the system
// libraries the engine depends on. See CLAUDE.md for the platform list.
let staticLinkerSettings: [LinkerSetting] = [
    .unsafeFlags(forceUndefined(prefix: "_") + [ffiArchive],
                 .when(platforms: [.macOS, .iOS])),
    .linkedFramework("AudioUnit", .when(platforms: [.macOS, .iOS])),
    .linkedFramework("CoreAudio", .when(platforms: [.macOS, .iOS])),
    .linkedFramework("CoreFoundation", .when(platforms: [.macOS, .iOS])),
    .linkedLibrary("iconv", .when(platforms: [.macOS, .iOS])),
    // Linux: ELF symbols carry no underscore; ALSA backs cpal.
    .unsafeFlags(forceUndefined(prefix: "") + [ffiArchive],
                 .when(platforms: [.linux])),
    .linkedLibrary("asound", .when(platforms: [.linux])),
]

let package = Package(
    name: "RockboxFFI",
    platforms: [
        .macOS(.v11),
        .iOS(.v14),
    ],
    products: [
        // Self-contained: statically links librockbox_ffi.a into the consumer.
        .library(name: "RockboxFFI", targets: ["RockboxFFI"]),
        // Runtime-loaded: dlopen's the shared library, like Ruby / Python.
        .library(name: "RockboxFFIDynamic", targets: ["RockboxFFIDynamic"]),
        .executable(name: "rockbox-ffi-smoke", targets: ["rockbox-ffi-smoke"]),
        .executable(name: "rockbox-ffi-play", targets: ["rockbox-ffi-play"]),
    ],
    targets: [
        // Shared implementation: the public API + the dlsym loader. The loader
        // resolves symbols from the process image (static) or a dlopen'd
        // library (dynamic) — decided at runtime — so both products reuse it.
        .target(name: "RockboxFFICore"),

        // Static product: re-export core + link the archive at build time.
        .target(
            name: "RockboxFFI",
            dependencies: ["RockboxFFICore"],
            linkerSettings: staticLinkerSettings
        ),

        // Dynamic product: re-export core, link nothing.
        .target(
            name: "RockboxFFIDynamic",
            dependencies: ["RockboxFFICore"]
        ),

        // Examples: smoke exercises the static product (headline: no runtime
        // library lookup), play uses the dynamic product.
        .executableTarget(
            name: "rockbox-ffi-smoke",
            dependencies: ["RockboxFFI"]
        ),
        .executableTarget(
            name: "rockbox-ffi-play",
            dependencies: ["RockboxFFIDynamic"]
        ),
    ]
)
