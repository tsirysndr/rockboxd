use std::path::{Path, PathBuf};

/// One Rockbox codec: the wrapper .c in lib/rbcodec/codecs/ plus the
/// shared decoder libraries it needs.
///
/// The wrapper + codec_crt0.c are compiled with per-codec `-D` renames of
/// the four colliding entry symbols (`__header`, `codec_main`,
/// `codec_run`, `codec_start`) — the same compile-time renaming scheme
/// the repo's WASM build uses (lib/rbcodec/codecs/codecs.make), which
/// needs no objcopy and works for Mach-O, ELF and WASM alike.
///
/// This is the same codec set the repo's Android cdylib build links
/// statically (lc-android.c) — the libgme chiptune family is excluded
/// there and here because its emulators share symbols across codecs.
struct Codec {
    name: &'static str,
    feature: &'static str,
    /// shared decoder libraries (keys into LIBS)
    libs: &'static [&'static str],
}

const CODECS: &[Codec] = &[
    // PCM container family (libpcm)
    Codec {
        name: "wav",
        feature: "CARGO_FEATURE_WAV",
        libs: &["libpcm"],
    },
    Codec {
        name: "aiff",
        feature: "CARGO_FEATURE_AIFF",
        libs: &["libpcm"],
    },
    Codec {
        name: "au",
        feature: "CARGO_FEATURE_AU",
        libs: &["libpcm"],
    },
    Codec {
        name: "smaf",
        feature: "CARGO_FEATURE_SMAF",
        libs: &["libpcm"],
    },
    Codec {
        name: "vox",
        feature: "CARGO_FEATURE_VOX",
        libs: &["libpcm"],
    },
    Codec {
        name: "wav64",
        feature: "CARGO_FEATURE_WAV64",
        libs: &["libpcm"],
    },
    // lossless
    Codec {
        name: "flac",
        feature: "CARGO_FEATURE_FLAC",
        libs: &["libffmpegFLAC"],
    },
    Codec {
        name: "shorten",
        feature: "CARGO_FEATURE_SHORTEN",
        libs: &["libffmpegFLAC"],
    },
    Codec {
        name: "wavpack",
        feature: "CARGO_FEATURE_WAVPACK",
        libs: &["libwavpack"],
    },
    Codec {
        name: "alac",
        feature: "CARGO_FEATURE_ALAC",
        libs: &["libalac", "libm4a"],
    },
    Codec {
        name: "ape",
        feature: "CARGO_FEATURE_APE",
        libs: &["demac"],
    },
    Codec {
        name: "tta",
        feature: "CARGO_FEATURE_TTA",
        libs: &["libtta"],
    },
    // lossy
    Codec {
        name: "mpa",
        feature: "CARGO_FEATURE_MPA",
        libs: &["libmad", "libasf"],
    },
    Codec {
        name: "vorbis",
        feature: "CARGO_FEATURE_VORBIS",
        libs: &["libtremor", "tlsf"],
    },
    Codec {
        name: "aac",
        feature: "CARGO_FEATURE_AAC",
        libs: &["libfaad", "libm4a"],
    },
    Codec {
        name: "aac_bsf",
        feature: "CARGO_FEATURE_AAC_BSF",
        libs: &["libfaad"],
    },
    Codec {
        name: "opus",
        feature: "CARGO_FEATURE_OPUS",
        libs: &["libopus", "tlsf"],
    },
    Codec {
        name: "mpc",
        feature: "CARGO_FEATURE_MPC",
        libs: &["libmusepack"],
    },
    Codec {
        name: "speex",
        feature: "CARGO_FEATURE_SPEEX",
        libs: &["libspeex"],
    },
    Codec {
        name: "wma",
        feature: "CARGO_FEATURE_WMA",
        libs: &["libwma", "libasf"],
    },
    Codec {
        name: "wmapro",
        feature: "CARGO_FEATURE_WMAPRO",
        libs: &["libwmapro", "libasf"],
    },
    Codec {
        name: "a52",
        feature: "CARGO_FEATURE_A52",
        libs: &["liba52"],
    },
    // RealMedia / Sony containers
    Codec {
        name: "cook",
        feature: "CARGO_FEATURE_COOK",
        libs: &["libcook", "librm"],
    },
    Codec {
        name: "raac",
        feature: "CARGO_FEATURE_RAAC",
        libs: &["libfaad", "librm"],
    },
    Codec {
        name: "a52_rm",
        feature: "CARGO_FEATURE_A52_RM",
        libs: &["liba52", "librm"],
    },
    Codec {
        name: "atrac3_rm",
        feature: "CARGO_FEATURE_ATRAC3_RM",
        libs: &["libatrac", "librm"],
    },
    Codec {
        name: "atrac3_oma",
        feature: "CARGO_FEATURE_ATRAC3_OMA",
        libs: &["libatrac"],
    },
    // self-contained wrappers
    Codec {
        name: "adx",
        feature: "CARGO_FEATURE_ADX",
        libs: &[],
    },
    Codec {
        name: "mod",
        feature: "CARGO_FEATURE_MOD",
        libs: &[],
    },
];

