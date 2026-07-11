//! Audio decoding with [Rockbox](https://www.rockbox.org)'s codecs as a
//! reusable library.
//!
//! The actual decoders are the Rockbox firmware codecs (`lib/rbcodec/codecs/`
//! plus their vendored decoder libraries — libmad, libtremor,
//! libffmpegFLAC, libfaad, …), compiled by `build.rs` and driven through
//! Rockbox's `codec_api` by a small C shim modeled on the upstream
//! `warble` test player.
//!
//! ```no_run
//! let mut dec = rockbox_codecs::Decoder::open("song.flac").unwrap();
//! println!("{} — {}", dec.metadata().artist, dec.metadata().title);
//! while let Some(chunk) = dec.next_chunk() {
//!     // chunk.pcm: interleaved stereo i16 at chunk.sample_rate Hz
//! }
//! ```
//!
//! All 29 codecs of Rockbox's static-link set are feature-gated (`flac`,
//! `mpa`, `vorbis`, `alac`, `aac`, `opus`, `wma`, `ape`, …) so you only
//! compile the decoders you need.
//!
//! # Concurrency
//!
//! Rockbox codecs share one global `codec_api` instance, so **one decode
//! session runs at a time**: `Decoder::open` blocks until any other live
//! `Decoder` is dropped. Within a session, decoding runs on a dedicated
//! thread and PCM is pulled through a bounded channel.

use std::ffi::{c_char, c_void, CString};
use std::path::Path;
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub use rockbox_metadata::Metadata;

extern "C" {
    fn rbcodec_set_sink(
        cb: Option<unsafe extern "C" fn(*mut c_void, *const i16, usize, std::ffi::c_ulong)>,
        user: *mut c_void,
    );
    fn rbcodec_open(path: *const c_char) -> i32;
    fn rbcodec_open_stream(
        read_cb: Option<
            unsafe extern "C" fn(*mut c_void, *mut c_void, std::ffi::c_ulong) -> std::ffi::c_long,
        >,
        user: *mut c_void,
        ext: *const c_char,
    ) -> i32;
    fn rbcodec_open_seekable(
        read_cb: Option<
            unsafe extern "C" fn(*mut c_void, *mut c_void, std::ffi::c_ulong) -> std::ffi::c_long,
        >,
        seek_cb: Option<
            unsafe extern "C" fn(*mut c_void, i64) -> std::ffi::c_int,
        >,
        user: *mut c_void,
        size: i64,
        frequency: std::ffi::c_ulong,
        ext: *const c_char,
    ) -> i32;
    fn rbcodec_run() -> i32;
    fn rbcodec_close();
    fn rbcodec_request_seek(time_ms: std::ffi::c_long);
    fn rbcodec_request_halt();
    fn rbcodec_elapsed_ms() -> std::ffi::c_ulong;
}

/// One buffer of decoded audio: interleaved stereo `i16` frames.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Interleaved stereo samples (`pcm.len() == frames * 2`).
    pub pcm: Vec<i16>,
    /// Sample rate the codec is currently producing, in Hz.
    pub sample_rate: u32,
}

/// Errors returned by [`Decoder::open`].
#[derive(Debug)]
pub enum Error {
    /// The file could not be opened.
    Open(std::path::PathBuf),
    /// Metadata parsing failed — unrecognized or corrupt file.
    Parse(std::path::PathBuf),
    /// The file's format was recognized but its codec isn't compiled in
    /// (enable the matching cargo feature).
    CodecNotAvailable(String),
    /// The codec failed to initialize.
    CodecInit(std::path::PathBuf),
    /// The path contains an interior NUL byte.
    InvalidPath,
    /// Out of memory.
    OutOfMemory,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Open(p) => write!(f, "cannot open {}", p.display()),
            Error::Parse(p) => write!(f, "unrecognized or corrupt audio file {}", p.display()),
            Error::CodecNotAvailable(c) => write!(
                f,
                "no decoder for {c} compiled in (enable the matching cargo feature)"
            ),
            Error::CodecInit(p) => write!(f, "codec failed to initialize for {}", p.display()),
            Error::InvalidPath => write!(f, "path contains a NUL byte"),
            Error::OutOfMemory => write!(f, "out of memory"),
        }
    }
}

