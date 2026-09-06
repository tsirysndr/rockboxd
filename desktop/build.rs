use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protos = [
        "proto/rockbox/v1alpha1/playback.proto",
        "proto/rockbox/v1alpha1/playlist.proto",
        "proto/rockbox/v1alpha1/library.proto",
        "proto/rockbox/v1alpha1/sound.proto",
        "proto/rockbox/v1alpha1/system.proto",
        "proto/rockbox/v1alpha1/settings.proto",
        "proto/rockbox/v1alpha1/browse.proto",
        "proto/rockbox/v1alpha1/saved_playlist.proto",
    ];
    tonic_build::configure()
        .build_server(false)
        .build_client(true)
        .compile_protos(&protos, &["proto"])?;
    for p in &protos {
        println!("cargo:rerun-if-changed={p}");
    }

    slint_build::compile_with_config(
        "ui/app.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent-dark".into()),
    )?;

    // ── Embedded daemon (same librockboxd.a the GPUI app links) ────────────
    // Build it with: cd build-headless && make lib && cd .. &&
    //   cargo build --release -p rockbox-embed -p rockbox-server &&
    //   cd zig && zig build lib
    // If the archive is absent we still build — as a remote-only client.
    println!("cargo:rustc-check-cfg=cfg(embedded_daemon)");
    let manifest = std::env::var("CARGO_MANIFEST_DIR")?;
    let lib_dir = format!("{manifest}/../zig/zig-out/lib");
    let archive = format!("{lib_dir}/librockboxd.a");
    println!("cargo:rerun-if-changed={archive}");
    if Path::new(&archive).exists() {
        println!("cargo:rustc-cfg=embedded_daemon");
        println!("cargo:rustc-link-search=native={lib_dir}");
        println!("cargo:rustc-link-lib=static=rockboxd");

        let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        match target_os.as_str() {
            "macos" => {
                for fw in [
                    "CoreAudio",
                    "AudioUnit",
                    "AudioToolbox",
                    "CoreFoundation",
                    "Security",
                    "CoreServices",
                ] {
                    println!("cargo:rustc-link-lib=framework={fw}");
                }
            }
            "linux" => {
                println!("cargo:rustc-link-lib=dylib=asound");
                println!("cargo:rustc-link-lib=dylib=unwind");
                println!("cargo:rustc-link-lib=dylib=dbus-1");
            }
            _ => {}
        }
    } else {
        println!(
            "cargo:warning=librockboxd.a not found — building remote-only client \
             (run `zig build lib` to enable the embedded daemon)"
        );
    }
    Ok(())
}
