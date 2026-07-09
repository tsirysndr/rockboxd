//! Flat C ABI over [`rockbox_metadata`]. Rich metadata is returned as a
//! JSON C string (the pragmatic lowest common denominator across every
//! binding language); the caller frees it with `rb_string_free`.

use crate::util::{cstr, into_cstring};
use rockbox_metadata::{AlbumArt, AlbumArtKind, CueEncoding, Cuesheet, Metadata, ReplayGain};
use serde::Serialize;
use std::os::raw::c_char;

#[derive(Serialize)]
struct ReplayGainJson {
    track_gain_db: Option<f32>,
    album_gain_db: Option<f32>,
    track_peak: Option<f32>,
    album_peak: Option<f32>,
    raw_track_gain: i64,
    raw_album_gain: i64,
    raw_track_peak: i64,
    raw_album_peak: i64,
}

impl From<&ReplayGain> for ReplayGainJson {
    fn from(r: &ReplayGain) -> Self {
        ReplayGainJson {
            track_gain_db: r.track_gain_db,
            album_gain_db: r.album_gain_db,
            track_peak: r.track_peak,
            album_peak: r.album_peak,
            raw_track_gain: r.raw_track_gain,
            raw_album_gain: r.raw_album_gain,
            raw_track_peak: r.raw_track_peak,
            raw_album_peak: r.raw_album_peak,
        }
    }
}

#[derive(Serialize)]
struct AlbumArtJson {
    kind: &'static str,
    offset: u64,
    size: u32,
    id3_unsync: bool,
    vorbis_base64: bool,
}

impl From<&AlbumArt> for AlbumArtJson {
    fn from(a: &AlbumArt) -> Self {
        AlbumArtJson {
            kind: match a.kind {
                AlbumArtKind::Bmp => "bmp",
                AlbumArtKind::Png => "png",
                AlbumArtKind::Jpeg => "jpeg",
                AlbumArtKind::Unknown => "unknown",
            },
            offset: a.offset,
            size: a.size,
            id3_unsync: a.id3_unsync,
            vorbis_base64: a.vorbis_base64,
        }
    }
}

#[derive(Serialize)]
struct CuesheetJson {
    offset: u64,
    size: u32,
    encoding: &'static str,
}

impl From<&Cuesheet> for CuesheetJson {
    fn from(c: &Cuesheet) -> Self {
        CuesheetJson {
            offset: c.offset,
            size: c.size,
            encoding: match c.encoding {
                CueEncoding::Latin1 => "latin1",
                CueEncoding::Utf8 => "utf8",
                CueEncoding::Utf16Le => "utf16le",
                CueEncoding::Utf16Be => "utf16be",
                CueEncoding::Unknown => "unknown",
            },
        }
    }
}

#[derive(Serialize)]
pub(crate) struct MetadataJson {
    codec: String,
    codec_id: u32,
    title: String,
    artist: String,
    album: String,
    albumartist: String,
    composer: String,
    grouping: String,
    comment: String,
    genre: String,
    year_string: String,
    track_string: String,
    disc_string: String,
    mb_track_id: String,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    year: Option<u32>,
    duration_ms: u64,
    bitrate: u32,
    sample_rate: u32,
    filesize: u64,
    samples: u64,
    vbr: bool,
    first_frame_offset: u64,
    replaygain: ReplayGainJson,
    album_art: Option<AlbumArtJson>,
    cuesheet: Option<CuesheetJson>,
}

impl From<&Metadata> for MetadataJson {
    fn from(m: &Metadata) -> Self {
        MetadataJson {
            codec: m.codec.clone(),
            codec_id: m.codec_id,
            title: m.title.clone(),
            artist: m.artist.clone(),
            album: m.album.clone(),
            albumartist: m.albumartist.clone(),
            composer: m.composer.clone(),
            grouping: m.grouping.clone(),
            comment: m.comment.clone(),
            genre: m.genre.clone(),
            year_string: m.year_string.clone(),
            track_string: m.track_string.clone(),
            disc_string: m.disc_string.clone(),
            mb_track_id: m.mb_track_id.clone(),
            track_number: m.track_number,
            disc_number: m.disc_number,
            year: m.year,
            duration_ms: m.duration.as_millis() as u64,
            bitrate: m.bitrate,
            sample_rate: m.sample_rate,
            filesize: m.filesize,
            samples: m.samples,
            vbr: m.vbr,
            first_frame_offset: m.first_frame_offset,
            replaygain: (&m.replaygain).into(),
            album_art: m.album_art.as_ref().map(Into::into),
            cuesheet: m.cuesheet.as_ref().map(Into::into),
        }
    }
}

/// Parse the audio file at `path` and return its metadata as a JSON string
/// the caller must free with `rb_string_free`. Returns null if the path is
/// null/invalid, the file cannot be opened, or no parser recognises it.
#[no_mangle]
pub extern "C" fn rb_meta_read_json(path: *const c_char) -> *mut c_char {
    let Some(path) = cstr(path) else {
        return std::ptr::null_mut();
    };
    match rockbox_metadata::read(path) {
        Ok(meta) => {
            let json = MetadataJson::from(&meta);
            match serde_json::to_string(&json) {
                Ok(s) => into_cstring(s),
                Err(_) => std::ptr::null_mut(),
            }
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Guess the codec label (e.g. `"FLAC"`) from a filename's extension
/// without opening the file. Returns a string to free with `rb_string_free`,
/// or null if the extension is unknown.
#[no_mangle]
pub extern "C" fn rb_meta_probe(filename: *const c_char) -> *mut c_char {
    let Some(filename) = cstr(filename) else {
        return std::ptr::null_mut();
    };
    match rockbox_metadata::probe(filename) {
        Some(label) => into_cstring(label),
        None => std::ptr::null_mut(),
    }
}
