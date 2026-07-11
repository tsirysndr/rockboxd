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
pub use http::HttpSource;

#[cfg(feature = "http")]
mod http {
    use super::*;
    use std::fs::File;
    use std::io::Write;

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

    impl HttpSource {
        /// Probe `url` (size + range support) and create the cache. Does not
        /// download any media body yet.
        pub fn new(url: &str) -> io::Result<Self> {
            let client = reqwest::blocking::Client::builder()
                .build()
                .map_err(to_io)?;

            // A 1-byte ranged GET tells us the total size (`Content-Range`)
            // and whether ranges are honoured (206 vs 200), cheaply.
            let resp = client
                .get(url)
                .header(reqwest::header::RANGE, "bytes=0-0")
                .send()
                .map_err(to_io)?;
            let status = resp.status();

            let (size, ranges) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
                let total = resp
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.rsplit('/').next())
                    .and_then(|s| s.trim().parse::<u64>().ok());
                (total, true)
            } else if status.is_success() {
                let len = resp.content_length();
                (len, false)
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("HTTP {status} for {url}"),
                ));
            };

            let size = size.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "server did not report a content length",
                )
            })?;

            // Prefer the server's declared MIME type for format detection; the
            // codec/metadata layer is probed partly by file extension, and a
            // URL may carry a misleading or absent one (e.g. `/stream?id=42`).
            let mime_ext = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .and_then(mime_to_ext);
            let suffix = mime_ext.unwrap_or_else(|| url_suffix(url));
            let named = tempfile::Builder::new()
                .prefix("rbstream_")
                .suffix(&suffix)
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
