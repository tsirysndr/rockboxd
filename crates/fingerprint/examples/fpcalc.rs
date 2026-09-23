//! `fpcalc` in Rust, for checking this crate against the real one.
//!
//! Prints the same two lines Chromaprint's own `fpcalc` prints, so the two can
//! be diffed directly:
//!
//! ```sh
//! cargo run -p rockbox-fingerprint --example fpcalc -- track.flac
//! fpcalc track.flac
//! ```
//!
//! They must match exactly. A fingerprint that differs from `fpcalc`'s is one
//! the AcoustID index cannot match, which would make the whole pass a
//! no-op that quietly stored two kilobytes per track for nothing.

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Same flag `fpcalc` uses to print the sub-fingerprints instead of the
    // compressed string, so the two can be compared item by item.
    let raw = args.iter().any(|a| a == "-raw" || a == "--raw");
    args.retain(|a| !a.starts_with('-'));

    if args.is_empty() {
        eprintln!("usage: fpcalc [-raw] <file>...");
        std::process::exit(2);
    }

    for path in args {
        let fingerprint = rockbox_fingerprint::fingerprint_file(&path)?;
        println!("FILE={path}");
        println!("DURATION={}", fingerprint.duration_secs());
        if raw {
            let items: Vec<String> = fingerprint.raw.iter().map(|v| v.to_string()).collect();
            println!("FINGERPRINT={}", items.join(","));
        } else {
            println!("FINGERPRINT={}", fingerprint.compressed);
        }
    }
    Ok(())
}
