//! Adaptive-streaming demo: play a public **HLS** (`.m3u8`) or **MPEG-DASH**
//! (`.mpd`) URL.
//!
//! The engine downloads the manifest, picks the best audio rendition, and
//! feeds the segments (demuxing MPEG-TS / fragmented-MP4 containers down to
//! a raw AAC/MP3 bitstream) to the Rockbox codecs — so a video test stream
//! plays fine: only its audio is fetched/decoded where the manifest offers a
//! separate audio rendition, or demuxed out of the muxed segments otherwise.
//!
//! ```sh
//! # a well-known public HLS test stream (default)
//! cargo run --release --example stream
//! cargo run --release --example stream -- hls
//!
//! # a well-known public MPEG-DASH test stream
//! cargo run --release --example stream -- dash
//!
//! # any manifest URL
//! cargo run --release --example stream -- https://example.com/live/master.m3u8
//! cargo run --release --example stream -- https://example.com/vod/manifest.mpd
//! ```
//!
//! VOD presentations show a duration and finish normally; live ones play
//! until Ctrl-C.

use std::time::Duration;

use rockbox_playback::{PlaybackState, Player};

/// Mux's public HLS test stream (Big Buck Bunny, MPEG-TS segments — the
/// audio is demuxed out of the muxed TS).
const HLS_DEFAULT: &str = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8";

/// Akamai's public MPEG-DASH test stream (Big Buck Bunny, fragmented-MP4
/// SegmentTemplate — only the audio adaptation set is fetched).
const DASH_DEFAULT: &str = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = match std::env::args().nth(1).as_deref() {
        None | Some("hls") => HLS_DEFAULT.to_string(),
        Some("dash") => DASH_DEFAULT.to_string(),
        Some(arg) if arg.starts_with("http://") || arg.starts_with("https://") => arg.to_string(),
        Some(other) => {
            eprintln!("unrecognized argument: {other}");
            eprintln!("usage: stream [hls|dash|<manifest URL>]");
            std::process::exit(2);
        }
    };

    let player = Player::new()?;
    eprintln!("output: {} Hz", player.sample_rate());
    eprintln!("streaming: {url}");

    player.set_queue(vec![url]);
    player.play();

    // Opening a manifest means several round-trips (manifest → playlist →
    // first segments) before any audio decodes, so allow a generous start
    // window before treating Stopped as failure.
    let mut last_line = String::new();
    let mut started = false;
    let start_deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let st = player.status();

        if st.state != PlaybackState::Stopped {
            started = true;
        }
        if !started {
            if std::time::Instant::now() > start_deadline {
                eprintln!("\nfailed to start playback (could not open the stream)");
                std::process::exit(1);
            }
            eprint!("\rbuffering…   ");
            use std::io::Write;
            std::io::stderr().flush().ok();
            continue;
        }
        if st.state == PlaybackState::Stopped {
            break; // VOD finished (a live stream plays until Ctrl-C)
        }

        let m = st.metadata.as_ref();
        // The codec label carries the protocol, e.g. "HLS AAC" / "DASH AAC".
        let codec = m.map(|m| m.codec.clone()).unwrap_or_default();
        let samplerate = m
            .map(|m| m.sample_rate)
            .filter(|r| *r > 0)
            .map(|r| format!(" {:.1}kHz", r as f32 / 1000.0))
            .unwrap_or_default();

        let pos = st.position.as_secs();
        let dur = st.duration.as_secs();
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

        let line = format!("{codec}{samplerate}  {clock}   ");
        if line != last_line {
            eprint!("\r{line}");
            use std::io::Write;
            std::io::stderr().flush().ok();
            last_line = line;
        }
    }
    eprintln!("\ndone");
    Ok(())
}
