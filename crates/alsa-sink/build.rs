// Enable the real libasound implementation on the platforms that ship
// alsa-lib: Linux natively, and FreeBSD/NetBSD via the audio/alsa-lib port.
// Everywhere else (macOS, WASM, …) the crate compiles to empty stubs.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(alsa_backend)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(os.as_str(), "linux" | "freebsd" | "netbsd") {
        println!("cargo::rustc-cfg=alsa_backend");
    }
}