impl std::error::Error for Error {}

/// The codec_api state is global — serialize sessions.
struct Gate {
    busy: Mutex<bool>,
    cv: Condvar,
}

static GATE: Gate = Gate {
    busy: Mutex::new(false),
    cv: Condvar::new(),
};

impl Gate {
    fn acquire(&self) {
        let mut busy = self.busy.lock().unwrap_or_else(|e| e.into_inner());
        while *busy {
            busy = self.cv.wait(busy).unwrap_or_else(|e| e.into_inner());
        }
        *busy = true;
    }

    fn release(&self) {
        *self.busy.lock().unwrap_or_else(|e| e.into_inner()) = false;
        self.cv.notify_one();
    }
}

enum Msg {
    Pcm(Chunk),
    Done(i32),
}

struct SinkCtx {
    tx: SyncSender<Msg>,
}

/// Holds the byte reader for a streaming decode session; its address is the
/// C `user` pointer passed to `rbcodec_open_stream`.
struct ReaderCtx {
    reader: Box<dyn std::io::Read + Send>,
}

/// Stream-read trampoline: pulls bytes from the Rust reader on the decode
/// thread (blocking). Returns bytes read, 0 at end of stream, or -1 on error.
unsafe extern "C" fn stream_read_trampoline(
    user: *mut c_void,
    buf: *mut c_void,
    len: std::ffi::c_ulong,
) -> std::ffi::c_long {
    let ctx = &mut *(user as *mut ReaderCtx);
    let slice = std::slice::from_raw_parts_mut(buf as *mut u8, len as usize);
    match ctx.reader.read(slice) {
        Ok(n) => n as std::ffi::c_long,
        Err(_) => -1,
    }
}

/// A seekable byte source for a streaming decode session (e.g. an HTTP file
/// buffered on demand via range requests).
pub trait SeekableSource: std::io::Read + std::io::Seek + Send {}
impl<T: std::io::Read + std::io::Seek + Send> SeekableSource for T {}

/// Holds a seekable source; its address is the C `user` pointer passed to
/// `rbcodec_open_seekable`.
struct SeekCtx {
    source: Box<dyn SeekableSource>,
}

unsafe extern "C" fn seek_read_trampoline(
    user: *mut c_void,
    buf: *mut c_void,
    len: std::ffi::c_ulong,
) -> std::ffi::c_long {
    let ctx = &mut *(user as *mut SeekCtx);
    let slice = std::slice::from_raw_parts_mut(buf as *mut u8, len as usize);
    match ctx.source.read(slice) {
        Ok(n) => n as std::ffi::c_long,
        Err(_) => -1,
    }
}

/// Seek trampoline: absolute byte seek. Returns 0 on success, -1 on error.
unsafe extern "C" fn seek_seek_trampoline(user: *mut c_void, pos: i64) -> std::ffi::c_int {
    let ctx = &mut *(user as *mut SeekCtx);
    match ctx.source.seek(std::io::SeekFrom::Start(pos.max(0) as u64)) {
        Ok(_) => 0,
        Err(_) => -1,
    }
}

/// Sink trampoline, called on the decode thread for every burst of PCM
/// the codec produces. A blocking send provides backpressure; when the
/// receiver is gone (Decoder dropped mid-track) we request a halt and
/// drop samples so the codec reaches its next command poll.
unsafe extern "C" fn sink_trampoline(
    user: *mut c_void,
    pcm: *const i16,
    frames: usize,
    frequency: std::ffi::c_ulong,
) {
    let ctx = &*(user as *const SinkCtx);
    let samples = std::slice::from_raw_parts(pcm, frames * 2);
    let chunk = Chunk {
        pcm: samples.to_vec(),
        sample_rate: frequency as u32,
    };
    if ctx.tx.send(Msg::Pcm(chunk)).is_err() {
        rbcodec_request_halt();
    }
}

/// A running decode session for one audio file.
pub struct Decoder {
    rx: Receiver<Msg>,
    thread: Option<std::thread::JoinHandle<()>>,
    /// Kept alive for the C sink's `user` pointer.
    _sink: Box<SinkCtx>,
    /// Kept alive for the C stream-read `user` pointer (streaming only).
    _reader: Option<Box<ReaderCtx>>,
    /// Kept alive for the C seekable-source `user` pointer (HTTP files).
    _seek: Option<Box<SeekCtx>>,
    meta: Metadata,
    status: Option<i32>,
}

