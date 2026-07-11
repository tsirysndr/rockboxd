//! Queue player CLI with crossfade + ReplayGain.
//!
//! Plays local files **and remote `http(s)://` URLs** — both finite files
//! (buffered on demand via range requests) and unbounded live streams
//! (internet radio). Plays every codec the build supports, including
//! **HE-AAC / AAC+** (`.m4a` with SBR) via `CODEC_AAC_SBR_DEC`.
//!
//! ```sh
//! # local files, gapless
//! cargo run --release --example play -- a.flac b.mp3 c.opus
//!
//! # a remote file (starts as soon as the header is buffered)
//! cargo run --release --example play -- https://example.com/song.flac
//!
//! # internet radio (unbounded live stream — Ctrl-C to stop)
//! cargo run --release --example play -- http://ice1.somafm.com/groovesalad-128-mp3
//!
//! # mix local + remote, with 2 s crossfade + track ReplayGain
//! cargo run --release --example play -- --crossfade 2 --replaygain track \
//!     a.flac https://example.com/b.mp3
//! ```

use std::time::Duration;

use rockbox_playback::{
    is_url, CrossfadeMode, CrossfadeSettings, PlaybackState, Player, ReplayGainMode,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut crossfade_secs = 0u64;
    let mut replaygain = ReplayGainMode::Off;
    let mut volume = 1.0f32;
    // Tracks are kept as strings so `http(s)://` URLs pass through unchanged
    // alongside local file paths — the engine dispatches on the string.
    let mut tracks: Vec<String> = Vec::new();

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
            "--volume" | "-v" => {
                volume = args.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
            }
            _ => tracks.push(arg),
        }
    }

    if tracks.is_empty() {
        eprintln!(
            "usage: play [--volume 0..1] [--crossfade SECS] [--replaygain track|album] \
             <files-or-URLs…>"
        );
        std::process::exit(2);
    }
    let has_stream = tracks.iter().any(|t| is_url(t));

    let player = Player::new()?;
    println!("output: {} Hz", player.sample_rate());

    // NOTE: volume 0.0 pauses ring consumption (a click-free mute), so playback
    // looks frozen. Use a non-zero volume to actually hear/advance.
    player.set_volume(volume);
    println!("volume: {volume}");

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

    for t in &tracks {
        println!("{}: {t}", if is_url(t) { "url" } else { "file" });
    }
    if has_stream {
        println!("(live streams play until Ctrl-C)");
    }

    player.set_queue(tracks.clone());
    player.play();

    // Poll status and print a one-line now-playing / progress display until
    // the queue finishes. A remote URL can take a moment to connect (the
    // engine is still `Stopped` while it probes/buffers), so we wait for
    // playback to actually start before treating `Stopped` as "finished".
    let mut last_line = String::new();
    let mut started = false;
    let start_deadline = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let st = player.status();

        if st.state != PlaybackState::Stopped {
            started = true;
        }
        if !started {
            // Still connecting/buffering. Give it up to 20 s, then give up.
            if std::time::Instant::now() > start_deadline {
                eprintln!("\nfailed to start playback (could not open the source)");
                std::process::exit(1);
            }
            print!("\rconnecting…   ");
            use std::io::Write;
            std::io::stdout().flush().ok();
            continue;
        }
        // Once started, a return to Stopped means the queue finished.
        if st.state == PlaybackState::Stopped {
            break;
        }

        let m = st.metadata.as_ref();
        // Now-playing: prefer "Artist — Title", else title, else codec label.
        let title = m
            .map(|m| {
                if !m.title.is_empty() && !m.artist.is_empty() {
                    format!("{} — {}", m.artist, m.title)
                } else if !m.title.is_empty() {
                    m.title.clone()
                } else {
                    m.codec.clone()
                }
            })
            .unwrap_or_default();
        let codec = m.map(|m| m.codec.clone()).unwrap_or_default();
        // Station name (ICY `icy-name`) lands in `album` for live streams.
        let station = m
            .map(|m| m.album.clone())
            .filter(|s| !s.is_empty())
            .map(|s| format!("  @{s}"))
            .unwrap_or_default();
        // Bitrate (ICY `icy-br` for live, else the file's) + sample rate.
        let bitrate = m
            .map(|m| m.bitrate)
            .filter(|b| *b > 0)
            .map(|b| format!(" {b}kbps"))
            .unwrap_or_default();
        let samplerate = m
            .map(|m| m.sample_rate)
            .filter(|r| *r > 0)
            .map(|r| format!(" {:.1}kHz", r as f32 / 1000.0))
            .unwrap_or_default();

        let pos = st.position.as_secs();
        let dur = st.duration.as_secs();
        // A live stream reports duration 0 — show elapsed against "LIVE".
        let clock = if dur == 0 {
            format!("{}:{:02} / LIVE", pos / 60, pos % 60)
        } else {
            format!(
                "{}:{:02} / {}:{:02}",
                pos / 60,
                pos % 60,
                dur / 60,
                dur % 60
            )
        };
        let line = format!(
            "[{}/{}] {} ({}{}{}){}  {}   ",
            st.index.map(|i| i + 1).unwrap_or(0),
            st.queue_len,
            title,
            codec,
            bitrate,
            samplerate,
            station,
            clock,
        );
        if line != last_line {
            print!("\r{line}");
            use std::io::Write;
            std::io::stdout().flush().ok();
            last_line = line;
        }
    }
    println!("\ndone");
    Ok(())
}
