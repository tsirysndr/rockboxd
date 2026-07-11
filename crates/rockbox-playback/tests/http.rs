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

/// Track bytes served, so a test can prove playback started without a full
/// download.
fn spawn_counting_server(body: Vec<u8>, content_type: &str) -> (TestServer, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/big", addr);
    let requests = Arc::new(AtomicUsize::new(0));
    let served = Arc::new(AtomicUsize::new(0));

    let body = Arc::new(body);
    let ct = content_type.to_string();
    let req_c = Arc::clone(&requests);
    let served_c = Arc::clone(&served);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let body = Arc::clone(&body);
            let ct = ct.clone();
            let rc = Arc::clone(&req_c);
            let sc = Arc::clone(&served_c);
            std::thread::spawn(move || {
                rc.fetch_add(1, Ordering::SeqCst);
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
                let (s, e) = range.unwrap_or((0, total - 1));
                let slice = &body[s as usize..=e as usize];
                sc.fetch_add(slice.len(), Ordering::SeqCst);
                let mut head = format!(
                    "HTTP/1.1 206 Partial Content\r\nContent-Type: {ct}\r\nAccept-Ranges: bytes\r\nContent-Range: bytes {s}-{e}/{total}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    slice.len()
                )
                .into_bytes();
                head.extend_from_slice(slice);
                let _ = stream.write_all(&head);
                let _ = stream.flush();
            });
        }
    });

    (TestServer { url, requests }, served)
}

#[test]
fn big_finite_file_starts_without_full_download() {
    // A large finite MP3 (~2 min ≈ 2 MB at 128 kbps): playback must start
    // after buffering only the header (via range requests), not after
    // downloading the whole file.
    let Some(mp3) = encode_mp3(120.0, 440.0) else {
        eprintln!("ffmpeg not installed — skipping");
        return;
    };
    let total = mp3.len();
    assert!(
        total > 1_500_000,
        "need a sizeable file for this test (got {total})"
    );
    let (server, served) = spawn_counting_server(mp3, "audio/mpeg");

    let Ok(player) = Player::new() else {
        eprintln!("no output device — skipping");
        return;
    };
    player.set_volume(0.05);
    player.set_queue(vec![server.url.clone()]);
    player.play();

    assert!(
        wait_until(&player, Duration::from_secs(5), |p| {
            p.status().state == PlaybackState::Playing && p.status().position > Duration::ZERO
        }),
        "big remote file should start playing quickly"
    );

    // At the moment playback starts, only the header region (plus read-ahead)
    // should have been fetched — nowhere near the whole file.
    let bytes = served.load(Ordering::SeqCst);
    assert!(
        bytes < total / 2,
        "should not have downloaded the whole file to start: served {bytes} of {total}"
    );

    player.stop();
    assert!(wait_until(&player, Duration::from_secs(3), |p| {
        p.status().state == PlaybackState::Stopped
    }));
}

// ---- ICY (SHOUTcast/Icecast) live metadata --------------------------------

/// Build one ICY metadata block: a length byte (in 16-byte units) followed by
/// `StreamTitle='…';` NUL-padded to that length.
fn icy_block(title: &str) -> Vec<u8> {
    let s = format!("StreamTitle='{title}';");
    let units = s.len().div_ceil(16);
    let mut block = Vec::with_capacity(1 + units * 16);
    block.push(units as u8);
    block.extend_from_slice(s.as_bytes());
    block.resize(1 + units * 16, 0);
    block
}

/// Interleave `audio` with an ICY metadata `block` every `metaint` audio bytes
/// (the wire format the client de-interleaves).
fn interleave_icy(audio: &[u8], metaint: usize, block: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < audio.len() {
        let end = (i + metaint).min(audio.len());
        out.extend_from_slice(&audio[i..end]);
        if end - i == metaint {
            out.extend_from_slice(block); // full interval → metadata block
        }
        i = end;
    }
    out
}

/// A live radio server with `icy-*` headers and in-band `StreamTitle`.
fn spawn_icy_server(
    audio: Vec<u8>,
    repeats: usize,
    metaint: usize,
    name: &str,
    bitrate: u32,
    title: &str,
) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/icy", addr);
    let requests = Arc::new(AtomicUsize::new(0));

    let full: Vec<u8> = std::iter::repeat(audio).take(repeats).flatten().collect();
    let body = Arc::new(interleave_icy(&full, metaint, &icy_block(title)));
    let name = name.to_string();
    let counter = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let body = Arc::clone(&body);
            let name = name.clone();
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                }
                // No Content-Length (→ live stream); icy-metaint drives the
                // de-interleaver; icy-name / icy-br are station info.
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nicy-metaint: {metaint}\r\nicy-name: {name}\r\nicy-br: {bitrate}\r\nConnection: close\r\n\r\n"
                );
                if stream.write_all(head.as_bytes()).is_err() {
                    return;
                }
                for piece in body.chunks(4096) {
                    if stream.write_all(piece).is_err() {
                        return;
                    }
                    let _ = stream.flush();
                    std::thread::sleep(Duration::from_millis(3));
                }
            });
        }
    });

    TestServer { url, requests }
}