impl Decoder {
    /// Open `path` and start decoding. Blocks while another `Decoder`
    /// exists anywhere in the process (the codec state is global).
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref();
        let c_path =
            CString::new(path.to_string_lossy().as_bytes()).map_err(|_| Error::InvalidPath)?;

        // Parse tags up-front with the sibling crate (cheap second pass).
        let meta = rockbox_metadata::read(path).map_err(|e| match e {
            rockbox_metadata::Error::Open(p) => Error::Open(p),
            rockbox_metadata::Error::OutOfMemory => Error::OutOfMemory,
            _ => Error::Parse(path.to_path_buf()),
        })?;

        GATE.acquire();

        let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(8);
        let sink = Box::new(SinkCtx { tx: tx.clone() });

        let rc = unsafe {
            rbcodec_set_sink(
                Some(sink_trampoline),
                &*sink as *const SinkCtx as *mut c_void,
            );
            rbcodec_open(c_path.as_ptr())
        };
        if rc != 0 {
            GATE.release();
            return Err(match rc {
                -1 => Error::Open(path.to_path_buf()),
                -2 => Error::Parse(path.to_path_buf()),
                -3 => Error::CodecNotAvailable(meta.codec.clone()),
                -6 => Error::OutOfMemory,
                _ => Error::CodecInit(path.to_path_buf()),
            });
        }

        let thread = std::thread::Builder::new()
            .name("rbcodec".into())
            .spawn(move || {
                let status = unsafe { rbcodec_run() };
                let _ = tx.send(Msg::Done(status));
            })
            .expect("spawn codec thread");

