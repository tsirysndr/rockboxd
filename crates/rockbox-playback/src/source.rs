//! A seekable media source that can be backed by a **local file** or a
//! **remote HTTP(S) URL**.
//!
//! The Rockbox codec and metadata layers read their input as a random-access,
//! seekable byte stream (`read` / `lseek` / `filesize`). [`MediaSource`]
//! captures exactly that contract so a track can live on disk or on a server.
//!
//! - [`FileSource`] wraps a local file.
//! - [`HttpSource`] (feature `http`) fetches byte ranges on demand with
//!   [`reqwest`], caching them in a seekable temp file. Because the codec and
//!   metadata parsers need arbitrary seeks (including `get_metadata`, which is
//!   vendored firmware and can't be intercepted), the engine materializes the
//!   cache before decoding — but the fetch itself is a single open-ended HTTP
//!   **range request** (`Range: bytes=0-`), and partial reads/seeks issue
//!   smaller ranged requests, so the type is a faithful random-access source.

use std::io::{self, Read, Seek, SeekFrom};

/// Does `s` look like an HTTP(S) URL (as opposed to a filesystem path)?
pub fn is_url(s: &str) -> bool {
    let s = s.trim_start();
    s.starts_with("http://") || s.starts_with("https://")
}

/// A random-access, seekable byte source with a known total length.
pub trait MediaSource: Read + Seek + Send {
    /// Total size of the media in bytes.
    fn size(&self) -> u64;
}

/// A [`MediaSource`] backed by a local file.
pub struct FileSource {
    file: std::fs::File,
    size: u64,
}

impl FileSource {
    /// Open `path` for reading.
    pub fn open(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let size = file.metadata()?.len();
        Ok(FileSource { file, size })
    }
}

impl Read for FileSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}
impl Seek for FileSource {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.file.seek(pos)
    }
}
impl MediaSource for FileSource {
    fn size(&self) -> u64 {
        self.size
    }
}

// ---------------------------------------------------------------------------
// HTTP source
// ---------------------------------------------------------------------------

#[cfg(feature = "http")]
pub use http::{open_remote, HttpSource, HttpStream, IcyInfo, Remote};

#[cfg(feature = "http")]
mod http {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    /// Fetch at least this many extra bytes beyond a satisfied read, to
    /// amortize request overhead on sequential playback.
    const READAHEAD: u64 = 512 * 1024;

    /// A [`MediaSource`] backed by a remote HTTP(S) URL. Ranges are fetched on
    /// demand into a seekable temp-file cache; already-fetched intervals are
    /// served locally.
    pub struct HttpSource {
        client: reqwest::blocking::Client,
        url: String,
        size: u64,
        /// Whether the server honoured `Range` (else we fall back to a single
        /// full-body download on first need).
        ranges: bool,
        /// Temp file cache, pre-sized to `size`. Auto-deleted on drop.
        cache: File,
        _tmp: tempfile::TempPath,
        /// Sorted, merged half-open `[start, end)` intervals already cached.
        have: Vec<(u64, u64)>,
        pos: u64,
    }

    /// Result of a one-byte probe GET.
    struct Probe {
        /// Raw `Content-Type` header, for manifest (HLS/DASH) detection.
        content_type: Option<String>,
        /// Total length if the server reported one (`None` = unbounded stream).
        length: Option<u64>,
        /// Whether the server honoured `Range` (206).
        ranges: bool,
        /// Format extension from the `Content-Type` header, if recognised.
        mime_ext: Option<String>,
        /// The probe response — its body is the stream when unbounded.
        resp: reqwest::blocking::Response,
        /// SHOUTcast/Icecast metadata interval (`icy-metaint`), if the server
        /// interleaves in-band metadata in the audio.
        icy_metaint: Option<usize>,
        /// Station info from `icy-name` / `icy-genre` / `icy-br` headers.
        icy: IcyInfo,
    }

