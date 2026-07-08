use std::path::PathBuf;

/// Rockbox metadata parsers — portable C, plain POSIX file I/O, no OS
/// dependencies beyond open/read/lseek/close.
const METADATA_SOURCES: &[&str] = &[
    "a52.c",
    "aac.c",
    "adx.c",
    "aiff.c",
    "ape.c",
    "asap.c",
    "asf.c",
    "au.c",
    "ay.c",
    "flac.c",
    "gbs.c",
    "hes.c",
    "id3tags.c",
    "kss.c",
    "metadata.c",
    "metadata_common.c",
    "mod.c",
    "monkeys.c",
    "mp3.c",
    "mp3data.c",
    "mp4.c",
    "mpc.c",
    "nsf.c",
    "ogg.c",
    "oma.c",
    "replaygain.c",
    "rm.c",
    "sgc.c",
    "sid.c",
    "smaf.c",
    "spc.c",
    "tta.c",
    "vgm.c",
    "vorbis.c",
    "vox.c",
    "vtx.c",
    "wave.c",
    "wavpack.c",
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..");

    // Inside the rockbox source tree, build straight from lib/rbcodec so
    // development stays live. In the published package (crates.io,
    // docs.rs) those paths don't exist — build from the vendor/ copy
    // (re-sync with ./sync-vendor.sh before publishing).
    let in_repo = root.join("lib/rbcodec/metadata/metadata.c").is_file();

    let (metadata, fixedpoint_c, platform_h, extra_includes): (
        PathBuf,
        PathBuf,
        PathBuf,
        Vec<PathBuf>,
    ) = if in_repo {
        (
            root.join("lib/rbcodec/metadata"),
            root.join("lib/fixedpoint/fixedpoint.c"),
            root.join("lib/rbcodec/platform.h"),
            vec![
                // "metadata/metadata_common.h", <codecs/librm/rm.h> and
                // <codecs/libasf/asf.h> resolve relative to lib/rbcodec.
                // Do NOT add firmware/include here — its assert.h shadows
                // the system one; gcc_extensions.h is vendored into shim/
                // instead.
                root.join("lib/rbcodec"),
                root.join("lib/fixedpoint"),
            ],
        )
    } else {
        let vendor = manifest.join("vendor");
        (
            vendor.join("metadata"),
            vendor.join("fixedpoint/fixedpoint.c"),
            vendor.join("platform.h"),
            vec![vendor.clone(), vendor.join("fixedpoint")],
        )
    };

    let mut build = cc::Build::new();
    build
        // shim/ first so its stub headers (config.h, debug.h, logf.h,
        // rbunicode.h, string-extra.h, misc.h, plugin.h, buffering.h,
        // rbcodecconfig.h, rbcodecplatform.h) shadow the firmware/apps ones.
        .include(manifest.join("shim"))
        .include(&metadata);
    for inc in &extra_includes {
        build.include(inc);
    }
    build
        .flag_if_supported("-std=gnu99")
        // Force gcc_extensions.h + platform.h (attribute macros, MIN/MAX,
        // endian helpers, …) into every TU — on firmware builds the target
        // config.h provides these before any other header.
        .flag("-include")
        .flag(manifest.join("shim/gcc_extensions.h").to_str().unwrap())
        .flag("-include")
        .flag(platform_h.to_str().unwrap())
        // Silence the raw fprintf(stderr, "[metadata] …") stream-debugging
        // trace in metadata.c (META_TRACE) — a library must not write to
        // stderr.
        .define("RBMETA_QUIET", None)
        // rockbox-dsp's shim also defines get_replaygain_int (dB×100 →
        // Q7.24). Rename our definition (metadata/replaygain.c) so both
        // crates can be linked into the same binary.
        .define("get_replaygain_int", "rbmeta_get_replaygain_int")
        .warnings(false);

    for src in METADATA_SOURCES {
        build.file(metadata.join(src));
    }
    build.file(&fixedpoint_c);
    build.file(manifest.join("shim/rbmeta_shim.c"));

    build.compile("rockboxmetadata");

    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=vendor");
    for src in METADATA_SOURCES {
        println!("cargo:rerun-if-changed={}", metadata.join(src).display());
    }
    println!("cargo:rerun-if-changed={}", fixedpoint_c.display());
}