/// Shared decoder libraries: compiled ONCE each, no matter how many
/// codecs use them (duplicate objects across archives would collide).
/// (name, directory relative to lib/rbcodec/codecs/)
const LIBS: &[(&str, &str)] = &[
    ("libpcm", "libpcm"),
    ("libffmpegFLAC", "libffmpegFLAC"),
    ("libwavpack", "libwavpack"),
    ("libalac", "libalac"),
    ("libm4a", "libm4a"),
    ("demac", "demac/libdemac"),
    ("libtta", "libtta"),
    ("libmad", "libmad"),
    ("libtremor", "libtremor"),
    ("libfaad", "libfaad"),
    ("libopus", "libopus"),
    ("libmusepack", "libmusepack"),
    ("libspeex", "libspeex"),
    ("libwma", "libwma"),
    ("libwmapro", "libwmapro"),
    ("liba52", "liba52"),
    ("libcook", "libcook"),
    ("librm", "librm"),
    ("libatrac", "libatrac"),
    ("libasf", "libasf"),
];

/// libogg public API (framing.c). THREE copies exist in the tree —
/// libtremor's framing.c, libopus's ogg/framing.c and libspeex's
/// oggframing.c — same symbol names, different internals. libtremor
/// keeps the original names; the other two get per-lib prefixes so all
/// three codecs can coexist in one binary.
const OGG_SYMBOLS: &[&str] = &[
    "ogg_packet_clear",
    "ogg_page_bos",
    "ogg_page_checksum_set",
    "ogg_page_continued",
    "ogg_page_eos",
    "ogg_page_granulepos",
    "ogg_page_packets",
    "ogg_page_pageno",
    "ogg_page_serialno",
    "ogg_page_version",
    "ogg_stream_check",
    "ogg_stream_clear",
    "ogg_stream_destroy",
    "ogg_stream_eos",
    "ogg_stream_flush",
    "ogg_stream_flush_fill",
    "ogg_stream_init",
    "ogg_stream_iovecin",
    "ogg_stream_packetin",
    "ogg_stream_packetout",
    "ogg_stream_packetpeek",
    "ogg_stream_pagein",
    "ogg_stream_pageout",
    "ogg_stream_pageout_fill",
    "ogg_stream_reset",
    "ogg_stream_reset_serialno",
    "ogg_sync_buffer",
    "ogg_sync_check",
    "ogg_sync_clear",
    "ogg_sync_destroy",
    "ogg_sync_init",
    "ogg_sync_pageout",
    "ogg_sync_pageseek",
    "ogg_sync_reset",
    "ogg_sync_wrote",
];

const CODECLIB_SOURCES: &[&str] = &[
    "lib/codeclib.c",
    "lib/ffmpeg_bitstream.c",
    "lib/mdct_lookup.c",
    "lib/fft-ffmpeg.c",
    "lib/mdct.c",
];

fn base_build(shim: &Path, codecs_dir: &Path, root: &Path) -> cc::Build {
    base_build_pre(&[], shim, codecs_dir, root)
}