    /// Station-level info from an Icecast/SHOUTcast live stream's `icy-*`
    /// response headers (as opposed to the per-song `StreamTitle`).
    #[derive(Debug, Clone, Default)]
    pub struct IcyInfo {
        /// Station name (`icy-name`).
        pub name: Option<String>,
        /// Station genre (`icy-genre`).
        pub genre: Option<String>,
        /// Advertised bitrate in kbps (`icy-br`).
        pub bitrate: Option<u32>,
    }

    /// Build the HTTP client. A connect timeout keeps an unreachable or
    /// hanging host from blocking the decode/engine thread forever; no total
    /// timeout is set, so long-lived live streams and slow large fetches are
    /// not aborted mid-transfer.
    ///
    /// A `User-Agent` is always sent: some stream hosts (e.g. zeno.fm) reject
    /// requests with a missing/empty UA with `401 Unauthorized` instead of
    /// serving the stream, so an empty default would make those stations
    /// silently fail to play.
    fn build_client() -> io::Result<reqwest::blocking::Client> {
        reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .user_agent(concat!("rockbox-playback/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(to_io)
    }

    fn probe(client: &reqwest::blocking::Client, url: &str) -> io::Result<Probe> {
        let resp = client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            // Ask for in-band metadata; radio servers reply with `icy-metaint`
            // and interleave `StreamTitle` blocks. Plain files ignore this.
            .header("Icy-MetaData", "1")
            .send()
            .map_err(to_io)?
            .error_for_status()
            .map_err(to_io)?;
        let status = resp.status();
        let headers = resp.headers();

        let content_type = headers
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let mime_ext = content_type.as_deref().and_then(mime_to_ext);

        let hdr = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let icy_metaint = hdr("icy-metaint").and_then(|s| s.parse::<usize>().ok());
        let icy = IcyInfo {
            name: hdr("icy-name"),
            genre: hdr("icy-genre"),
            bitrate: hdr("icy-br").and_then(|s| s.parse::<u32>().ok()),
        };

        let (length, ranges) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
            let total = headers
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.rsplit('/').next())
                .and_then(|s| s.trim().parse::<u64>().ok());
            (total, true)
        } else {
            (resp.content_length(), false)
        };

        Ok(Probe {
            content_type,
            length,
            ranges,
            mime_ext,
            resp,
            icy_metaint,
            icy,
        })
    }

    /// A recognised format extension (no dot) from a `Content-Type`, else the
    /// URL's extension.
    fn stream_ext(mime_ext: &Option<String>, url: &str) -> String {
        let with_dot = mime_ext.clone().unwrap_or_else(|| url_suffix(url));
        with_dot.trim_start_matches('.').to_string()
    }

