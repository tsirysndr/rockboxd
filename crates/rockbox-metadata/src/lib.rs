//! Audio file metadata / tag parsing extracted from
//! [Rockbox](https://www.rockbox.org) as a reusable library.
//!
//! One call parses tags, duration, bitrate, sample rate, ReplayGain and
//! embedded album-art / cuesheet positions for 40+ audio formats — MP3
//! (ID3v1/v2), FLAC, Ogg Vorbis, Opus, Speex, MP4/M4A (AAC & ALAC),
//! WavPack, Monkey's Audio, Musepack, WMA, AIFF, WAV/AU/VOX, TTA, ADX,
//! SHN, and the chiptune family (SPC, NSF, SID, VGM, KSS, AY, GBS, HES,
//! SGC, VTX, MOD, SAP, …).
//!
//! ```no_run
//! let meta = rockbox_metadata::read("song.flac").unwrap();
//! println!(
//!     "{} — {} [{}] {:?}",
//!     meta.artist, meta.title, meta.codec, meta.duration,
//! );
//! ```
//!
//! The parsers are the battle-tested Rockbox C sources
//! (`lib/rbcodec/metadata/`), compiled by `build.rs` and bridged through a
//! small flat struct (`shim/rbmeta.h`). Pairs with the
//! [`rockbox-dsp`](https://crates.io/crates/rockbox-dsp) crate: the raw
//! Q7.24 ReplayGain values from [`ReplayGain`] feed directly into its
//! `set_replaygain_gains_raw()`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

const STR: usize = 512;
const COMMENT: usize = 1024;
const GENRE: usize = 256;
const SMALL: usize = 64;
const CODEC: usize = 32;

/// Mirror of `struct rbmeta_tags` in `shim/rbmeta.h`.
#[repr(C)]
struct RawTags {
    codec: [u8; CODEC],
    title: [u8; STR],
    artist: [u8; STR],
    album: [u8; STR],
    albumartist: [u8; STR],
    composer: [u8; STR],
    grouping: [u8; STR],
    comment: [u8; COMMENT],
    genre: [u8; GENRE],
    year_string: [u8; SMALL],
    track_string: [u8; SMALL],
    disc_string: [u8; SMALL],
    mb_track_id: [u8; SMALL],

    codectype: i32,
    tracknum: i32,
    discnum: i32,
    year: i32,
    layer: i32,
    id3version: i32,
    vbr: i32,
    bitrate: i32,

    frequency: u32,
    reserved_: u32,

    filesize: u64,
    length_ms: u64,
    samples: u64,
    frame_count: u64,
    first_frame_offset: u64,

    track_level: i64,
    album_level: i64,
    track_gain: i64,
    album_gain: i64,
    track_peak: i64,
    album_peak: i64,

    has_albumart: i32,
    albumart_type: i32,
    albumart_pos: i64,
    albumart_size: i32,

    has_cuesheet: i32,
    cuesheet_pos: i64,
    cuesheet_size: i32,
    cuesheet_encoding: i32,
}

extern "C" {
    fn rbmeta_read(path: *const c_char, out: *mut RawTags) -> c_int;
    fn rbmeta_codec_label(codectype: c_uint) -> *const c_char;
    fn probe_file_format(filename: *const c_char) -> c_uint;
}

/// Some Rockbox parsers keep static scratch state; serialize all calls.
static META_LOCK: Mutex<()> = Mutex::new(());

/// ReplayGain values parsed from the file's tags.
///
/// The `*_db` / `*_peak` fields are user-friendly conversions; the `raw_*`
/// fields keep Rockbox's native fixed-point encoding — linear scale
/// factors in Q7.24, exactly what `rockbox-dsp`'s
/// `set_replaygain_gains_raw()` expects.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ReplayGain {
    /// Track gain in dB (e.g. `-6.5`), if the tag is present.
    pub track_gain_db: Option<f32>,
    /// Album gain in dB, if the tag is present.
    pub album_gain_db: Option<f32>,
    /// Track peak amplitude, 1.0 = full scale.
    pub track_peak: Option<f32>,
    /// Album peak amplitude, 1.0 = full scale.
    pub album_peak: Option<f32>,
    /// Track gain as a linear Q7.24 scale factor (0 = tag absent).
    pub raw_track_gain: i64,
    /// Album gain as a linear Q7.24 scale factor (0 = tag absent).
    pub raw_album_gain: i64,
    /// Track peak in Q7.24 (0 = tag absent).
    pub raw_track_peak: i64,
    /// Album peak in Q7.24 (0 = tag absent).
    pub raw_album_peak: i64,
}

