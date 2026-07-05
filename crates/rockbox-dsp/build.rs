use std::path::PathBuf;

/// Rockbox DSP pipeline sources (lib/rbcodec/dsp/) — portable fixed-point C,
/// no OS dependencies. The .S files in that directory are ARM32/ColdFire
/// only; on host targets the generic C paths compile instead.
const DSP_SOURCES: &[&str] = &[
    "afr.c",
    "channel_mode.c",
    "compressor.c",
    "crossfeed.c",
    "dsp_core.c",
    "dsp_filter.c",
    "dsp_misc.c",
    "dsp_sample_input.c",
    "dsp_sample_io.c",
    "dsp_sample_output.c",
    "eq.c",
    "pbe.c",
    "pga.c",
    "resample.c",
    "surround.c",
    "tdspeed.c",
    "tone_controls.c",
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../..");
    let dsp = root.join("lib/rbcodec/dsp");

    let mut build = cc::Build::new();
    build
        // Include order matters: shim/ first so its stub headers
        // (settings.h, config.h, sound.h, core_alloc.h, replaygain.h,
        // logf.h, debug.h) shadow the firmware/apps ones.
        .include(manifest.join("shim"))
        .include(&dsp)
        .include(root.join("lib/rbcodec"))
        .include(root.join("lib/fixedpoint"))
        // fracmul.h only (settings.h is shadowed by shim/). Do NOT add
        // firmware/include here — its assert.h shadows the system one;
        // gcc_extensions.h is vendored into shim/ instead.
        .include(root.join("apps"))
        .flag_if_supported("-std=gnu99")
        // Force lib/rbcodec/platform.h (attribute macros: INIT_ATTR,
        // ICODE_ATTR, MIN/MAX, …) into every TU — on firmware builds the
        // target config.h provides these before any other header.
        .flag("-include")
        .flag(manifest.join("shim/gcc_extensions.h").to_str().unwrap())
        .flag("-include")
        .flag(root.join("lib/rbcodec/platform.h").to_str().unwrap())
        .warnings(false);

    for src in DSP_SOURCES {
        build.file(dsp.join(src));
    }
    build.file(root.join("lib/fixedpoint/fixedpoint.c"));
    build.file(manifest.join("shim/rbdsp_shim.c"));

    build.compile("rockboxdsp");

    println!("cargo:rerun-if-changed=shim");
    for src in DSP_SOURCES {
        println!("cargo:rerun-if-changed={}", dsp.join(src).display());
    }
    println!(
        "cargo:rerun-if-changed={}",
        root.join("lib/fixedpoint/fixedpoint.c").display()
    );
}
