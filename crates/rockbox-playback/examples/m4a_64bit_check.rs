//! Validate MP4 handling for files that keep `moov` at EOF and/or use a
//! 64-bit extended `mdat` size — the shape produced by streaming encoders
//! (and served by plyr.fm).
//!
//! ```sh
//! cargo run --release --example m4a_64bit_check -- <file-or-url> [more...]
//! ```
//!
//! For each input it reports:
//!   - the container shape (moov position, 32/64-bit atom sizes)
//!   - the duration `rockbox_metadata` reports from the whole file
//!   - the duration the engine derives over HTTP (header prefetch + tail retry)
//!   - whether the codec actually decodes audio, and how much

use std::io::{Read, Seek, SeekFrom};
use std::time::Duration;

use rockbox_playback::source::{id3v2_len, mp4_moov_extent, MediaSource};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let inputs: Vec<String> = std::env::args().skip(1).collect();
    if inputs.is_empty() {
        eprintln!("usage: m4a_64bit_check <file-or-url> [more...]");
        std::process::exit(2);
    }

    let mut failures = 0;
    for input in &inputs {
        println!("\n=== {input} ===");
        if let Err(e) = check(input) {
            println!("  FAILED: {e}");
            failures += 1;
        }
    }

    println!("\n{} of {} input(s) failed", failures, inputs.len());
    if failures > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn check(input: &str) -> Result<(), Box<dyn std::error::Error>> {
    let is_url = input.starts_with("http://") || input.starts_with("https://");

    // Local copy so we can inspect the container and decode it.
    let local = if is_url {
        let tmp = std::env::temp_dir().join(format!(
            "m4a_check_{}",
            input.rsplit('/').next().unwrap_or("track.m4a")
        ));
        if !tmp.exists() {
            let body = reqwest::blocking::Client::builder()
                .user_agent("rockbox-playback/m4a-check")
                .build()?
                .get(input)
                .send()?
                .error_for_status()?
                .bytes()?;
            std::fs::write(&tmp, &body)?;
        }
        tmp
    } else {
        std::path::PathBuf::from(input)
    };

    describe_container(&local)?;

    // 1. Whole-file metadata parse — exercises read_mp4_atom's 64-bit path.
    let meta = rockbox_metadata::read(&local);
    match &meta {
        Ok(m) => println!(
            "  metadata (whole file): duration={:?} rate={} Hz codec={:?}",
            m.duration, m.sample_rate, m.codec
        ),
        Err(e) => println!("  metadata (whole file): FAILED — {e}"),
    }
    let file_duration = meta.as_ref().map(|m| m.duration).unwrap_or_default();
    if file_duration.is_zero() {
        return Err("metadata parse yielded no duration".into());
    }

    // 2. Decode for real — exercises libm4a's demuxer 64-bit path.
    // The sink emits interleaved stereo i16 regardless of source layout.
    let decoded = rockbox_codecs::decode_file_sync(&local)?;
    let frames = decoded.pcm.len() / 2;
    let decoded_secs = frames as f64 / decoded.sample_rate.max(1) as f64;
    println!(
        "  decoded: {} frames @ {} Hz = {:.2}s",
        frames, decoded.sample_rate, decoded_secs
    );
    if frames == 0 {
        return Err("codec produced no PCM".into());
    }

    // 3. The engine's HTTP path — header prefetch + tail retry.
    if is_url {
        match rockbox_playback::source::open_remote(input)? {
            rockbox_playback::source::Remote::File(mut src) => {
                const HEADER_BYTES: u64 = 512 * 1024;
                const TAIL_BYTES: u64 = 1024 * 1024;
                src.prefetch(HEADER_BYTES)?;
                let size = src.size();
                let mut m = rockbox_metadata::read(src.cache_path()).unwrap_or_default();
                let header_only = m.duration;
                if m.duration.is_zero() && size > HEADER_BYTES {
                    if let Some(tag_len) = id3v2_len(src.cache_path()) {
                        let want = tag_len.saturating_add(256 * 1024).min(size);
                        if want > HEADER_BYTES && src.prefetch_range(0, want).is_ok() {
                            println!("  (widened header past {tag_len}-byte ID3v2 tag)");
                            m = rockbox_metadata::read(src.cache_path()).unwrap_or_default();
                        }
                    }
                }
                if m.duration.is_zero() && size > HEADER_BYTES {
                    if let Some((start, end)) = mp4_moov_extent(&mut src) {
                        println!("  (located moov at {start}..{end}, fetching exactly it)");
                        src.prefetch_range(start, end)?;
                        m = rockbox_metadata::read(src.cache_path()).unwrap_or_default();
                    }
                }
                if m.duration.is_zero() && size > HEADER_BYTES {
                    let tail = size.saturating_sub(TAIL_BYTES).max(HEADER_BYTES);
                    println!("  (falling back to {TAIL_BYTES}-byte tail)");
                    src.prefetch_range(tail, size)?;
                    m = rockbox_metadata::read(src.cache_path()).unwrap_or_default();
                }
                println!(
                    "  over HTTP: header-only={:?} after-tail-retry={:?} (size {} bytes)",
                    header_only, m.duration, size
                );
                if m.duration.is_zero() {
                    return Err("HTTP path still yielded no duration".into());
                }
                let drift = abs_diff(m.duration, file_duration);
                if drift > Duration::from_millis(50) {
                    return Err(format!(
                        "HTTP duration {:?} disagrees with whole-file {:?}",
                        m.duration, file_duration
                    )
                    .into());
                }
            }
            _ => println!("  over HTTP: not a seekable file source"),
        }
    }

    println!("  OK");
    Ok(())
}

fn abs_diff(a: Duration, b: Duration) -> Duration {
    if a > b {
        a - b
    } else {
        b - a
    }
}

/// Walk the top-level atoms and report where `moov` sits and whether any
/// atom uses the 64-bit extended size form.
fn describe_container(path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata()?.len();
    let mut pos = 0u64;
    let mut order = Vec::new();
    let mut ext64 = false;

    while pos + 8 <= size && order.len() < 16 {
        f.seek(SeekFrom::Start(pos))?;
        let mut head = [0u8; 16];
        if f.read(&mut head[..8])? < 8 {
            break;
        }
        let short = u32::from_be_bytes(head[0..4].try_into().unwrap()) as u64;
        let typ = String::from_utf8_lossy(&head[4..8]).to_string();
        let (len, header) = if short == 1 {
            ext64 = true;
            if f.read(&mut head[8..16])? < 8 {
                break;
            }
            (u64::from_be_bytes(head[8..16].try_into().unwrap()), 16)
        } else if short == 0 {
            (size - pos, 8)
        } else {
            (short, 8)
        };
        order.push(format!("{typ}@{pos}"));
        if len < header {
            break;
        }
        pos += len;
    }

    let moov_at = order.iter().position(|a| a.starts_with("moov"));
    println!(
        "  container: {} bytes, atoms [{}], 64-bit sizes: {}, moov: {}",
        size,
        order.join(" "),
        if ext64 { "yes" } else { "no" },
        match moov_at {
            Some(0) => "first (faststart)".to_string(),
            Some(i) => format!("index {i}"),
            None => "at/after EOF scan — trailing".to_string(),
        }
    );
    Ok(())
}
