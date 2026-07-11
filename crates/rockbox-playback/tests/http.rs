//! HTTP(S) remote-media tests. The `HttpSource` range/cache logic is checked
//! against a tiny in-process server (no audio device needed); the end-to-end
//! playback test needs a device and skips without one.
#![cfg(feature = "http")]

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rockbox_playback::{HttpSource, MediaSource};

/// A minimal HTTP/1.1 server for one body. `support_range` toggles whether it
/// honours `Range` (206) or always returns the whole body (200). Counts the
/// requests it served so tests can assert ranged vs. full fetches.
struct TestServer {
    url: String,
    requests: Arc<AtomicUsize>,
}

fn spawn_server(body: Vec<u8>, support_range: bool, content_type: &str) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/audio", addr);
    let requests = Arc::new(AtomicUsize::new(0));

    let body = Arc::new(body);
    let ct = content_type.to_string();
    let req_counter = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let body = Arc::clone(&body);
            let ct = ct.clone();
            let counter = Arc::clone(&req_counter);
            std::thread::spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut range: Option<(u64, u64)> = None;
                let mut line = String::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                    if let Some(v) = line
                        .to_ascii_lowercase()
                        .strip_prefix("range:")
                        .map(str::trim)
                        .and_then(|v| v.strip_prefix("bytes="))
                        .map(str::to_string)
                    {
                        let (s, e) = v.split_once('-').unwrap_or((v.as_str(), ""));
                        let start: u64 = s.trim().parse().unwrap_or(0);
                        let end: u64 = e
                            .trim()
                            .parse()
                            .unwrap_or(body.len() as u64 - 1)
                            .min(body.len() as u64 - 1);
                        range = Some((start, end));
                    }
                }

                let total = body.len() as u64;
                let resp = match (support_range, range) {
                    (true, Some((s, e))) => {
                        let slice = &body[s as usize..=e as usize];
                        let mut head = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Type: {ct}\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {s}-{e}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            slice.len()
                        )
                        .into_bytes();
                        head.extend_from_slice(slice);
                        head
                    }
                    _ => {
                        let mut head = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nContent-Length: {total}\r\nConnection: close\r\n\r\n"
                        )
                        .into_bytes();
                        head.extend_from_slice(&body);
                        head
                    }
                };
                let _ = stream.write_all(&resp);
                let _ = stream.flush();
            });
        }
    });

    TestServer { url, requests }
}

fn sample_bytes(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 251) as u8).collect()
}

#[test]
fn http_source_ranged_random_access() {
    let data = sample_bytes(200_000);
    let server = spawn_server(data.clone(), true, "application/octet-stream");

    let mut src = HttpSource::new(&server.url).unwrap();
    assert_eq!(src.size(), data.len() as u64);

    // Read a window in the middle.
    src.seek(SeekFrom::Start(50_000)).unwrap();
    let mut buf = vec![0u8; 4096];
    src.read_exact(&mut buf).unwrap();
    assert_eq!(buf, data[50_000..54_096]);

    // Seek backward — served from cache, no error.
    src.seek(SeekFrom::Start(10)).unwrap();
    let mut buf2 = vec![0u8; 32];
    src.read_exact(&mut buf2).unwrap();
    assert_eq!(buf2, data[10..42]);

    // Read near the end.
    src.seek(SeekFrom::End(-16)).unwrap();
    let mut tail = vec![0u8; 16];
    src.read_exact(&mut tail).unwrap();
    assert_eq!(tail, data[data.len() - 16..]);

    // The probe plus a few ranged fetches — nowhere near one-request-per-read.
    assert!(
        server.requests.load(Ordering::SeqCst) <= 6,
        "expected a handful of ranged requests, got {}",
        server.requests.load(Ordering::SeqCst)
    );
}

#[test]
fn http_source_full_fetch_matches() {
    let data = sample_bytes(70_000);
    let server = spawn_server(data.clone(), true, "audio/mpeg");
    let mut src = HttpSource::new(&server.url).unwrap();
    src.ensure_complete().unwrap();
    let mut all = Vec::new();
    src.seek(SeekFrom::Start(0)).unwrap();
    src.read_to_end(&mut all).unwrap();
    assert_eq!(all, data);
}

#[test]
fn http_source_falls_back_when_range_ignored() {
    // Server returns 200 for everything (no Range support).
    let data = sample_bytes(40_000);
    let server = spawn_server(data.clone(), false, "audio/flac");
    let mut src = HttpSource::new(&server.url).unwrap();
    assert_eq!(src.size(), data.len() as u64);
    src.seek(SeekFrom::Start(1000)).unwrap();
    let mut buf = vec![0u8; 500];
    src.read_exact(&mut buf).unwrap();
    assert_eq!(buf, data[1000..1500]);
}

// ---- end-to-end playback of an http:// URL (needs an audio device) --------

use std::time::{Duration, Instant};

use rockbox_playback::{PlaybackState, Player};

fn wav_bytes(secs: f32, freq: f32) -> Vec<u8> {
    const RATE: u32 = 44100;
    let frames = (RATE as f32 * secs) as usize;
    let mut data = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        let t = i as f32 / RATE as f32;
        let s = (10000.0 * (2.0 * std::f32::consts::PI * freq * t).sin()) as i16;
        data.extend_from_slice(&s.to_le_bytes());
        data.extend_from_slice(&s.to_le_bytes());
    }
    let n = data.len() as u32;
    let mut v = Vec::new();
    v.extend(b"RIFF");
    v.extend(&(36 + n).to_le_bytes());
    v.extend(b"WAVEfmt ");
    v.extend(&16u32.to_le_bytes());
    v.extend(&1u16.to_le_bytes());
    v.extend(&2u16.to_le_bytes());
    v.extend(&RATE.to_le_bytes());
    v.extend(&(RATE * 4).to_le_bytes());
    v.extend(&4u16.to_le_bytes());
    v.extend(&16u16.to_le_bytes());
    v.extend(b"data");
    v.extend(&n.to_le_bytes());
    v.extend(&data);
    v
}

fn wait_until<F: Fn(&Player) -> bool>(p: &Player, timeout: Duration, cond: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond(p) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    false
}

#[test]
fn plays_a_remote_http_wav() {
    // Serve a WAV with no file extension in the URL — format must be found
    // from the audio/wav Content-Type (MIME detection).
    let server = spawn_server(wav_bytes(0.8, 440.0), true, "audio/wav");

    let Ok(player) = Player::new() else {
        eprintln!("no output device — skipping");
        return;
    };
    player.set_volume(0.05);
    player.set_queue(vec![server.url.clone()]);
    player.play();

    assert!(
        wait_until(&player, Duration::from_secs(5), |p| {
            p.status().state == PlaybackState::Playing
        }),
        "remote WAV should start playing"
    );
    assert!(
        wait_until(&player, Duration::from_secs(6), |p| {
            p.status().state == PlaybackState::Stopped
        }),
        "remote WAV should play to the end and stop"
    );
}
