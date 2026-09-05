//! HLS (HTTP Live Streaming, RFC 8216) playlist parsing.
//!
//! Two playlist kinds share the `.m3u8` syntax:
//!
//! - A **master** (multivariant) playlist lists variant streams
//!   (`#EXT-X-STREAM-INF`) and alternate renditions (`#EXT-X-MEDIA`).
//!   [`parse_master`] + [`select_media_playlist`] pick the best *audio*
//!   playlist URL out of it.
//! - A **media** playlist lists the actual segments (`#EXTINF`), an optional
//!   fMP4 init segment (`#EXT-X-MAP`), sub-resource byte ranges
//!   (`#EXT-X-BYTERANGE`), and the live/VOD markers (`#EXT-X-ENDLIST`,
//!   `#EXT-X-MEDIA-SEQUENCE`). [`parse_media`] models it for the segment
//!   fetch loop in [`super::AdaptiveStream`].

use super::url_join;

/// One variant stream from `#EXT-X-STREAM-INF`.
#[derive(Debug, Clone)]
pub struct Variant {
    /// Absolute media-playlist URL.
    pub uri: String,
    /// `BANDWIDTH` in bits/s (0 if absent).
    pub bandwidth: u64,
    /// `CODECS` entries, e.g. `["mp4a.40.2", "avc1.64001f"]`.
    pub codecs: Vec<String>,
    /// `AUDIO` rendition group id, if the variant references one.
    pub audio_group: Option<String>,
}

/// One `#EXT-X-MEDIA:TYPE=AUDIO` rendition.
#[derive(Debug, Clone)]
pub struct AudioRendition {
    pub group_id: String,
    /// Absolute playlist URL. `None` means the audio is muxed into the
    /// variant's own segments.
    pub uri: Option<String>,
    pub default: bool,
}

/// A parsed master playlist.
#[derive(Debug, Clone, Default)]
pub struct MasterPlaylist {
    pub variants: Vec<Variant>,
    pub audio: Vec<AudioRendition>,
}

/// One media segment.
#[derive(Debug, Clone)]
pub struct MediaSegment {
    /// Absolute segment URL.
    pub uri: String,
    /// `#EXTINF` duration in seconds.
    pub duration: f64,
    /// Absolute media sequence number (position in the live window).
    pub sequence: u64,
    /// `#EXT-X-BYTERANGE` resolved to `(offset, length)` within `uri`.
    pub byte_range: Option<(u64, u64)>,
    /// The `#EXT-X-MAP` init segment in effect for this segment.
    pub map: Option<InitMap>,
}

/// An `#EXT-X-MAP` init segment (fMP4 `moov`).
#[derive(Debug, Clone, PartialEq)]
pub struct InitMap {
    pub uri: String,
    pub byte_range: Option<(u64, u64)>,
}

/// A parsed media playlist.
#[derive(Debug, Clone, Default)]
pub struct MediaPlaylist {
    /// `#EXT-X-TARGETDURATION` in seconds (live reload pacing).
    pub target_duration: f64,
    /// Sequence number of the first segment (`#EXT-X-MEDIA-SEQUENCE`).
    pub media_sequence: u64,
    /// `#EXT-X-ENDLIST` present — VOD, no reloads.
    pub endlist: bool,
    /// An `#EXT-X-KEY` with `METHOD` other than `NONE` was seen (we don't
    /// decrypt; the caller reports a clear error).
    pub encrypted: bool,
    pub segments: Vec<MediaSegment>,
}

impl MediaPlaylist {
    /// Total duration (sum of segment durations) — meaningful for VOD.
    pub fn total_duration(&self) -> f64 {
        self.segments.iter().map(|s| s.duration).sum()
    }
}

/// Does this `#EXTM3U` text look like an HLS playlist at all (as opposed to a
/// plain M3U track list, which shares the extension)?
pub fn is_hls(text: &str) -> bool {
    text.contains("#EXT-X-STREAM-INF") || text.contains("#EXT-X-TARGETDURATION")
}

