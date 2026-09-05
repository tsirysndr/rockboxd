//! Adaptive streaming: **HLS** (`.m3u8`) and **MPEG-DASH** (`.mpd`).
//!
//! A manifest URL resolves to an [`AdaptiveStream`] — a forward-only
//! [`Read`] the decoder pulls like any live radio stream. Under the hood it:
//!
//! 1. parses the manifest ([`hls`] / [`dash`]) and picks the best audio
//!    rendition,
//! 2. fetches media segments sequentially (reloading the playlist/manifest
//!    for live streams),
//! 3. demuxes each segment container down to a raw audio bitstream the
//!    Rockbox codecs decode directly: MPEG-TS ([`ts`]) and fragmented MP4
//!    ([`fmp4`]) become ADTS AAC or MP3; raw `.aac`/`.mp3` segments pass
//!    through (with ID3 timed-metadata tags stripped).
//!
//! VOD presentations report a total duration and end normally; live ones
//! play until the origin stops publishing. Seeking is not supported (the
//! stream is consumed forward-only), matching the existing internet-radio
//! path.

pub mod dash;
pub mod fmp4;
pub mod hls;
pub mod ts;

use std::collections::VecDeque;
use std::io::{self, Read};
use std::time::{Duration, Instant, SystemTime};

use reqwest::blocking::Client;

/// Manifests are small; refuse to buffer absurd ones.
const MAX_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;
/// Give up on a live stream after this many playlist reloads with nothing new.
const MAX_STALE_RELOADS: u32 = 10;
/// Give up on a live segment after this many not-yet-available retries.
const MAX_MISSING_RETRIES: u32 = 20;
/// How many segments from the live edge to start playback at.
const LIVE_EDGE_BACKOFF: usize = 3;

fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::other(e.to_string())
}

// ---------------------------------------------------------------------------
// URL resolution
// ---------------------------------------------------------------------------

/// Resolve `rel` against `base` (RFC 3986-style, enough for media manifests):
/// absolute URLs pass through; `//host/…`, `/path`, and relative paths join
/// against the base's scheme / origin / directory, with `.`/`..` segments
/// normalized.
pub fn url_join(base: &str, rel: &str) -> String {
    let rel = rel.trim();
    if rel.contains("://") {
        return rel.to_string();
    }
    let scheme_end = base.find("://").map(|p| p + 3).unwrap_or(0);
    if let Some(rest) = rel.strip_prefix("//") {
        return format!("{}//{}", &base[..scheme_end.saturating_sub(2)], rest);
    }
    let origin_end = base[scheme_end..]
        .find('/')
        .map(|p| scheme_end + p)
        .unwrap_or(base.len());
    if rel.starts_with('/') {
        return format!("{}{}", &base[..origin_end], normalize_path(rel));
    }
    // Relative: replace everything after the last '/' of the base path
    // (query/fragment stripped).
    let path_end = base[origin_end..]
        .find(['?', '#'])
        .map(|p| origin_end + p)
        .unwrap_or(base.len());
    let dir_end = base[origin_end..path_end]
        .rfind('/')
        .map(|p| origin_end + p + 1)
        .unwrap_or(path_end);
    if dir_end <= origin_end {
        return format!("{}/{}", &base[..origin_end], rel);
    }
    let joined_path = format!("{}{}", &base[origin_end..dir_end], rel);
    format!("{}{}", &base[..origin_end], normalize_path(&joined_path))
}

/// Remove `.` and resolvable `..` segments from a URL path (query and
/// fragment, if present, ride along untouched).
fn normalize_path(path: &str) -> String {
    let (path, suffix) = match path.find(['?', '#']) {
        Some(p) => (&path[..p], &path[p..]),
        None => (path, ""),
    };
    let trailing_slash = path.ends_with('/');
    let mut segs: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                segs.pop();
            }
            s => segs.push(s),
        }
    }
    let mut out = String::from("/");
    out.push_str(&segs.join("/"));
    if trailing_slash && out.len() > 1 {
        out.push('/');
    }
    out.push_str(suffix);
    out
}