#[test]
fn live_stream_reports_icy_metadata() {
    let Some(mp3) = encode_mp3(0.5, 440.0) else {
        eprintln!("ffmpeg not installed — skipping");
        return;
    };
    let server = spawn_icy_server(
        mp3,
        80,
        8192,
        "Test Radio",
        128,
        "Daft Punk - Around the World",
    );

    let Ok(player) = Player::new() else {
        eprintln!("no output device — skipping");
        return;
    };
    player.set_volume(0.05);
    player.set_queue(vec![server.url.clone()]);
    player.play();

    // The station name / bitrate come from the icy-* headers immediately once
    // playback starts; the StreamTitle appears after the first in-band block.
    assert!(
        wait_until(&player, Duration::from_secs(8), |p| {
            p.status()
                .metadata
                .as_ref()
                .map(|m| m.title.contains("Around the World"))
                .unwrap_or(false)
        }),
        "should surface the ICY StreamTitle in status().metadata"
    );

    let m = player.status().metadata.unwrap();
    assert_eq!(m.artist, "Daft Punk", "StreamTitle artist split");
    assert_eq!(m.title, "Around the World", "StreamTitle title split");
    assert_eq!(m.album, "Test Radio", "icy-name → album");
    assert_eq!(m.bitrate, 128, "icy-br → bitrate");
    // The decoded sample rate is filled in once decoding starts.
    assert!(
        m.sample_rate >= 8000,
        "live stream should report a decoded sample rate, got {}",
        m.sample_rate
    );

    player.stop();
    assert!(wait_until(&player, Duration::from_secs(3), |p| {
        p.status().state == PlaybackState::Stopped
    }));
}

// ---- infinite / live stream (no Content-Length) ---------------------------

/// Serve `body`, repeated `repeats` times, as a chunked HTTP response with NO
/// Content-Length and NO Range support — i.e. an unbounded live stream like
/// internet radio. A short delay between chunks mimics real-time delivery.
fn spawn_stream_server(body: Vec<u8>, repeats: usize, content_type: &str) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{}/radio", addr);
    let requests = Arc::new(AtomicUsize::new(0));

    let body = Arc::new(body);
    let ct = content_type.to_string();
    let counter = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let body = Arc::clone(&body);
            let ct = ct.clone();
            let counter = Arc::clone(&counter);
            std::thread::spawn(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                // Drain request headers.
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" || line == "\n" {
                        break;
                    }
                }
                // Chunked response, no Content-Length → unbounded stream.
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {ct}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                );
                if stream.write_all(head.as_bytes()).is_err() {
                    return;
                }
                for _ in 0..repeats {
                    for piece in body.chunks(8192) {
                        let hdr = format!("{:x}\r\n", piece.len());
                        if stream.write_all(hdr.as_bytes()).is_err()
                            || stream.write_all(piece).is_err()
                            || stream.write_all(b"\r\n").is_err()
                        {
                            return;
                        }
                        let _ = stream.flush();
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
                let _ = stream.write_all(b"0\r\n\r\n");
                let _ = stream.flush();
            });
        }
    });

    TestServer { url, requests }
}

/// Encode `secs` of a `freq` Hz sine as MP3 via ffmpeg. Returns `None` if
/// ffmpeg isn't installed (the test then skips).
fn encode_mp3(secs: f32, freq: f32) -> Option<Vec<u8>> {
    use std::process::Command;
    let wav = wav_bytes(secs, freq);
    let dir = std::env::temp_dir();
    let wav_path = dir.join(format!("rbstream_src_{}.wav", std::process::id()));
    let mp3_path = dir.join(format!("rbstream_src_{}.mp3", std::process::id()));
    std::fs::write(&wav_path, &wav).ok()?;
    let status = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "quiet", "-i"])
        .arg(&wav_path)
        .args(["-codec:a", "libmp3lame", "-b:a", "128k"])
        .arg(&mp3_path)
        .status();
    let out = match status {
        Ok(s) if s.success() => std::fs::read(&mp3_path).ok(),
        _ => None,
    };
    let _ = std::fs::remove_file(&wav_path);
    let _ = std::fs::remove_file(&mp3_path);
    out
}

#[test]
fn plays_an_infinite_http_stream() {
    // A live MP3 stream with no Content-Length; format from audio/mpeg.
    let Some(mp3) = encode_mp3(0.5, 440.0) else {
        eprintln!("ffmpeg not installed — skipping infinite-stream test");
        return;
    };
    // Repeat the clip enough times that the stream outlives the assertions.
    let server = spawn_stream_server(mp3, 40, "audio/mpeg");

    let Ok(player) = Player::new() else {
        eprintln!("no output device — skipping");
        return;
    };
    player.set_volume(0.05);
    player.set_queue(vec![server.url.clone()]);
    player.play();

    // It must start playing and keep playing (unbounded — no auto-stop).
    assert!(
        wait_until(&player, Duration::from_secs(6), |p| {
            p.status().state == PlaybackState::Playing
        }),
        "live stream should start playing"
    );
    // Confirm position advances (audio really flows), and it does NOT stop.
    assert!(
        wait_until(&player, Duration::from_secs(4), |p| {
            p.status().position >= Duration::from_millis(300)
        }),
        "live stream position should advance"
    );
    assert_eq!(
        player.status().state,
        PlaybackState::Playing,
        "an unbounded stream must not auto-stop"
    );

    // A manual stop must interrupt the never-ending stream promptly.
    player.stop();
    assert!(
        wait_until(&player, Duration::from_secs(3), |p| {
            p.status().state == PlaybackState::Stopped
        }),
        "stop() should interrupt a live stream"
    );
}