/// `pre` include dirs are searched BEFORE shim/ — needed when a vendor
/// lib has its own config.h that must win over the shim's (libopus).
fn base_build_pre(pre: &[PathBuf], shim: &Path, codecs_dir: &Path, root: &Path) -> cc::Build {
    let mut b = cc::Build::new();
    for inc in pre {
        b.include(inc);
    }
    b.include(shim)
        .include(codecs_dir)
        .include(codecs_dir.join("lib"))
        .include(root.join("lib/rbcodec"))
        .include(root.join("lib/rbcodec/metadata"))
        .include(root.join("lib/rbcodec/dsp"))
        .include(root.join("lib/fixedpoint"))
        .include(root.join("lib/tlsf/src"))
        .flag_if_supported("-std=gnu99")
        .flag("-include")
        .flag(shim.join("gcc_extensions.h").to_str().unwrap())
        .flag("-include")
        .flag(root.join("lib/rbcodec/platform.h").to_str().unwrap())
        .define("CODEC", None)
        .define("ROCKBOX", None) // set globally by tools/root.make in firmware builds
        .define("CODECS_STATIC", None)
        // Silence the raw stderr stream-debugging traces (AAC_TRACE) —
        // a library must not write to stderr.
        .define("RBCODEC_QUIET", None)
        .warnings(false);
    b
}

/// Per-lib build tweaks (own config headers, ogg symbol prefixes).
/// Applied to the lib build AND to every codec-wrapper build that links
/// the lib, so renames stay consistent across translation units.
fn lib_tweaks(lib: &str, codecs_dir: &Path, b: &mut cc::Build) {
    match lib {
        "libopus" => {
            b.define("HAVE_CONFIG_H", None);
            for sym in OGG_SYMBOLS {
                b.define(sym, format!("opus_{sym}").as_str());
            }
        }
        "libspeex" => {
            b.define("HAVE_CONFIG_H", None);
            b.define("SPEEX_DISABLE_ENCODER", None);
            b.flag_if_supported("-fno-strict-aliasing");
            for sym in OGG_SYMBOLS {
                b.define(sym, format!("speex_{sym}").as_str());
            }
        }
        "libfaad" | "libm4a" => {
            b.include(codecs_dir.join("libm4a"));
            // Enable HE-AAC. common.h gates SBR (High Efficiency) on
            // CODEC_AAC_SBR_DEC and PS (HE-AACv2 parametric stereo) on
            // CODEC_SIZE >= 0x80000; without these the sbr_*.c / ps_*.c sources
            // compile to nothing and HE-AAC streams fail to start. Hosted builds
            // have the RAM/CPU for it.
            b.define("CODEC_AAC_SBR_DEC", None);
            b.define("CODEC_SIZE", "0x80000");
        }
        _ => {}
    }
}