// ---------------------------------------------------------------------------
// Manifest detection / opening
// ---------------------------------------------------------------------------

/// Does the URL or `Content-Type` suggest a manifest (HLS/DASH/plain
/// playlist) that must be downloaded and inspected rather than decoded?
pub fn looks_like_manifest(url: &str, content_type: Option<&str>) -> bool {
    if let Some(ct) = content_type {
        let ct = ct
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if ct.contains("mpegurl") || ct.contains("dash+xml") || ct.contains("scpls") {
            return true;
        }
    }
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('/').next().and_then(|n| n.rsplit_once('.'));
    matches!(
        ext.map(|(_, e)| e.to_ascii_lowercase()).as_deref(),
        Some("m3u8") | Some("m3u") | Some("mpd") | Some("pls")
    )
}

/// What opening a manifest URL resolved to.
pub enum ManifestOutcome {
    /// An adaptive stream ready to decode.
    Adaptive(AdaptiveStream),
    /// The manifest was a plain playlist (M3U/PLS) or a degenerate DASH
    /// presentation pointing at one media file — open this URL instead.
    Redirect(String),
}

/// Download and classify the manifest at `url`, following HLS master →
/// media playlists, and build the stream.
pub fn open_manifest(client: &Client, url: &str) -> io::Result<ManifestOutcome> {
    let mut url = url.to_string();
    // Follow nested playlists (HLS master → media, plain playlist → entry)
    // a bounded number of times.
    for _ in 0..4 {
        let text = fetch_text(client, &url)?;
        if text.contains("<MPD") {
            return open_dash(client, &url, &text);
        }
        if text.contains("#EXT-X-") || text.trim_start().starts_with("#EXTM3U") {
            if hls::is_master(&text) {
                let master = hls::parse_master(&text, &url);
                url = hls::select_media_playlist(&master)
                    .ok_or_else(|| to_io("HLS master playlist has no playable variant"))?;
                continue;
            }
            if hls::is_hls(&text) {
                return open_hls(client, &url, &text);
            }
            // Plain M3U: redirect to its first entry.
            let entry = text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with('#'))
                .ok_or_else(|| to_io("playlist has no entries"))?;
            return Ok(ManifestOutcome::Redirect(url_join(&url, entry)));
        }
        if text.to_ascii_lowercase().contains("[playlist]") {
            // PLS: FileN=<url>
            let entry = text
                .lines()
                .filter_map(|l| l.trim().split_once('='))
                .find(|(k, _)| k.trim().to_ascii_lowercase().starts_with("file"))
                .map(|(_, v)| v.trim().to_string())
                .ok_or_else(|| to_io("PLS playlist has no File entries"))?;
            return Ok(ManifestOutcome::Redirect(url_join(&url, &entry)));
        }
        return Err(to_io(format!("unrecognized manifest at {url}")));
    }
    Err(to_io("manifest nesting too deep"))
}

fn open_hls(client: &Client, url: &str, text: &str) -> io::Result<ManifestOutcome> {
    let playlist = hls::parse_media(text, url);
    if playlist.encrypted {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "HLS stream is encrypted (EXT-X-KEY), which is not supported",
        ));
    }
    if playlist.segments.is_empty() {
        return Err(to_io("HLS media playlist has no segments"));
    }
    let live = !playlist.endlist;
    let duration = (!live).then(|| Duration::from_secs_f64(playlist.total_duration()));
    let next_seq = if live {
        let start = playlist.segments.len().saturating_sub(LIVE_EDGE_BACKOFF);
        playlist.segments[start].sequence
    } else {
        playlist.segments[0].sequence
    };
    let source = SourceKind::Hls(HlsSource {
        playlist_url: url.to_string(),
        playlist,
        next_seq,
        last_load: Instant::now(),
        stale_reloads: 0,
    });
    AdaptiveStream::build(client.clone(), source, "HLS", live, duration, None)
        .map(ManifestOutcome::Adaptive)
}