impl ReplayGain {
    /// True if no ReplayGain information was found at all.
    pub fn is_empty(&self) -> bool {
        self.raw_track_gain == 0
            && self.raw_album_gain == 0
            && self.raw_track_peak == 0
            && self.raw_album_peak == 0
    }
}

/// Image format of embedded album art.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumArtKind {
    Bmp,
    Png,
    Jpeg,
    Unknown,
}

/// Location of album art embedded in the audio file (ID3 APIC, FLAC
/// PICTURE, MP4 `covr`). Read `size` bytes at byte `offset` to extract the
/// image — unless one of the transform flags is set, in which case the
/// data first needs ID3 de-unsynchronization or base64 decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlbumArt {
    pub kind: AlbumArtKind,
    /// Byte offset of the image data within the file.
    pub offset: u64,
    /// Image data size in bytes.
    pub size: u32,
    /// Data is inside an unsynchronized ID3v2 frame (0xFF 0x00 stuffing
    /// must be removed).
    pub id3_unsync: bool,
    /// Data is base64-encoded (Vorbis `METADATA_BLOCK_PICTURE`).
    pub vorbis_base64: bool,
}

/// Character encoding of an embedded cuesheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CueEncoding {
    Latin1,
    Utf8,
    Utf16Le,
    Utf16Be,
    Unknown,
}

/// Location of a cuesheet embedded in the file's tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cuesheet {
    pub offset: u64,
    pub size: u32,
    pub encoding: CueEncoding,
}

/// Parsed metadata for one audio file.
#[derive(Debug, Clone, Default)]
pub struct Metadata {
    /// Codec label, e.g. `"MP3"`, `"FLAC"`, `"Ogg"`, `"AAC"`.
    pub codec: String,
    /// Rockbox `AFMT_*` codec index (stable across releases).
    pub codec_id: u32,

    /// Tag strings — empty when the tag is absent.
    pub title: String,
    pub artist: String,
    pub album: String,
    pub albumartist: String,
    pub composer: String,
    pub grouping: String,
    pub comment: String,
    pub genre: String,
    /// Raw year / track / disc strings as found in the tags.
    pub year_string: String,
    pub track_string: String,
    pub disc_string: String,
    /// MusicBrainz track ID — empty when untagged.
    pub mb_track_id: String,

    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,

    /// Track length.
    pub duration: Duration,
    /// Average bitrate in kbit/s.
    pub bitrate: u32,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Audio payload size in bytes (excluding tag headers).
    pub filesize: u64,
    /// Total PCM frames, when the container declares it (0 otherwise).
    pub samples: u64,
    /// Whether the stream is variable bitrate.
    pub vbr: bool,
    /// Byte offset of the first audio frame (after leading tags).
    pub first_frame_offset: u64,

    pub replaygain: ReplayGain,
    pub album_art: Option<AlbumArt>,
    pub cuesheet: Option<Cuesheet>,
}

/// Errors returned by [`read`].
#[derive(Debug)]
pub enum Error {
    /// The file could not be opened.
    Open(std::path::PathBuf),
    /// No parser recognized the file, or parsing failed.
    Parse(std::path::PathBuf),
    /// The path contains an interior NUL byte.
    InvalidPath,
    /// Out of memory in the C bridge.
    OutOfMemory,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Open(p) => write!(f, "cannot open {}", p.display()),
            Error::Parse(p) => write!(f, "unrecognized or corrupt audio file {}", p.display()),
            Error::InvalidPath => write!(f, "path contains a NUL byte"),
            Error::OutOfMemory => write!(f, "out of memory"),
        }
    }
}

impl std::error::Error for Error {}

