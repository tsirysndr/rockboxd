//! HLS / MPEG-DASH adaptive-streaming tests against a tiny in-process HTTP
//! server (no audio device or real decode needed): manifest classification
//! via `source::open_remote`, segment fetching order, live playlist reload,
//! and plain-playlist redirection.
#![cfg(feature = "http")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use rockbox_playback::source::{open_remote, Remote};

/// A response body + content type. Each path holds a list of versions —
/// request N gets version `min(N, len-1)`, letting tests model a live
/// playlist that grows between reloads.
#[derive(Clone)]
struct Route {
    content_type: String,
    versions: Vec<Vec<u8>>,
    hits: usize,
}

struct TestServer {
    base: String,
    routes: Arc<Mutex<HashMap<String, Route>>>,
}

impl TestServer {
    fn start(routes: Vec<(&str, &str, Vec<Vec<u8>>)>) -> Self {
        let map: HashMap<String, Route> = routes
            .into_iter()
            .map(|(path, ct, versions)| {
                (
                    path.to_string(),
                    Route {
                        content_type: ct.to_string(),
                        versions,
                        hits: 0,
                    },
                )
            })
            .collect();
        let routes = Arc::new(Mutex::new(map));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());

        let served = Arc::clone(&routes);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let served = Arc::clone(&served);
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request_line = String::new();
                    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                        return;
                    }
                    let path = request_line
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("/")
                        .to_string();
                    // Drain headers.
                    let mut line = String::new();
                    loop {
                        line.clear();
                        if reader.read_line(&mut line).unwrap_or(0) == 0
                            || line == "\r\n"
                            || line == "\n"
                        {
                            break;
                        }
                    }
                    let resp = {
                        let mut routes = served.lock().unwrap();
                        match routes.get_mut(&path) {
                            Some(route) => {
                                let v = route.versions[route.hits.min(route.versions.len() - 1)]
                                    .clone();
                                route.hits += 1;
                                let mut head = format!(
                                    "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                    route.content_type,
                                    v.len()
                                )
                                .into_bytes();
                                head.extend_from_slice(&v);
                                head
                            }
                            None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                                .to_vec(),
                        }
                    };
                    let _ = stream.write_all(&resp);
                });
            }
        });
        TestServer { base, routes }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn hits(&self, path: &str) -> usize {
        self.routes.lock().unwrap().get(path).map_or(0, |r| r.hits)
    }
}

/// A fake ADTS-looking segment: valid sync bytes then filler.
fn adts_segment(fill: u8, len: usize) -> Vec<u8> {
    let mut v = vec![0xFF, 0xF1, 0x50, 0x80];
    v.resize(len, fill);
    v
}

#[test]
fn hls_vod_master_to_segments() {
    let seg0 = adts_segment(0xA0, 500);
    let seg1 = adts_segment(0xA1, 600);
    let seg2 = adts_segment(0xA2, 700);
    let master =
        b"#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2\"\nmedia.m3u8\n".to_vec();
    let media = b"#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXTINF:9.5,\nseg0.aac\n#EXTINF:10.0,\nseg1.aac\n#EXTINF:8.5,\nseg2.aac\n#EXT-X-ENDLIST\n"
        .to_vec();

    let server = TestServer::start(vec![
        (
            "/master.m3u8",
            "application/vnd.apple.mpegurl",
            vec![master],
        ),
        ("/media.m3u8", "application/vnd.apple.mpegurl", vec![media]),
        ("/seg0.aac", "audio/aac", vec![seg0.clone()]),
        ("/seg1.aac", "audio/aac", vec![seg1.clone()]),
        ("/seg2.aac", "audio/aac", vec![seg2.clone()]),
    ]);

    let remote = open_remote(&server.url("/master.m3u8")).unwrap();
    let Remote::Adaptive(mut stream) = remote else {
        panic!("expected an adaptive stream for an HLS master playlist");
    };
    assert_eq!(stream.format_ext(), "aac");
    assert!(!stream.is_live());
    let dur = stream.duration().expect("VOD duration");
    assert!((dur.as_secs_f64() - 28.0).abs() < 1e-6);
    assert_eq!(stream.kind_label(), "HLS");

    let mut all = Vec::new();
    stream.read_to_end(&mut all).unwrap();
    let expected: Vec<u8> = [seg0, seg1, seg2].concat();
    assert_eq!(all, expected);
}