/// Is it a master playlist (variants) rather than a media playlist (segments)?
pub fn is_master(text: &str) -> bool {
    text.contains("#EXT-X-STREAM-INF")
}

/// Split an attribute list (`KEY=VALUE,KEY="quoted,value",…`) into pairs,
/// honouring quotes.
fn parse_attributes(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // key up to '='
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        let key = s[key_start..i].trim().to_string();
        if i >= bytes.len() {
            break;
        }
        i += 1; // '='
        let value;
        if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let v_start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            value = s[v_start..i].to_string();
            i += 1; // closing quote
                    // skip to next comma
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
        } else {
            let v_start = i;
            while i < bytes.len() && bytes[i] != b',' {
                i += 1;
            }
            value = s[v_start..i].trim().to_string();
        }
        if i < bytes.len() {
            i += 1; // ','
        }
        if !key.is_empty() {
            out.push((key, value));
        }
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

/// Parse a master playlist; URIs are resolved against `base_url`.
pub fn parse_master(text: &str, base_url: &str) -> MasterPlaylist {
    let mut master = MasterPlaylist::default();
    let mut pending: Option<Variant> = None;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            let attrs = parse_attributes(rest);
            pending = Some(Variant {
                uri: String::new(),
                bandwidth: attr(&attrs, "BANDWIDTH")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                codecs: attr(&attrs, "CODECS")
                    .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default(),
                audio_group: attr(&attrs, "AUDIO").map(str::to_string),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = parse_attributes(rest);
            if attr(&attrs, "TYPE").is_some_and(|t| t.eq_ignore_ascii_case("AUDIO")) {
                master.audio.push(AudioRendition {
                    group_id: attr(&attrs, "GROUP-ID").unwrap_or("").to_string(),
                    uri: attr(&attrs, "URI").map(|u| url_join(base_url, u)),
                    default: attr(&attrs, "DEFAULT").is_some_and(|d| d.eq_ignore_ascii_case("YES")),
                });
            }
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        // A URI line closes the pending #EXT-X-STREAM-INF.
        if let Some(mut v) = pending.take() {
            v.uri = url_join(base_url, line);
            master.variants.push(v);
        }
    }
    master
}

/// Is a `CODECS` entry a video codec?
fn is_video_codec(c: &str) -> bool {
    let c = c.to_ascii_lowercase();
    [
        "avc1", "avc3", "hvc1", "hev1", "vp09", "vp08", "av01", "mp4v",
    ]
    .iter()
    .any(|p| c.starts_with(p))
}

/// Pick the audio media-playlist URL out of a master playlist.
///
/// Preference order:
/// 1. Audio-only variants (no video codec in `CODECS`) — highest bandwidth
///    (best audio quality; there is no video to waste bytes on).
/// 2. Otherwise the *lowest*-bandwidth variant — its segments carry video we
///    demux away, so fetch as little as possible.
///
/// If the chosen variant references an `AUDIO` rendition group with its own
/// URI, that dedicated audio playlist wins over the muxed variant.
pub fn select_media_playlist(master: &MasterPlaylist) -> Option<String> {
    if master.variants.is_empty() {
        // No variants — an audio rendition alone may carry the stream.
        return master
            .audio
            .iter()
            .find(|a| a.default)
            .or(master.audio.first())
            .and_then(|a| a.uri.clone());
    }
    let audio_only: Vec<&Variant> = master
        .variants
        .iter()
        .filter(|v| !v.codecs.is_empty() && !v.codecs.iter().any(|c| is_video_codec(c)))
        .collect();
    let chosen = if !audio_only.is_empty() {
        audio_only
            .into_iter()
            .max_by_key(|v| v.bandwidth)
            .expect("non-empty")
    } else {
        master
            .variants
            .iter()
            .min_by_key(|v| v.bandwidth)
            .expect("non-empty")
    };
    if let Some(group) = &chosen.audio_group {
        let rendition = master
            .audio
            .iter()
            .filter(|a| &a.group_id == group && a.uri.is_some())
            .max_by_key(|a| a.default);
        if let Some(r) = rendition {
            return r.uri.clone();
        }
    }
    Some(chosen.uri.clone())
}

/// Parse `#EXT-X-BYTERANGE:<n>[@<o>]`. Without an explicit offset the range
/// starts where the previous one (on the same URI) ended — `prev_end` carries
/// that.
fn parse_byterange(v: &str, prev_end: u64) -> Option<(u64, u64)> {
    let (len_s, off_s) = match v.split_once('@') {
        Some((l, o)) => (l, Some(o)),
        None => (v, None),
    };
    let len: u64 = len_s.trim().parse().ok()?;
    let off: u64 = match off_s {
        Some(o) => o.trim().parse().ok()?,
        None => prev_end,
    };
    Some((off, len))
}

/// Parse a media playlist; URIs resolve against `base_url`.
pub fn parse_media(text: &str, base_url: &str) -> MediaPlaylist {
    let mut pl = MediaPlaylist::default();
    let mut pending_duration: Option<f64> = None;
    let mut pending_range: Option<(u64, u64)> = None;
    let mut current_map: Option<InitMap> = None;
    // End offset of the previous BYTERANGE, keyed implicitly by "the previous
    // segment line" (the spec only allows offset-less ranges to continue the
    // same resource).
    let mut prev_range_end: u64 = 0;

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            pl.target_duration = rest.trim().parse().unwrap_or(0.0);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            pl.media_sequence = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let secs = rest.split(',').next().unwrap_or("").trim();
            pending_duration = Some(secs.parse().unwrap_or(0.0));
        } else if let Some(rest) = line.strip_prefix("#EXT-X-BYTERANGE:") {
            pending_range = parse_byterange(rest.trim(), prev_range_end);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attrs = parse_attributes(rest);
            if let Some(uri) = attr(&attrs, "URI") {
                let byte_range = attr(&attrs, "BYTERANGE").and_then(|v| parse_byterange(v, 0));
                current_map = Some(InitMap {
                    uri: url_join(base_url, uri),
                    byte_range,
                });
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            let attrs = parse_attributes(rest);
            if attr(&attrs, "METHOD").is_some_and(|m| !m.eq_ignore_ascii_case("NONE")) {
                pl.encrypted = true;
            }
        } else if line.starts_with("#EXT-X-ENDLIST") {
            pl.endlist = true;
        } else if line.starts_with('#') {
            // Other tags (PROGRAM-DATE-TIME, DISCONTINUITY, …) don't affect
            // the fetch loop.
        } else {
            let sequence = pl.media_sequence + pl.segments.len() as u64;
            let byte_range = pending_range.take();
            if let Some((off, len)) = byte_range {
                prev_range_end = off + len;
            }
            pl.segments.push(MediaSegment {
                uri: url_join(base_url, line),
                duration: pending_duration.take().unwrap_or(0.0),
                sequence,
                byte_range,
                map: current_map.clone(),
            });
        }
    }
    pl
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://cdn.example.com/live/main.m3u8";

    #[test]
    fn attribute_list_handles_quotes_and_commas() {
        let attrs =
            parse_attributes(r#"BANDWIDTH=128000,CODECS="mp4a.40.2,avc1.64001f",AUDIO="aud1""#);
        assert_eq!(attr(&attrs, "BANDWIDTH"), Some("128000"));
        assert_eq!(attr(&attrs, "CODECS"), Some("mp4a.40.2,avc1.64001f"));
        assert_eq!(attr(&attrs, "AUDIO"), Some("aud1"));
    }

    #[test]
    fn master_parse_and_audio_only_selection() {
        let text = "\
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=64000,CODECS=\"mp4a.40.2\"
audio-64k.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=128000,CODECS=\"mp4a.40.2\"
audio-128k.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.64001f,mp4a.40.2\"
video.m3u8
";
        let m = parse_master(text, BASE);
        assert_eq!(m.variants.len(), 3);
        // Highest-bandwidth audio-only variant wins over any video variant.
        assert_eq!(
            select_media_playlist(&m).as_deref(),
            Some("https://cdn.example.com/live/audio-128k.m3u8")
        );
    }

    #[test]
    fn master_video_only_picks_lowest_bandwidth() {
        let text = "\
#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.64001f,mp4a.40.2\"
hi.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=300000,CODECS=\"avc1.42c00d,mp4a.40.2\"
lo.m3u8
";
        let m = parse_master(text, BASE);
        assert_eq!(
            select_media_playlist(&m).as_deref(),
            Some("https://cdn.example.com/live/lo.m3u8")
        );
    }

    #[test]
    fn master_audio_rendition_group_wins() {
        let text = "\
#EXTM3U
#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",DEFAULT=YES,URI=\"eng/audio.m3u8\"
#EXT-X-STREAM-INF:BANDWIDTH=900000,CODECS=\"avc1.64001f,mp4a.40.2\",AUDIO=\"aud\"
video.m3u8
";
        let m = parse_master(text, BASE);
        assert_eq!(
            select_media_playlist(&m).as_deref(),
            Some("https://cdn.example.com/live/eng/audio.m3u8")
        );
    }

    #[test]
    fn media_parse_vod() {
        let text = "\
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-TARGETDURATION:10
#EXT-X-MEDIA-SEQUENCE:0
#EXTINF:9.6,
seg0.aac
#EXTINF:10.0,
seg1.aac
#EXT-X-ENDLIST
";
        let pl = parse_media(text, BASE);
        assert!(pl.endlist);
        assert!(!pl.encrypted);
        assert_eq!(pl.target_duration, 10.0);
        assert_eq!(pl.segments.len(), 2);
        assert_eq!(pl.segments[0].sequence, 0);
        assert_eq!(pl.segments[1].sequence, 1);
        assert_eq!(pl.segments[0].uri, "https://cdn.example.com/live/seg0.aac");
        assert!((pl.total_duration() - 19.6).abs() < 1e-9);
    }

    #[test]
    fn media_parse_live_sequence_and_map() {
        let text = "\
#EXTM3U
#EXT-X-TARGETDURATION:6
#EXT-X-MEDIA-SEQUENCE:100
#EXT-X-MAP:URI=\"init.mp4\"
#EXTINF:6.0,
seg100.m4s
#EXTINF:6.0,
seg101.m4s
";
        let pl = parse_media(text, BASE);
        assert!(!pl.endlist);
        assert_eq!(pl.media_sequence, 100);
        assert_eq!(pl.segments[0].sequence, 100);
        assert_eq!(pl.segments[1].sequence, 101);
        let map = pl.segments[1].map.as_ref().unwrap();
        assert_eq!(map.uri, "https://cdn.example.com/live/init.mp4");
    }

    #[test]
    fn media_parse_byteranges_chain() {
        let text = "\
#EXTM3U
#EXT-X-TARGETDURATION:4
#EXTINF:4.0,
#EXT-X-BYTERANGE:1000@0
all.ts
#EXTINF:4.0,
#EXT-X-BYTERANGE:2000
all.ts
#EXT-X-ENDLIST
";
        let pl = parse_media(text, BASE);
        assert_eq!(pl.segments[0].byte_range, Some((0, 1000)));
        // Offset-less range continues where the previous one ended.
        assert_eq!(pl.segments[1].byte_range, Some((1000, 2000)));
    }

    #[test]
    fn media_detects_encryption() {
        let text = "\
#EXTM3U
#EXT-X-TARGETDURATION:6
#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"
#EXTINF:6.0,
seg.ts
";
        assert!(parse_media(text, BASE).encrypted);
        let none = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:6,\ns.ts\n";
        assert!(!parse_media(none, BASE).encrypted);
    }

    #[test]
    fn hls_vs_plain_m3u_detection() {
        assert!(is_hls("#EXTM3U\n#EXT-X-TARGETDURATION:10\n"));
        assert!(is_hls("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nx.m3u8\n"));
        assert!(!is_hls("#EXTM3U\n#EXTINF:212,Song\n/music/a.flac\n"));
        assert!(is_master("#EXT-X-STREAM-INF:BANDWIDTH=1\nx\n"));
        assert!(!is_master("#EXT-X-TARGETDURATION:10\n"));
    }
}
