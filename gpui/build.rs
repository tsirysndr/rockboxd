fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Link the embeddable librockboxd.a produced by `cd zig && zig build lib`.
    // Must be built before `cargo build` in the gpui workspace.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search=native={manifest}/../zig/zig-out/lib");
    println!("cargo:rustc-link-lib=static=rockboxd");
    println!("cargo:rerun-if-changed={manifest}/../zig/zig-out/lib/librockboxd.a");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target_os.as_str() {
        "macos" => {
            println!("cargo:rustc-link-lib=framework=CoreAudio");
            println!("cargo:rustc-link-lib=framework=AudioUnit");
            println!("cargo:rustc-link-lib=framework=AudioToolbox");
            println!("cargo:rustc-link-lib=framework=CoreFoundation");
            println!("cargo:rustc-link-lib=framework=Security");
            // reqwest system-proxy (rocksky-sdk) → SCDynamicStore
            println!("cargo:rustc-link-lib=framework=SystemConfiguration");
        }
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=asound");
            println!("cargo:rustc-link-lib=dylib=unwind");
            println!("cargo:rustc-link-lib=dylib=dbus-1");
            println!("cargo:rustc-link-lib=dylib=fdk-aac");
        }
        _ => {}
    }

    tonic_build::configure()
        .out_dir("src/api")
        .file_descriptor_set_path("src/api/rockbox_descriptor.bin")
        .compile_protos(
            &[
                "proto/rockbox/v1alpha1/browse.proto",
                "proto/rockbox/v1alpha1/genre.proto",
                "proto/rockbox/v1alpha1/library.proto",
                "proto/rockbox/v1alpha1/metadata.proto",
                "proto/rockbox/v1alpha1/playback.proto",
                "proto/rockbox/v1alpha1/playlist.proto",
                "proto/rockbox/v1alpha1/settings.proto",
                "proto/rockbox/v1alpha1/sound.proto",
                "proto/rockbox/v1alpha1/system.proto",
                "proto/rockbox/v1alpha1/saved_playlist.proto",
                "proto/rockbox/v1alpha1/smart_playlist.proto",
                "proto/rockbox/v1alpha1/bluetooth.proto",
            ],
            &["proto"],
        )?;
    Ok(())
}