fn open_dash(client: &Client, url: &str, text: &str) -> io::Result<ManifestOutcome> {
    let mpd = dash::parse_mpd(text)?;
    let now = unix_now();
    let plan = dash::build_plan(&mpd, url, now)?;
    if let Some(single) = plan.single_file {
        return Ok(ManifestOutcome::Redirect(single));
    }
    let live = plan.dynamic;
    let duration = plan
        .total_duration
        .filter(|_| !live)
        .map(Duration::from_secs_f64);
    let sample_rate = plan.sample_rate;
    let last_key = plan.segments.last().map(|s| s.key);
    let source = SourceKind::Dash(DashSource {
        mpd_url: url.to_string(),
        segments: plan.segments.into(),
        open: plan.open_template,
        dynamic: plan.dynamic,
        update_period: plan.update_period,
        last_key,
        last_load: Instant::now(),
        stale_reloads: 0,
    });
    AdaptiveStream::build(client.clone(), source, "DASH", live, duration, sample_rate)
        .map(ManifestOutcome::Adaptive)
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// GET a small text resource (the manifest).
fn fetch_text(client: &Client, url: &str) -> io::Result<String> {
    let resp = client
        .get(url)
        .send()
        .map_err(to_io)?
        .error_for_status()
        .map_err(to_io)?;
    let mut buf = Vec::new();
    resp.take(MAX_MANIFEST_BYTES).read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// GET a segment (optionally a byte range: `(offset, length)`).
fn fetch_bytes(client: &Client, url: &str, range: Option<(u64, u64)>) -> io::Result<Vec<u8>> {
    let mut req = client.get(url);
    if let Some((off, len)) = range {
        req = req.header(
            reqwest::header::RANGE,
            format!("bytes={}-{}", off, off + len - 1),
        );
    }
    let resp = req.send().map_err(to_io)?;
    let status = resp.status();
    if !status.is_success() {
        return Err(io::Error::new(
            if status.as_u16() == 404 || status.as_u16() == 410 {
                io::ErrorKind::NotFound
            } else {
                io::ErrorKind::Other
            },
            format!("segment fetch failed: HTTP {} for {url}", status.as_u16()),
        ));
    }
    let mut buf = Vec::new();
    let mut resp = resp;
    resp.read_to_end(&mut buf).map_err(to_io)?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Segment sources
// ---------------------------------------------------------------------------

/// A segment to fetch: media URL (+ optional byte range) and the init
/// segment in effect for it.
#[derive(Debug, Clone)]
struct SegmentDesc {
    url: String,
    byte_range: Option<(u64, u64)>,
    init: Option<InitDesc>,
}

#[derive(Debug, Clone, PartialEq)]
struct InitDesc {
    url: String,
    byte_range: Option<(u64, u64)>,
}

enum SourceKind {
    Hls(HlsSource),
    Dash(DashSource),
}

struct HlsSource {
    playlist_url: String,
    playlist: hls::MediaPlaylist,
    /// Media-sequence number of the next segment to serve.
    next_seq: u64,
    last_load: Instant,
    stale_reloads: u32,
}

struct DashSource {
    mpd_url: String,
    segments: VecDeque<dash::SegmentRef>,
    open: Option<dash::OpenTemplate>,
    dynamic: bool,
    update_period: f64,
    /// Highest segment key handed out (timeline dedup across refreshes).
    last_key: Option<u64>,
    last_load: Instant,
    stale_reloads: u32,
}

impl SourceKind {
    /// The next segment to fetch. Blocks (sleeps) while waiting for a live
    /// playlist to grow. `Ok(None)` = the stream ended.
    fn next(&mut self, client: &Client) -> io::Result<Option<SegmentDesc>> {
        match self {
            SourceKind::Hls(h) => h.next(client),
            SourceKind::Dash(d) => d.next(client),
        }
    }

    /// Is a missing (404) segment worth retrying (live edge race)?
    fn tolerates_missing(&self) -> bool {
        match self {
            SourceKind::Hls(h) => !h.playlist.endlist,
            SourceKind::Dash(d) => d.dynamic,
        }
    }

    /// How long to wait before retrying a not-yet-available segment.
    fn retry_interval(&self) -> Duration {
        let secs = match self {
            SourceKind::Hls(h) => (h.playlist.target_duration / 2.0).max(0.5),
            SourceKind::Dash(d) => d
                .open
                .as_ref()
                .map(|o| (o.segment_seconds / 2.0).max(0.5))
                .unwrap_or_else(|| (d.update_period / 2.0).max(0.5)),
        };
        Duration::from_secs_f64(secs.min(5.0))
    }
}

impl HlsSource {
    fn next(&mut self, client: &Client) -> io::Result<Option<SegmentDesc>> {
        loop {
            if let Some(seg) = self
                .playlist
                .segments
                .iter()
                .find(|s| s.sequence >= self.next_seq)
            {
                self.next_seq = seg.sequence + 1;
                return Ok(Some(SegmentDesc {
                    url: seg.uri.clone(),
                    byte_range: seg.byte_range,
                    init: seg.map.as_ref().map(|m| InitDesc {
                        url: m.uri.clone(),
                        byte_range: m.byte_range,
                    }),
                }));
            }
            if self.playlist.endlist {
                return Ok(None);
            }
            // Live: reload the playlist, paced to the target duration.
            let interval =
                Duration::from_secs_f64((self.playlist.target_duration / 2.0).clamp(1.0, 15.0));
            let since = self.last_load.elapsed();
            if since < interval {
                std::thread::sleep(interval - since);
            }
            let text = fetch_text(client, &self.playlist_url)?;
            self.last_load = Instant::now();
            let fresh = hls::parse_media(&text, &self.playlist_url);
            if fresh.encrypted {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "HLS stream became encrypted mid-play",
                ));
            }
            let has_new = fresh.segments.iter().any(|s| s.sequence >= self.next_seq);
            self.playlist = fresh;
            if has_new || self.playlist.endlist {
                self.stale_reloads = 0;
            } else {
                self.stale_reloads += 1;
                if self.stale_reloads > MAX_STALE_RELOADS {
                    return Ok(None); // origin stopped publishing
                }
            }
        }
    }
}

impl DashSource {
    fn next(&mut self, client: &Client) -> io::Result<Option<SegmentDesc>> {
        loop {
            if let Some(seg) = self.segments.pop_front() {
                self.last_key = Some(self.last_key.map_or(seg.key, |k| k.max(seg.key)));
                return Ok(Some(SegmentDesc {
                    url: seg.url,
                    byte_range: None,
                    init: seg.init_url.map(|u| InitDesc {
                        url: u,
                        byte_range: None,
                    }),
                }));
            }
            if let Some(open) = self.open.as_mut() {
                let number = open.next_number;
                open.next_number += 1;
                let url = open.media_url(number);
                let init = open.init_url.clone();
                return Ok(Some(SegmentDesc {
                    url,
                    byte_range: None,
                    init: init.map(|u| InitDesc {
                        url: u,
                        byte_range: None,
                    }),
                }));
            }
            if !self.dynamic {
                return Ok(None);
            }
            // Dynamic timeline: re-fetch the MPD and append segments newer
            // than everything already played.
            let interval = Duration::from_secs_f64(self.update_period.clamp(1.0, 15.0));
            let since = self.last_load.elapsed();
            if since < interval {
                std::thread::sleep(interval - since);
            }
            let text = fetch_text(client, &self.mpd_url)?;
            self.last_load = Instant::now();
            let mpd = dash::parse_mpd(&text)?;
            let plan = dash::build_plan(&mpd, &self.mpd_url, unix_now())?;
            self.open = plan.open_template;
            let cutoff = self.last_key;
            let fresh: Vec<dash::SegmentRef> = plan
                .segments
                .into_iter()
                .filter(|s| cutoff.is_none_or(|k| s.key > k))
                .collect();
            if fresh.is_empty() && self.open.is_none() {
                self.stale_reloads += 1;
                if self.stale_reloads > MAX_STALE_RELOADS {
                    return Ok(None);
                }
            } else {
                self.stale_reloads = 0;
                self.segments.extend(fresh);
            }
            if !plan.dynamic {
                self.dynamic = false; // the live event ended (now static)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The stream
// ---------------------------------------------------------------------------

enum Demux {
    /// Not yet determined (before the first segment).
    Unknown,
    /// Segments are already a raw bitstream (`.aac`/`.mp3`/… segments).
    Passthrough,
    Ts(ts::TsDemuxer),
    Fmp4(fmp4::Fmp4Demuxer),
}

/// A forward-only byte stream over HLS/DASH media segments. Implements
/// [`Read`]; hand it to `Decoder::open_stream` with [`format_ext`]
/// (`AdaptiveStream::format_ext`).
pub struct AdaptiveStream {
    client: Client,
    source: SourceKind,
    demux: Demux,
    /// Init-segment cache: the descriptor last fed to the demuxer.
    current_init: Option<InitDesc>,
    buf: Vec<u8>,
    pos: usize,
    ext: String,
    duration: Option<Duration>,
    live: bool,
    label: &'static str,
    sample_rate: u32,
    ended: bool,
}

impl AdaptiveStream {
    /// Construct and prime: the first segment is fetched eagerly so the
    /// codec extension is known before the decoder opens.
    fn build(
        client: Client,
        source: SourceKind,
        label: &'static str,
        live: bool,
        duration: Option<Duration>,
        sample_rate: Option<u32>,
    ) -> io::Result<Self> {
        let mut s = AdaptiveStream {
            client,
            source,
            demux: Demux::Unknown,
            current_init: None,
            buf: Vec::new(),
            pos: 0,
            ext: String::new(),
            duration,
            live,
            label,
            sample_rate: sample_rate.unwrap_or(0),
            ended: false,
        };
        if !s.refill()? {
            return Err(to_io("adaptive stream has no media data"));
        }
        Ok(s)
    }

    /// Codec format extension (`"aac"`, `"mp3"`, …) for
    /// `Decoder::open_stream`.
    pub fn format_ext(&self) -> &str {
        &self.ext
    }

    /// Total duration, known for VOD presentations only.
    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Is this a live (unbounded) presentation?
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// `"HLS"` or `"DASH"` — for the codec label in metadata.
    pub fn kind_label(&self) -> &'static str {
        self.label
    }

    /// Declared sample rate in Hz (0 = unknown until decode).
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Fetch + demux segments until the buffer has data. `Ok(false)` = end
    /// of stream.
    fn refill(&mut self) -> io::Result<bool> {
        if self.ended {
            return Ok(false);
        }
        let mut missing_retries = 0u32;
        let mut pending: Option<SegmentDesc> = None;
        loop {
            let desc = match pending.take() {
                Some(d) => d,
                None => match self.source.next(&self.client)? {
                    Some(d) => d,
                    None => {
                        self.ended = true;
                        return Ok(false);
                    }
                },
            };
            let data = match fetch_bytes(&self.client, &desc.url, desc.byte_range) {
                Ok(d) => d,
                Err(e)
                    if e.kind() == io::ErrorKind::NotFound
                        && self.source.tolerates_missing()
                        && missing_retries < MAX_MISSING_RETRIES =>
                {
                    // Live-edge race: the manifest advertised a segment the
                    // origin hasn't published yet. Wait and retry it.
                    missing_retries += 1;
                    std::thread::sleep(self.source.retry_interval());
                    pending = Some(desc);
                    continue;
                }
                Err(e) => return Err(e),
            };
            missing_retries = 0;

            self.buf.clear();
            self.pos = 0;
            self.demux_segment(&desc, &data)?;
            if !self.buf.is_empty() {
                return Ok(true);
            }
            // Segment produced no audio (e.g. an init-only chunk) — keep going.
        }
    }

    fn demux_segment(&mut self, desc: &SegmentDesc, data: &[u8]) -> io::Result<()> {
        // (Re)configure on the first segment, or when the init segment
        // changes (e.g. a DASH period transition).
        let init_changed = desc.init != self.current_init;
        if matches!(self.demux, Demux::Unknown) || (init_changed && desc.init.is_some()) {
            self.configure(desc, data)?;
            self.current_init = desc.init.clone();
        }
        match &mut self.demux {
            Demux::Unknown => unreachable!("configure sets a demuxer"),
            Demux::Passthrough => {
                self.buf.extend_from_slice(strip_id3(data));
                Ok(())
            }
            Demux::Ts(demux) => demux.feed(data, &mut self.buf),
            Demux::Fmp4(demux) => demux.segment(data, &mut self.buf),
        }
    }

    /// Decide the container and configure the demuxer from the first
    /// segment (and its init segment, when there is one).
    fn configure(&mut self, desc: &SegmentDesc, media: &[u8]) -> io::Result<()> {
        if let Some(init) = &desc.init {
            let init_data = fetch_bytes(&self.client, &init.url, init.byte_range)?;
            let demux = fmp4::Fmp4Demuxer::init(&init_data)?;
            self.finish_configure_fmp4(demux);
            return Ok(());
        }
        if fmp4::looks_like_mp4(media) {
            // Self-initializing segment (moov inline).
            let demux = fmp4::Fmp4Demuxer::init(media)?;
            self.finish_configure_fmp4(demux);
            return Ok(());
        }
        if ts::looks_like_ts(media) {
            self.demux = Demux::Ts(ts::TsDemuxer::new());
            // The extension comes from the PMT — probe it on this segment.
            let mut probe_out = Vec::new();
            let mut probe = ts::TsDemuxer::new();
            probe.feed(media, &mut probe_out)?;
            self.ext = probe
                .kind()
                .map(|k| k.ext().to_string())
                .ok_or_else(|| to_io("MPEG-TS segment has no PAT/PMT"))?;
            return Ok(());
        }
        self.demux = Demux::Passthrough;
        self.ext = sniff_ext(strip_id3(media), &desc.url);
        Ok(())
    }

    fn finish_configure_fmp4(&mut self, demux: fmp4::Fmp4Demuxer) {
        self.ext = demux.ext().to_string();
        if self.sample_rate == 0 {
            self.sample_rate = demux.sample_rate().unwrap_or(0);
        }
        self.demux = Demux::Fmp4(demux);
    }
}

impl Read for AdaptiveStream {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        while self.pos >= self.buf.len() {
            if !self.refill()? {
                return Ok(0);
            }
        }
        let n = (self.buf.len() - self.pos).min(out.len());
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Skip a leading ID3v2 tag (HLS "packed audio" segments carry timed
/// metadata this way; the codec would choke on it).
fn strip_id3(data: &[u8]) -> &[u8] {
    if data.len() < 10 || &data[..3] != b"ID3" {
        return data;
    }
    // Syncsafe 28-bit size after the 6-byte header.
    let size = ((data[6] as usize & 0x7F) << 21)
        | ((data[7] as usize & 0x7F) << 14)
        | ((data[8] as usize & 0x7F) << 7)
        | (data[9] as usize & 0x7F);
    let total = 10 + size;
    data.get(total..).map(strip_id3).unwrap_or(data)
}

/// Format extension for a raw (already-elementary) segment: trust a known
/// URL extension, else sniff the bytes.
fn sniff_ext(data: &[u8], url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    if let Some((_, e)) = path.rsplit('/').next().and_then(|n| n.rsplit_once('.')) {
        let e = e.to_ascii_lowercase();
        if matches!(
            e.as_str(),
            "aac" | "mp3" | "mp2" | "mp1" | "mpa" | "ogg" | "oga" | "opus" | "flac" | "wav"
        ) {
            return e;
        }
    }
    if data.len() >= 4 {
        if &data[..4] == b"OggS" {
            return "ogg".into();
        }
        if &data[..4] == b"fLaC" {
            return "flac".into();
        }
        if data[0] == 0xFF && data[1] & 0xF6 == 0xF0 {
            return "aac".into(); // ADTS sync
        }
        if data[0] == 0xFF && data[1] & 0xE0 == 0xE0 {
            return "mp3".into(); // MPEG audio sync
        }
    }
    "aac".into() // most common HLS packed-audio payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_join_cases() {
        let base = "https://cdn.example.com/live/hls/main.m3u8?token=x";
        assert_eq!(
            url_join(base, "seg1.ts"),
            "https://cdn.example.com/live/hls/seg1.ts"
        );
        assert_eq!(
            url_join(base, "../audio/seg1.ts"),
            "https://cdn.example.com/live/audio/seg1.ts"
        );
        assert_eq!(
            url_join(base, "/root/seg.ts"),
            "https://cdn.example.com/root/seg.ts"
        );
        assert_eq!(
            url_join(base, "https://other.com/a.ts"),
            "https://other.com/a.ts"
        );
        assert_eq!(url_join(base, "//other.com/a.ts"), "https://other.com/a.ts");
        assert_eq!(url_join("https://h.com", "x.ts"), "https://h.com/x.ts");
        // Query strings on the relative part survive.
        assert_eq!(
            url_join(base, "seg1.ts?auth=1"),
            "https://cdn.example.com/live/hls/seg1.ts?auth=1"
        );
    }

    #[test]
    fn manifest_detection() {
        assert!(looks_like_manifest("http://h/x.m3u8", None));
        assert!(looks_like_manifest("http://h/x.m3u8?tok=1", None));
        assert!(looks_like_manifest("http://h/x.mpd", None));
        assert!(looks_like_manifest("http://h/x.pls", None));
        assert!(looks_like_manifest(
            "http://h/stream",
            Some("application/vnd.apple.mpegurl")
        ));
        assert!(looks_like_manifest(
            "http://h/stream",
            Some("application/dash+xml; charset=utf-8")
        ));
        assert!(!looks_like_manifest("http://h/x.mp3", None));
        assert!(!looks_like_manifest("http://h/stream", Some("audio/mpeg")));
    }

    #[test]
    fn id3_stripping() {
        // 20-byte payload after a 10+6-byte ID3 tag.
        let mut seg = b"ID3\x04\x00\x00\x00\x00\x00\x06".to_vec();
        seg.extend_from_slice(&[0u8; 6]); // tag body
        seg.extend_from_slice(&[0xFF, 0xF1, 0xAA, 0xBB]);
        assert_eq!(strip_id3(&seg), &[0xFF, 0xF1, 0xAA, 0xBB]);
        // No tag → unchanged.
        assert_eq!(strip_id3(&[0xFF, 0xF1]), &[0xFF, 0xF1]);
    }

    #[test]
    fn ext_sniffing() {
        assert_eq!(sniff_ext(&[0xFF, 0xF1, 0, 0], "http://h/seg.aac"), "aac");
        assert_eq!(sniff_ext(&[0xFF, 0xFB, 0, 0], "http://h/seg.bin"), "mp3");
        assert_eq!(sniff_ext(&[0xFF, 0xF9, 0, 0], "http://h/seg"), "aac");
        assert_eq!(sniff_ext(b"OggS\0\0", "http://h/x"), "ogg");
        assert_eq!(
            sniff_ext(&[0xFF, 0xFB, 0, 0], "http://h/seg.mp3?a=1"),
            "mp3"
        );
    }

    #[test]
    fn path_normalization() {
        assert_eq!(normalize_path("/a/b/../c.ts"), "/a/c.ts");
        assert_eq!(normalize_path("/a/./b.ts"), "/a/b.ts");
        assert_eq!(normalize_path("/../x.ts"), "/x.ts");
        assert_eq!(normalize_path("/a/b/"), "/a/b/");
        assert_eq!(normalize_path("/a/s.ts?q=1"), "/a/s.ts?q=1");
    }
}
