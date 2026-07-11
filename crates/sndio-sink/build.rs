// libsndio is OpenBSD's native audio API and ships in the base system, so it
// is available to link there and nowhere else. Emit the `sndio_backend` cfg
// (and the -lsndio link directive) only for target_os = "openbsd"; every other
// target compiles the crate down to the empty `_link_sndio_sink` stub.
fn main() {
    println!("cargo::rustc-check-cfg=cfg(sndio_backend)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os == "openbsd" {
        println!("cargo::rustc-cfg=sndio_backend");
        println!("cargo::rustc-link-lib=sndio");
    }
}
