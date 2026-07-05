use std::path::PathBuf;

/// Rockbox DSP pipeline sources — portable fixed-point C, no OS
/// dependencies. The .S files in the upstream directory are
/// ARM32/ColdFire only; on host targets the generic C paths compile
/// instead.
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

    // Inside the rockbox source tree, build straight from lib/rbcodec so
    // development stays live. In the published package (crates.io,
    // docs.rs) those paths don't exist — build from the vendor/ copy
    // (re-sync with ./sync-vendor.sh before publishing).
    let in_repo = root.join("lib/rbcodec/dsp/dsp_core.c").is_file();

    let (dsp, fixedpoint_c, platform_h, extra_includes): (PathBuf, PathBuf, PathBuf, Vec<PathBuf>) =
        if in_repo {
            (
                root.join("lib/rbcodec/dsp"),
                root.join("lib/fixedpoint/fixedpoint.c"),
                root.join("lib/rbcodec/platform.h"),
                vec![
                    root.join("lib/rbcodec"),
                    root.join("lib/fixedpoint"),
                    // fracmul.h only (settings.h is shadowed by shim/). Do
                    // NOT add firmware/include here — its assert.h shadows
                    // the system one; gcc_extensions.h is vendored into
                    // shim/ instead.
                    root.join("apps"),
                ],
            )
        } else {
            let vendor = manifest.join("vendor");
            (
                vendor.join("dsp"),
                vendor.join("fixedpoint/fixedpoint.c"),
                vendor.join("platform.h"),
                vec![vendor.clone(), vendor.join("fixedpoint")],
            )
        };

    let mut build = cc::Build::new();
    build
        // shim/ first so its stub headers (settings.h, config.h, sound.h,
        // core_alloc.h, replaygain.h, logf.h, debug.h) shadow the
        // firmware/apps ones.
        .include(manifest.join("shim"))
        .include(&dsp);
    for inc in &extra_includes {
        build.include(inc);
    }
    build
        .flag_if_supported("-std=gnu99")
        // Force gcc_extensions.h + platform.h (attribute macros:
        // INIT_ATTR, FORCE_INLINE, UNLIKELY, MIN/MAX, …) into every TU —
        // on firmware builds the target config.h provides these before
        // any other header.
        .flag("-include")
        .flag(manifest.join("shim/gcc_extensions.h").to_str().unwrap())
        .flag("-include")
        .flag(platform_h.to_str().unwrap())
        .warnings(false);

    for src in DSP_SOURCES {
        build.file(dsp.join(src));
    }
    build.file(&fixedpoint_c);
    build.file(manifest.join("shim/rbdsp_shim.c"));

    build.compile("rockboxdsp");

    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=vendor");
    for src in DSP_SOURCES {
        println!("cargo:rerun-if-changed={}", dsp.join(src).display());
    }
    println!("cargo:rerun-if-changed={}", fixedpoint_c.display());
}