        Ok(Decoder {
            rx,
            thread: Some(thread),
            _sink: sink,
            _reader: None,
            _seek: None,
            meta,
            status: None,
        })
    }

    /// Open an unbounded, forward-only byte stream (e.g. internet radio) and
    /// start decoding. Unlike [`Decoder::open`], there is no seekable file:
    /// `reader` is pulled forward on the decode thread (blocking), and the
    /// codec is chosen from `format_ext` (`"mp3"`, `"ogg"`, `"aac"`, …) since
    /// there is no file to sniff. Only self-describing streamable formats
    /// work; `meta` supplies whatever is known up front (e.g. codec label),
    /// with duration left `0` (unknown).
    ///
    /// Blocks while another `Decoder` exists (the codec state is global).
    pub fn open_stream(
        reader: Box<dyn std::io::Read + Send>,
        format_ext: &str,
        meta: Metadata,
    ) -> Result<Self, Error> {
        let c_ext = CString::new(format_ext).map_err(|_| Error::InvalidPath)?;

        GATE.acquire();

        let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(8);
        let sink = Box::new(SinkCtx { tx: tx.clone() });
        let reader = Box::new(ReaderCtx { reader });

        let rc = unsafe {
            rbcodec_set_sink(Some(sink_trampoline), &*sink as *const SinkCtx as *mut c_void);
            rbcodec_open_stream(
                Some(stream_read_trampoline),
                &*reader as *const ReaderCtx as *mut c_void,
                c_ext.as_ptr(),
            )
        };
        if rc != 0 {
            GATE.release();
            return Err(match rc {
                -3 => Error::CodecNotAvailable(meta.codec.clone()),
                _ => Error::CodecInit(std::path::PathBuf::from(format_ext)),
            });
        }

        let thread = std::thread::Builder::new()
            .name("rbcodec".into())
            .spawn(move || {
                let status = unsafe { rbcodec_run() };
                let _ = tx.send(Msg::Done(status));
            })
            .expect("spawn codec thread");

        Ok(Decoder {
            rx,
            thread: Some(thread),
            _sink: sink,
            _reader: Some(reader),
            _seek: None,
            meta,
            status: None,
        })
    }

    /// Open a *seekable* byte source (`Read + Seek`, e.g. an HTTP file
    /// buffered on demand via range requests) and start decoding. Unlike
    /// [`Decoder::open`] there is no local file: the codec reads and seeks
    /// through `source` lazily, so decoding starts as soon as the header is
    /// available — the whole file need never be downloaded.
    ///
    /// `size` is the total byte length (`0` if unknown); `frequency` seeds the
    /// codec's sample rate for formats that need it up front (e.g. WAV), since
    /// `get_metadata` is skipped. `meta` supplies the tags/duration (parse
    /// them separately, e.g. from a prefetched header). The codec is chosen
    /// from `format_ext` (`"mp3"`, `"flac"`, …).
    ///
    /// Blocks while another `Decoder` exists (the codec state is global).
    pub fn open_seekable(
        source: Box<dyn SeekableSource>,
        size: u64,
        frequency: u32,
        format_ext: &str,
        meta: Metadata,
    ) -> Result<Self, Error> {
        let c_ext = CString::new(format_ext).map_err(|_| Error::InvalidPath)?;

        GATE.acquire();

        let (tx, rx) = std::sync::mpsc::sync_channel::<Msg>(8);
        let sink = Box::new(SinkCtx { tx: tx.clone() });
        let seek = Box::new(SeekCtx { source });

        let rc = unsafe {
            rbcodec_set_sink(Some(sink_trampoline), &*sink as *const SinkCtx as *mut c_void);
            rbcodec_open_seekable(
                Some(seek_read_trampoline),
                Some(seek_seek_trampoline),
                &*seek as *const SeekCtx as *mut c_void,
                size as i64,
                frequency as std::ffi::c_ulong,
                c_ext.as_ptr(),
            )
        };
        if rc != 0 {
            GATE.release();
            return Err(match rc {
                -3 => Error::CodecNotAvailable(meta.codec.clone()),
                _ => Error::CodecInit(std::path::PathBuf::from(format_ext)),
            });
        }

        let thread = std::thread::Builder::new()
            .name("rbcodec".into())
            .spawn(move || {
                let status = unsafe { rbcodec_run() };
                let _ = tx.send(Msg::Done(status));
            })
            .expect("spawn codec thread");

        Ok(Decoder {
            rx,
            thread: Some(thread),
            _sink: sink,
            _reader: None,
            _seek: Some(seek),
            meta,
            status: None,
        })
    }

    /// Tags and stream properties of the open file.
    pub fn metadata(&self) -> &Metadata {
        &self.meta
    }

    /// Next buffer of decoded PCM, or `None` at end of track (or after a
    /// codec error — check [`Decoder::status`]).
    pub fn next_chunk(&mut self) -> Option<Chunk> {
        if self.status.is_some() {
            return None;
        }
        match self.rx.recv() {
            Ok(Msg::Pcm(chunk)) => Some(chunk),
            Ok(Msg::Done(status)) => {
                self.status = Some(status);
                None
            }
            Err(_) => {
                self.status = Some(-1);
                None
            }
        }
    }

    /// Request a seek to `pos` (picked up at the codec's next command
    /// poll; PCM already queued from before the seek still drains first).
    pub fn seek(&mut self, pos: Duration) {
        unsafe { rbcodec_request_seek(pos.as_millis() as std::ffi::c_long) };
    }

    /// Playback position last reported by the codec.
    pub fn elapsed(&self) -> Duration {
        Duration::from_millis(unsafe { rbcodec_elapsed_ms() } as u64)
    }

    /// Codec exit status: `Some(0)` after a clean end of track,
    /// `Some(negative)` after a codec error, `None` while decoding.
    pub fn status(&self) -> Option<i32> {
        self.status
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // Only halt-and-drain when the track is still decoding. If it
        // already finished (`status` set), its `Done` was consumed by
        // next_chunk() and the codec thread has exited — draining again
        // would block on the 5 s timeout because the sink keeps a live
        // sender, so there's nothing left to receive.
        if self.status.is_none() {
            unsafe { rbcodec_request_halt() };
            // Drain so a blocked sink send can't deadlock the codec thread.
            while let Ok(msg) = self.rx.recv_timeout(Duration::from_secs(5)) {
                if matches!(msg, Msg::Done(_)) {
                    break;
                }
            }
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        unsafe {
            rbcodec_close();
            rbcodec_set_sink(None, std::ptr::null_mut());
        }
        GATE.release();
    }
}
