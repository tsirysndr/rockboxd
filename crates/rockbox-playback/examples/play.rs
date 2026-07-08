//! Queue player CLI with crossfade + ReplayGain.
//!
//! ```sh
//! # gapless
//! cargo run --release --example play -- a.flac b.mp3 c.opus
//!
//! # 2 s crossfade + track ReplayGain
//! cargo run --release --example play -- --crossfade 2 --replaygain track a.flac b.mp3
//! ```

use std::path::PathBuf;
use std::time::Duration;

use rockbox_playback::{CrossfadeMode, CrossfadeSettings, PlaybackState, Player, ReplayGainMode};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut crossfade_secs = 0u64;
    let mut replaygain = ReplayGainMode::Off;
    let mut tracks: Vec<PathBuf> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--crossfade" | "-x" => {
                crossfade_secs = args.next().and_then(|s| s.parse().ok()).unwrap_or(2);
            }
            "--replaygain" | "-r" => {
                replaygain = match args.next().as_deref() {
                    Some("track") => ReplayGainMode::Track,
                    Some("album") => ReplayGainMode::Album,
                    _ => ReplayGainMode::Off,
                };
            }
            _ => tracks.push(PathBuf::from(arg)),
        }
    }

    if tracks.is_empty() {
        eprintln!("usage: play [--crossfade SECS] [--replaygain track|album] <files…>");
        std::process::exit(2);
    }

    let player = Player::new()?;
    println!("output: {} Hz", player.sample_rate());

    if crossfade_secs > 0 {
        player.set_crossfade(CrossfadeSettings {
            mode: CrossfadeMode::Always,
            fade_in_duration: Duration::from_secs(crossfade_secs),
            fade_out_duration: Duration::from_secs(crossfade_secs),
            ..Default::default()
        });
        println!("crossfade: {crossfade_secs}s");
    }
    if replaygain != ReplayGainMode::Off {
        player.set_replaygain(replaygain, 0.0, true);
        println!("replaygain: {replaygain:?}");
    }

    player.set_queue(tracks.clone());
    player.play();

    // Poll status and print a one-line now-playing / progress display
    // until the queue finishes.
    let mut last_line = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let st = player.status();
        if st.state == PlaybackState::Stopped && st.position.is_zero() && st.index.is_none() {
            break;
        }
        let title = st
            .metadata
            .as_ref()
            .map(|m| {
                if m.title.is_empty() {
                    m.codec.clone()
                } else {
                    format!("{} — {}", m.artist, m.title)
                }
            })
            .unwrap_or_default();
        let pos = st.position.as_secs();
        let dur = st.duration.as_secs();
        let line = format!(
            "[{}/{}] {}  {}:{:02} / {}:{:02}   ",
            st.index.map(|i| i + 1).unwrap_or(0),
            st.queue_len,
            title,
            pos / 60,
            pos % 60,
            dur / 60,
            dur % 60,
        );
        if line != last_line {
            print!("\r{line}");
            use std::io::Write;
            std::io::stdout().flush().ok();
            last_line = line;
        }
        // Stop once the last track has played out.
        if st.state == PlaybackState::Stopped && st.index == Some(tracks.len() - 1) {
            break;
        }
    }
    println!("\ndone");
    Ok(())
}