fn cstr_field(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

/// Q19.12 dB value → f32 dB.
fn q12_db(v: i64) -> f32 {
    v as f32 / 4096.0
}

/// Q7.24 linear value → f32.
fn q24(v: i64) -> f32 {
    v as f32 / (1u32 << 24) as f32
}

impl Metadata {
    fn from_raw(raw: &RawTags) -> Self {
        let opt_u32 = |v: i32| if v > 0 { Some(v as u32) } else { None };

        Metadata {
            codec: cstr_field(&raw.codec),
            codec_id: raw.codectype as u32,
            title: cstr_field(&raw.title),
            artist: cstr_field(&raw.artist),
            album: cstr_field(&raw.album),
            albumartist: cstr_field(&raw.albumartist),
            composer: cstr_field(&raw.composer),
            grouping: cstr_field(&raw.grouping),
            comment: cstr_field(&raw.comment),
            genre: cstr_field(&raw.genre),
            year_string: cstr_field(&raw.year_string),
            track_string: cstr_field(&raw.track_string),
            disc_string: cstr_field(&raw.disc_string),
            mb_track_id: cstr_field(&raw.mb_track_id),
            track_number: opt_u32(raw.tracknum),
            disc_number: opt_u32(raw.discnum),
            year: opt_u32(raw.year),
            duration: Duration::from_millis(raw.length_ms),
            bitrate: raw.bitrate.max(0) as u32,
            sample_rate: raw.frequency,
            filesize: raw.filesize,
            samples: raw.samples,
            vbr: raw.vbr != 0,
            first_frame_offset: raw.first_frame_offset,
            replaygain: ReplayGain {
                track_gain_db: (raw.track_gain != 0).then(|| q12_db(raw.track_level)),
                album_gain_db: (raw.album_gain != 0).then(|| q12_db(raw.album_level)),
                track_peak: (raw.track_peak != 0).then(|| q24(raw.track_peak)),
                album_peak: (raw.album_peak != 0).then(|| q24(raw.album_peak)),
                raw_track_gain: raw.track_gain,
                raw_album_gain: raw.album_gain,
                raw_track_peak: raw.track_peak,
                raw_album_peak: raw.album_peak,
            },
            album_art: (raw.has_albumart != 0 && raw.albumart_size > 0).then(|| AlbumArt {
                kind: match raw.albumart_type & 0xF {
                    1 => AlbumArtKind::Bmp,
                    2 => AlbumArtKind::Png,
                    3 => AlbumArtKind::Jpeg,
                    _ => AlbumArtKind::Unknown,
                },
                offset: raw.albumart_pos.max(0) as u64,
                size: raw.albumart_size as u32,
                id3_unsync: raw.albumart_type & (1 << 4) != 0,
                vorbis_base64: raw.albumart_type & (1 << 5) != 0,
            }),
            cuesheet: (raw.has_cuesheet != 0 && raw.cuesheet_size > 0).then(|| Cuesheet {
                offset: raw.cuesheet_pos.max(0) as u64,
                size: raw.cuesheet_size as u32,
                encoding: match raw.cuesheet_encoding {
                    1 => CueEncoding::Latin1,
                    2 => CueEncoding::Utf8,
                    3 => CueEncoding::Utf16Le,
                    4 => CueEncoding::Utf16Be,
                    _ => CueEncoding::Unknown,
                },
            }),
        }
    }
}

/// Parse the metadata of the audio file at `path`.
pub fn read<P: AsRef<Path>>(path: P) -> Result<Metadata, Error> {
    let path = path.as_ref();
    let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| Error::InvalidPath)?;

    // ~6 KB — heap-allocate and let the C side zero it.
    let mut raw: Box<RawTags> = unsafe { Box::new(std::mem::zeroed()) };

    let rc = {
        let _guard = META_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        unsafe { rbmeta_read(c_path.as_ptr(), &mut *raw) }
    };

    match rc {
        0 => Ok(Metadata::from_raw(&raw)),
        -1 => Err(Error::Open(path.to_path_buf())),
        -3 => Err(Error::OutOfMemory),
        _ => Err(Error::Parse(path.to_path_buf())),
    }
}

/// Guess the codec from a filename's extension without opening the file.
/// Returns the format label (e.g. `"FLAC"`) or `None` if unknown.
pub fn probe(filename: &str) -> Option<String> {
    let c = CString::new(filename).ok()?;
    unsafe {
        let afmt = probe_file_format(c.as_ptr());
        let label = rbmeta_codec_label(afmt);
        if afmt == 0 || label.is_null() {
            return None;
        }
        Some(CStr::from_ptr(label).to_string_lossy().into_owned())
    }
}