#[test]
fn hls_live_playlist_growth_and_end() {
    let seg0 = adts_segment(0xB0, 300);
    let seg1 = adts_segment(0xB1, 300);
    let seg2 = adts_segment(0xB2, 300);
    let head = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n";
    let v1 = format!("{head}#EXTINF:1,\nseg0.aac\n#EXTINF:1,\nseg1.aac\n").into_bytes();
    let v2 = format!(
        "{head}#EXTINF:1,\nseg0.aac\n#EXTINF:1,\nseg1.aac\n#EXTINF:1,\nseg2.aac\n#EXT-X-ENDLIST\n"
    )
    .into_bytes();

    let server = TestServer::start(vec![
        // Request 1 = probe, 2 = manifest open, 3+ = live reloads → grown+ended.
        (
            "/live.m3u8",
            "application/x-mpegURL",
            vec![v1.clone(), v1, v2],
        ),
        ("/seg0.aac", "audio/aac", vec![seg0.clone()]),
        ("/seg1.aac", "audio/aac", vec![seg1.clone()]),
        ("/seg2.aac", "audio/aac", vec![seg2.clone()]),
    ]);

    let remote = open_remote(&server.url("/live.m3u8")).unwrap();
    let Remote::Adaptive(mut stream) = remote else {
        panic!("expected an adaptive stream");
    };
    assert!(stream.is_live());
    assert_eq!(stream.duration(), None);

    let mut all = Vec::new();
    stream.read_to_end(&mut all).unwrap(); // blocks through one reload cycle
    let expected: Vec<u8> = [seg0, seg1, seg2].concat();
    assert_eq!(all, expected);
    assert!(server.hits("/live.m3u8") >= 3, "playlist was reloaded");
}

#[test]
fn dash_static_number_template() {
    let seg1 = adts_segment(0xC1, 400);
    let seg2 = adts_segment(0xC2, 450);
    let mpd = br#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT20S">
  <Period>
    <AdaptationSet contentType="audio" mimeType="audio/aac">
      <SegmentTemplate media="seg-$Number$.aac" duration="10" timescale="1" startNumber="1"/>
      <Representation id="a" bandwidth="128000" audioSamplingRate="44100"/>
    </AdaptationSet>
  </Period>
</MPD>"#
        .to_vec();

    let server = TestServer::start(vec![
        ("/stream.mpd", "application/dash+xml", vec![mpd]),
        ("/seg-1.aac", "audio/aac", vec![seg1.clone()]),
        ("/seg-2.aac", "audio/aac", vec![seg2.clone()]),
    ]);

    let remote = open_remote(&server.url("/stream.mpd")).unwrap();
    let Remote::Adaptive(mut stream) = remote else {
        panic!("expected an adaptive stream for a DASH MPD");
    };
    assert_eq!(stream.kind_label(), "DASH");
    assert_eq!(stream.format_ext(), "aac");
    assert_eq!(stream.sample_rate(), 44100);
    assert!(!stream.is_live());
    assert_eq!(stream.duration().unwrap().as_secs(), 20);

    let mut all = Vec::new();
    stream.read_to_end(&mut all).unwrap();
    assert_eq!(all, [seg1, seg2].concat());
}

#[test]
fn plain_m3u_redirects_to_media_file() {
    let mp3 = vec![0x11u8; 4096];
    let playlist = b"#EXTM3U\n#EXTINF:-1,Some Station\n/track.mp3\n".to_vec();
    let server = TestServer::start(vec![
        ("/list.m3u8", "audio/x-mpegurl", vec![playlist]),
        ("/track.mp3", "audio/mpeg", vec![mp3]),
    ]);

    // A plain M3U (no EXT-X tags) must fall through to its first entry,
    // which probes as a finite file.
    let remote = open_remote(&server.url("/list.m3u8")).unwrap();
    match remote {
        Remote::File(src) => {
            use rockbox_playback::MediaSource;
            assert_eq!(src.size(), 4096);
        }
        _ => panic!("expected the playlist to redirect to a plain file"),
    }
}