/// libopus's own config.h must shadow the shim's for its sources.
fn lib_pre_includes(lib: &str, codecs_dir: &Path) -> Vec<PathBuf> {
    match lib {
        "libopus" => vec![
            codecs_dir.join("libopus"),
            codecs_dir.join("libopus/celt"),
            codecs_dir.join("libopus/silk"),
        ],
        _ => vec![],
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let shim = manifest.join("shim");

    // In-repo vs published-vendor mode, same scheme as rockbox-metadata.
    let repo_root = manifest.join("../..");
    let in_repo = repo_root.join("lib/rbcodec/codecs/codecs.h").is_file();
    let root: PathBuf = if in_repo {
        repo_root
    } else {
        manifest.join("vendor")
    };
    let codecs_dir = root.join("lib/rbcodec/codecs");

    let enabled: Vec<&Codec> = CODECS
        .iter()
        .filter(|c| std::env::var(c.feature).is_ok())
        .collect();

    // ---- shared codeclib (compiled once) --------------------------------
    let mut lib = base_build(&shim, &codecs_dir, &root);
    for src in CODECLIB_SOURCES {
        lib.file(codecs_dir.join(src));
    }
    lib.compile("rbcodec_codeclib");

    // ---- shared decoder libraries, each compiled once --------------------
    let mut needed: Vec<&str> = enabled
        .iter()
        .flat_map(|c| c.libs.iter().copied())
        .collect();
    needed.sort_unstable();
    needed.dedup();

    for lib_name in &needed {
        if *lib_name == "tlsf" {
            let mut tlsf = base_build(&shim, &codecs_dir, &root);
            tlsf.file(root.join("lib/tlsf/src/tlsf.c"));
            tlsf.compile("rbcodec_tlsf");
            continue;
        }
        let dir = LIBS
            .iter()
            .find(|(n, _)| n == lib_name)
            .unwrap_or_else(|| panic!("unknown lib {lib_name}"))
            .1;
        let lib_dir = codecs_dir.join(dir);
        let pre = lib_pre_includes(lib_name, &codecs_dir);
        let mut b = base_build_pre(&pre, &shim, &codecs_dir, &root);
        b.include(&lib_dir);
        lib_tweaks(lib_name, &codecs_dir, &mut b);
        for src in parse_sources(&lib_dir) {
            b.file(src);
        }
        b.compile(&format!("rbcodec_{lib_name}"));
    }

    // ---- one build per codec: wrapper + crt0, with the four entry
    //      symbols renamed via -D (WASM-build scheme) ----------------------
    for codec in &enabled {
        let mut pre = Vec::new();
        for l in codec.libs {
            pre.extend(lib_pre_includes(l, &codecs_dir));
        }
        let mut b = base_build_pre(&pre, &shim, &codecs_dir, &root);
        for l in codec.libs {
            if *l != "tlsf" {
                let dir = LIBS.iter().find(|(n, _)| n == l).unwrap().1;
                b.include(codecs_dir.join(dir));
            }
            lib_tweaks(l, &codecs_dir, &mut b);
        }
        for (sym, mangled) in [
            ("__header", "___header"),
            ("codec_main", "_codec_main"),
            ("codec_run", "_codec_run"),
            ("codec_start", "_codec_start"),
        ] {
            b.define(sym, format!("{sym}_{}", codec.name).as_str());
            b.define(mangled, format!("{mangled}_{}", codec.name).as_str());
        }

        b.file(codecs_dir.join(format!("{}.c", codec.name)));
        b.file(codecs_dir.join("codec_crt0.c"));
        b.compile(&format!("rbcodec_{}", codec.name));
    }

    // ---- the codec_api shim (decode driver) ------------------------------
    let mut shim_build = base_build(&shim, &codecs_dir, &root);
    for codec in &enabled {
        shim_build.define(&format!("RBCODEC_HAVE_{}", codec.name.to_uppercase()), None);
    }
    shim_build.file(shim.join("rbcodec_shim.c"));
    shim_build.compile("rockboxcodecs");

    println!("cargo:rerun-if-changed=shim");
    println!("cargo:rerun-if-changed=vendor");
}

/// Read a decoder library's SOURCES file, resolving the firmware's
/// preprocessor gates for a hosted build: nothing is defined, so
/// `#ifdef X` blocks (CPU_ARM asm, HAVE_RECORDING, …) drop out,
/// `#ifndef X` blocks stay, and `#else` branches of CPU gates apply.
/// Only .c files are taken — .S fast paths all have C fallbacks.
fn parse_sources(lib_dir: &Path) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(lib_dir.join("SOURCES"))
        .unwrap_or_else(|e| panic!("{}/SOURCES: {e}", lib_dir.display()));

    let mut out = Vec::new();
    // Stack of "is the current branch active?"
    let mut active: Vec<bool> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix('#') {
            let rest = rest.trim_start();
            if rest.starts_with("ifndef") {
                active.push(true);
            } else if rest.starts_with("ifdef") || rest.starts_with("if") {
                // No firmware macros are defined in this build.
                active.push(false);
            } else if rest.starts_with("elif") {
                if let Some(top) = active.last_mut() {
                    *top = false;
                }
            } else if rest.starts_with("else") {
                if let Some(top) = active.last_mut() {
                    *top = !*top;
                }
            } else if rest.starts_with("endif") {
                active.pop();
            }
            continue;
        }
        if line.ends_with(".c") && active.iter().all(|&a| a) {
            out.push(lib_dir.join(line));
        }
    }
    out
}