    impl HttpSource {
        /// Probe `url` (size + range support) and create the cache. Does not
        /// download any media body yet. Errors if the server reports no length
        /// (an unbounded stream — use [`HttpStream`] / [`open_remote`]).
        pub fn new(url: &str) -> io::Result<Self> {
            let client = build_client()?;
            let p = probe(&client, url)?;
            let size = p.length.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "server did not report a content length",
                )
            })?;
            let suffix = p.mime_ext.unwrap_or_else(|| url_suffix(url));
            HttpSource::build(client, url, size, p.ranges, &suffix)
        }

        fn build(
            client: reqwest::blocking::Client,
            url: &str,
            size: u64,
            ranges: bool,
            suffix: &str,
        ) -> io::Result<Self> {
            let named = tempfile::Builder::new()
                .prefix("rbstream_")
                .suffix(suffix)
                .tempfile()
                .map_err(to_io)?;
            named.as_file().set_len(size)?;
            let (file, path) = named.into_parts();
            Ok(HttpSource {
                client,
                url: url.to_string(),
                size,
                ranges,
                cache: file,
                _tmp: path,
                have: Vec::new(),
                pos: 0,
            })
        }

        /// Path of the seekable local cache. Only complete once every byte has
        /// been fetched — see [`HttpSource::ensure_complete`].
        pub fn cache_path(&self) -> &std::path::Path {
            &self._tmp
        }

        /// Fetch every byte not yet cached (a single ranged request for the
        /// remaining span), so [`cache_path`](Self::cache_path) is a complete
        /// copy the codec/metadata layer can open directly.
        pub fn ensure_complete(&mut self) -> io::Result<()> {
            self.ensure(0, self.size)
        }

        /// Prefetch exactly `[0, len)` (clamped to the size) into the cache —
        /// used to make just the header available so metadata can be parsed
        /// and decoding can begin without downloading the whole file. Unlike a
        /// normal read this adds no read-ahead, so the header fetch stays
        /// bounded. Leaves the read cursor at 0.
        pub fn prefetch(&mut self, len: u64) -> io::Result<()> {
            let end = len.min(self.size);
            if !self.contains(0, end) {
                self.fetch(0, end)?;
            }
            self.pos = 0;
            Ok(())
        }

        /// Make sure `[start, end)` is present in the cache, fetching the
        /// missing sub-spans via HTTP range requests.
        fn ensure(&mut self, start: u64, end: u64) -> io::Result<()> {
            let end = end.min(self.size);
            if start >= end {
                return Ok(());
            }
            if !self.ranges {
                // Server ignores Range — fetch the whole body once.
                if !self.contains(0, self.size) {
                    self.fetch(0, self.size)?;
                }
                return Ok(());
            }
            let mut cur = start;
            while cur < end {
                if let Some(gap_end) = self.first_gap(cur, end) {
                    // Fetch the gap plus some read-ahead in one request.
                    let fetch_end = (gap_end + READAHEAD).min(self.size).max(gap_end);
                    self.fetch(cur, fetch_end)?;
                    cur = fetch_end;
                } else {
                    break; // fully cached
                }
            }
            Ok(())
        }

        /// Download `[start, end)` and write it into the cache at `start`.
        fn fetch(&mut self, start: u64, end: u64) -> io::Result<()> {
            let end = end.min(self.size);
            if start >= end {
                return Ok(());
            }
            let mut req = self.client.get(&self.url);
            if self.ranges {
                req = req.header(
                    reqwest::header::RANGE,
                    format!("bytes={}-{}", start, end - 1),
                );
            }
            let mut resp = req
                .send()
                .map_err(to_io)?
                .error_for_status()
                .map_err(to_io)?;

            self.cache.seek(SeekFrom::Start(start))?;
            let mut written = start;
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = resp.read(&mut buf).map_err(to_io)?;
                if n == 0 {
                    break;
                }
                // Guard against a server that ignores the upper bound.
                let take = (n as u64).min(self.size.saturating_sub(written)) as usize;
                if take == 0 {
                    break;
                }
                self.cache.write_all(&buf[..take])?;
                written += take as u64;
            }
            self.cache.flush()?;
            self.add_have(start, written);
            Ok(())
        }

        fn contains(&self, start: u64, end: u64) -> bool {
            self.have.iter().any(|&(s, e)| s <= start && end <= e)
        }

        /// The end of the first uncached gap within `[from, limit)`, or `None`
        /// if the whole range is already cached.
        fn first_gap(&self, from: u64, limit: u64) -> Option<u64> {
            // Find the interval covering `from`, if any.
            for &(s, e) in &self.have {
                if s <= from && from < e {
                    if e >= limit {
                        return None; // covered to the end
                    }
                    // Cached up to `e`; the gap starts there. Return `limit`
                    // as the target end of what still needs fetching.
                    return Some(limit);
                }
            }
            Some(limit)
        }

        fn add_have(&mut self, start: u64, end: u64) {
            if start >= end {
                return;
            }
            self.have.push((start, end));
            self.have.sort_by_key(|&(s, _)| s);
            let mut merged: Vec<(u64, u64)> = Vec::with_capacity(self.have.len());
            for &(s, e) in &self.have {
                if let Some(last) = merged.last_mut() {
                    if s <= last.1 {
                        last.1 = last.1.max(e);
                        continue;
                    }
                }
                merged.push((s, e));
            }
            self.have = merged;
        }
    }

    impl Read for HttpSource {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.pos >= self.size || buf.is_empty() {
                return Ok(0);
            }
            let end = (self.pos + buf.len() as u64).min(self.size);
            self.ensure(self.pos, end)?;
            let n = (end - self.pos) as usize;
            self.cache.seek(SeekFrom::Start(self.pos))?;
            self.cache.read_exact(&mut buf[..n])?;
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for HttpSource {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            let new = match pos {
                SeekFrom::Start(o) => o as i128,
                SeekFrom::End(o) => self.size as i128 + o as i128,
                SeekFrom::Current(o) => self.pos as i128 + o as i128,
            };
            if new < 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "seek before start",
                ));
            }
            self.pos = (new as u64).min(self.size);
            Ok(self.pos)
        }
    }

    impl MediaSource for HttpSource {
        fn size(&self) -> u64 {
            self.size
        }
    }

    /// An unbounded, forward-only HTTP stream (e.g. internet radio) — no known
    /// length, no seeking. Implements [`Read`] by pulling from a live response
    /// body; the audio format is carried out-of-band (from the `Content-Type`
    /// header). When the server interleaves SHOUTcast/Icecast metadata
    /// (`icy-metaint`), those blocks are stripped out of the audio and the
    /// current `StreamTitle` is published to [`HttpStream::title`].
    pub struct HttpStream {
        resp: reqwest::blocking::Response,
        ext: String,
        station: IcyInfo,
        /// `icy-metaint`: audio bytes between metadata blocks (0 = none).
        metaint: usize,
        /// Audio bytes left before the next metadata block.
        until_meta: usize,
        /// Current `StreamTitle`, updated in place as songs change. Shared so
        /// the player can read it while the decode thread pulls audio.
        title: Arc<Mutex<Option<String>>>,
    }

    impl HttpStream {
        /// Open `url` as a live stream (a plain GET whose body is consumed
        /// forward-only).
        pub fn new(url: &str) -> io::Result<Self> {
            let client = build_client()?;
            let p = probe(&client, url)?;
            Ok(HttpStream::from_probe(p, url))
        }

        fn from_probe(p: Probe, url: &str) -> Self {
            let ext = stream_ext(&p.mime_ext, url);
            let metaint = p.icy_metaint.unwrap_or(0);
            HttpStream {
                resp: p.resp,
                ext,
                station: p.icy,
                metaint,
                until_meta: metaint,
                title: Arc::new(Mutex::new(None)),
            }
        }

        /// Format hint (extension without dot, e.g. `"mp3"`) for the decoder.
        pub fn format_ext(&self) -> &str {
            &self.ext
        }

        /// Station-level info (`icy-name` / `icy-genre` / `icy-br`).
        pub fn station(&self) -> &IcyInfo {
            &self.station
        }

        /// Handle to the live `StreamTitle`, updated as the stream plays.
        /// Clone it and read `*handle.lock()` to see the current song.
        pub fn title(&self) -> Arc<Mutex<Option<String>>> {
            Arc::clone(&self.title)
        }

        /// Read exactly `buf.len()` bytes of the raw body, or fail on EOF —
        /// used to pull a whole metadata block.
        fn read_body_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
            let mut got = 0;
            while got < buf.len() {
                let n = self.resp.read(&mut buf[got..])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "stream ended inside an ICY metadata block",
                    ));
                }
                got += n;
            }
            Ok(())
        }

        /// Consume one interleaved metadata block: a length byte (× 16) then
        /// that many bytes, from which `StreamTitle='…'` is parsed.
        fn consume_metadata(&mut self) -> io::Result<()> {
            let mut len_byte = [0u8; 1];
            self.read_body_exact(&mut len_byte)?;
            let len = len_byte[0] as usize * 16;
            if len == 0 {
                return Ok(()); // no change since the last block
            }
            let mut meta = vec![0u8; len];
            self.read_body_exact(&mut meta)?;
            if let Some(title) = parse_stream_title(&meta) {
                *self.title.lock().unwrap() = Some(title);
            }
            Ok(())
        }
    }

    impl Read for HttpStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.metaint == 0 {
                return self.resp.read(buf); // no in-band metadata
            }
            // At a metadata boundary, strip the block before more audio.
            if self.until_meta == 0 {
                self.consume_metadata()?;
                self.until_meta = self.metaint;
            }
            let cap = buf.len().min(self.until_meta);
            let n = self.resp.read(&mut buf[..cap])?;
            self.until_meta -= n;
            Ok(n)
        }
    }

    /// Parse `StreamTitle='…';` out of an ICY metadata block (NUL-padded,
    /// `key='value';` pairs). Returns the title, empty ones filtered out.
    fn parse_stream_title(meta: &[u8]) -> Option<String> {
        let text = String::from_utf8_lossy(meta);
        let rest = text.split("StreamTitle=").nth(1)?;
        let rest = rest.strip_prefix('\'').unwrap_or(rest);
        // Value ends at the first "';" (or a lone "'" / end).
        let end = rest
            .find("';")
            .or_else(|| rest.find('\''))
            .unwrap_or(rest.len());
        let title = rest[..end].trim().to_string();
        (!title.is_empty()).then_some(title)
    }

    /// A remote URL resolved to a seekable finite file, an unbounded live
    /// stream, or an adaptive (HLS/DASH) presentation. See [`open_remote`].
    pub enum Remote {
        /// A finite, seekable file (fetch into its cache, then decode it).
        File(HttpSource),
        /// An unbounded live stream (decode forward-only).
        Stream(HttpStream),
        /// An HLS or MPEG-DASH manifest resolved to its segment stream
        /// (decode forward-only).
        Adaptive(crate::adaptive::AdaptiveStream),
    }

    /// Probe `url` once and classify it. An HLS/DASH/playlist manifest (by
    /// `Content-Type` or extension) becomes [`Remote::Adaptive`] (plain
    /// playlists redirect to their first entry and are re-probed). Otherwise
    /// a server that reports a length is a seekable [`Remote::File`]; one
    /// that does not (chunked / ICY radio) is a [`Remote::Stream`] whose
    /// probe response body *is* the stream, so no second request is made.
    pub fn open_remote(url: &str) -> io::Result<Remote> {
        let client = build_client()?;
        let mut url = url.to_string();
        for _ in 0..4 {
            let p = probe(&client, &url)?;
            if crate::adaptive::looks_like_manifest(&url, p.content_type.as_deref()) {
                drop(p); // the probe body is not the media
                match crate::adaptive::open_manifest(&client, &url)? {
                    crate::adaptive::ManifestOutcome::Adaptive(s) => {
                        return Ok(Remote::Adaptive(s));
                    }
                    crate::adaptive::ManifestOutcome::Redirect(next) => {
                        url = next; // plain playlist / single-file DASH
                        continue;
                    }
                }
            }
            return match p.length {
                Some(size) => {
                    let suffix = p.mime_ext.clone().unwrap_or_else(|| url_suffix(&url));
                    Ok(Remote::File(HttpSource::build(
                        client, &url, size, p.ranges, &suffix,
                    )?))
                }
                None => Ok(Remote::Stream(HttpStream::from_probe(p, &url))),
            };
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "playlist redirects nested too deep",
        ))
    }

    fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
        io::Error::new(io::ErrorKind::Other, e.to_string())
    }

    /// Map a `Content-Type` header value to a cache-file extension (with dot).
    /// Parameters (`; charset=…`) and case are ignored. Returns `None` for
    /// unknown or generic types (`application/octet-stream`) so the caller can
    /// fall back to the URL extension.
    fn mime_to_ext(content_type: &str) -> Option<String> {
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        let ext = match mime.as_str() {
            "audio/flac" | "audio/x-flac" => "flac",
            "audio/mpeg" | "audio/mp3" | "audio/x-mp3" | "audio/mpeg3" => "mp3",
            "audio/opus" => "opus",
            "audio/ogg" | "application/ogg" | "audio/vorbis" | "audio/x-vorbis+ogg" => "ogg",
            "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
            "audio/aac" | "audio/aacp" | "audio/x-aac" => "aac",
            "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => "wav",
            "audio/aiff" | "audio/x-aiff" => "aiff",
            "audio/x-ms-wma" | "audio/wma" => "wma",
            "audio/x-musepack" | "audio/musepack" => "mpc",
            "audio/x-ape" | "audio/ape" | "audio/x-monkeys-audio" => "ape",
            "audio/x-wavpack" | "audio/wavpack" => "wv",
            "audio/basic" => "au",
            "audio/webm" | "video/webm" => "webm",
            _ => return None,
        };
        Some(format!(".{ext}"))
    }

    /// Extract a file extension (with dot) from a URL path so the temp cache
    /// carries it — some codecs are probed by extension.
    fn url_suffix(url: &str) -> String {
        let path = url
            .split(['?', '#'])
            .next()
            .unwrap_or(url)
            .trim_end_matches('/');
        let name = path.rsplit('/').next().unwrap_or("");
        match name.rsplit_once('.') {
            Some((_, ext)) if !ext.is_empty() && ext.len() <= 5 => format!(".{ext}"),
            _ => String::new(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn mime_maps_common_audio_types() {
            assert_eq!(mime_to_ext("audio/flac").as_deref(), Some(".flac"));
            assert_eq!(mime_to_ext("audio/mpeg").as_deref(), Some(".mp3"));
            // parameters + case are ignored
            assert_eq!(
                mime_to_ext("Audio/MPEG; charset=binary").as_deref(),
                Some(".mp3")
            );
            assert_eq!(mime_to_ext("audio/ogg").as_deref(), Some(".ogg"));
            assert_eq!(mime_to_ext("audio/x-flac").as_deref(), Some(".flac"));
            assert_eq!(mime_to_ext("audio/mp4").as_deref(), Some(".m4a"));
            // generic/unknown → fall back to URL extension
            assert_eq!(mime_to_ext("application/octet-stream"), None);
            assert_eq!(mime_to_ext("text/html"), None);
        }

        #[test]
        fn url_suffix_extraction() {
            assert_eq!(url_suffix("http://h/song.flac"), ".flac");
            assert_eq!(url_suffix("http://h/a/b.mp3?x=1#y"), ".mp3");
            assert_eq!(url_suffix("http://h/stream"), "");
            assert_eq!(url_suffix("http://h/"), "");
        }

        #[test]
        fn icy_stream_title_parsing() {
            // Typical block: NUL-padded, single-quoted, multiple keys.
            let block = b"StreamTitle='Daft Punk - Around the World';StreamUrl='';\0\0\0";
            assert_eq!(
                parse_stream_title(block).as_deref(),
                Some("Daft Punk - Around the World")
            );
            // Title only.
            assert_eq!(
                parse_stream_title(b"StreamTitle='Just A Song';").as_deref(),
                Some("Just A Song")
            );
            // Empty title → None.
            assert_eq!(parse_stream_title(b"StreamTitle='';"), None);
            // No StreamTitle key → None.
            assert_eq!(parse_stream_title(b"StreamUrl='http://x';"), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_detection() {
        assert!(is_url("http://example.com/a.flac"));
        assert!(is_url("https://example.com/a.flac"));
        assert!(!is_url("/music/a.flac"));
        assert!(!is_url("relative/a.mp3"));
        assert!(!is_url("file:///music/a.flac"));
    }

    #[test]
    fn file_source_reads_and_seeks() {
        let path = std::env::temp_dir().join(format!("rbsrc_{}.bin", std::process::id()));
        let data: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        std::fs::write(&path, &data).unwrap();

        let mut src = FileSource::open(&path).unwrap();
        assert_eq!(src.size(), 4096);
        let mut buf = [0u8; 16];
        src.seek(SeekFrom::Start(100)).unwrap();
        src.read_exact(&mut buf).unwrap();
        assert_eq!(&buf[..], &data[100..116]);

        let _ = std::fs::remove_file(&path);
    }
}
