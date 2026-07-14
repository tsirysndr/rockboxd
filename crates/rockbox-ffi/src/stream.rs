//! Push-based byte stream for decoding **infinite / live** sources such as
//! internet radio, where the total length is unknown and the whole thing can
//! never be buffered.
//!
//! The caller (JS) pushes encoded bytes with [`rb_stream_feed`] as they arrive
//! off the network; the codec thread pulls them through a blocking [`Read`]:
//! when the buffer runs dry it *waits* (rather than reporting end-of-stream)
//! until more is fed or the stream is explicitly closed with
//! [`rb_stream_close`]. That distinction — empty-but-open vs. closed — is what
//! keeps a live stream playing forever instead of stopping at the first gap.
//!
//! Ordering matters on teardown: **close the stream before freeing the
//! decoder**. `Decoder`'s drop joins the codec thread, which can be parked
//! inside `read()`; closing first makes that read return EOF so the thread can
//! exit and the join completes (otherwise it would hang).

use crate::codecs::RbDecoder;
use crate::util::cstr;
use rockbox_codecs::{Decoder, Metadata};
use std::collections::VecDeque;
use std::io::Read;
use std::os::raw::c_char;
use std::sync::{Arc, Condvar, Mutex};

struct Buffer {
    data: VecDeque<u8>,
    closed: bool,
}

type Shared = Arc<(Mutex<Buffer>, Condvar)>;

/// Opaque push-stream handle. Create with [`rb_stream_new`], free with
/// [`rb_stream_free`] (after the decoder that reads it has been freed).
pub struct RbStream {
    shared: Shared,
}

/// The blocking reader handed to the codec thread. Reads drain the shared
/// buffer; an empty-but-open buffer parks the thread until [`rb_stream_feed`]
/// or [`rb_stream_close`] wakes it.
struct StreamReader {
    shared: Shared,
}

impl Read for StreamReader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let (lock, cv) = &*self.shared;
        let mut buf = lock.lock().unwrap();
        loop {
            if !buf.data.is_empty() {
                let n = out.len().min(buf.data.len());
                for (slot, byte) in out[..n].iter_mut().zip(buf.data.drain(..n)) {
                    *slot = byte;
                }
                return Ok(n);
            }
            if buf.closed {
                return Ok(0); // genuine end of stream
            }
            buf = cv.wait(buf).unwrap();
        }
    }
}

/// Create an empty push stream. Returns null only on allocation failure.
#[no_mangle]
pub extern "C" fn rb_stream_new() -> *mut RbStream {
    let shared: Shared = Arc::new((
        Mutex::new(Buffer {
            data: VecDeque::new(),
            closed: false,
        }),
        Condvar::new(),
    ));
    Box::into_raw(Box::new(RbStream { shared }))
}

/// Append `len` bytes at `ptr` to the stream and wake the codec thread. A null
/// handle/pointer or `len == 0` is a no-op.
///
/// # Safety
/// `ptr` must point to at least `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn rb_stream_feed(s: *mut RbStream, ptr: *const u8, len: usize) {
    if s.is_null() || ptr.is_null() || len == 0 {
        return;
    }
    let bytes = std::slice::from_raw_parts(ptr, len);
    let (lock, cv) = &*(*s).shared;
    let mut buf = lock.lock().unwrap();
    buf.data.extend(bytes.iter().copied());
    cv.notify_all();
}

/// Mark the stream as ended: once the buffered bytes are drained, the codec's
/// next `read()` returns EOF. Call this before [`rb_decoder_free`] on a stream
/// decoder so the codec thread can exit. Null is a no-op.
///
/// # Safety
/// `s` must be null or a live handle from [`rb_stream_new`].
#[no_mangle]
pub unsafe extern "C" fn rb_stream_close(s: *mut RbStream) {
    if s.is_null() {
        return;
    }
    let (lock, cv) = &*(*s).shared;
    let mut buf = lock.lock().unwrap();
    buf.closed = true;
    cv.notify_all();
}

/// Number of buffered, not-yet-decoded bytes. The JS pump uses this to only
/// pull PCM when there is enough encoded input that the decode won't park the
/// worker thread. 0 on a null handle.
///
/// # Safety
/// `s` must be null or a live handle from [`rb_stream_new`].
#[no_mangle]
pub unsafe extern "C" fn rb_stream_available(s: *mut RbStream) -> usize {
    if s.is_null() {
        return 0;
    }
    let (lock, _cv) = &*(*s).shared;
    lock.lock().unwrap().data.len()
}

/// Free a push-stream handle. Free the decoder reading it *first*. Null is a
/// no-op.
///
/// # Safety
/// `s` must be null or a live handle from [`rb_stream_new`], freed once.
#[no_mangle]
pub unsafe extern "C" fn rb_stream_free(s: *mut RbStream) {
    if !s.is_null() {
        drop(Box::from_raw(s));
    }
}

/// Open a decoder over a live push stream. `format_ext` names the container /
/// codec (`"mp3"`, `"aac"`, `"ogg"`, …) since there is no file to sniff —
/// derive it from the HTTP `Content-Type` or the URL. Returns a handle usable
/// with all the other `rb_decoder_*` functions (except seeking, which a live
/// stream ignores), or null if the codec isn't available. Metadata carries the
/// codec label with an unknown (0) duration.
///
/// Blocks while another decoder is still open (codec state is global).
///
/// # Safety
/// `s` must be a live handle from [`rb_stream_new`]; `format_ext` a valid C
/// string. The stream must outlive the returned decoder.
#[no_mangle]
pub unsafe extern "C" fn rb_decoder_open_stream(
    s: *mut RbStream,
    format_ext: *const c_char,
) -> *mut RbDecoder {
    if s.is_null() {
        return std::ptr::null_mut();
    }
    let ext = cstr(format_ext).unwrap_or("mp3");
    let reader = Box::new(StreamReader {
        shared: (*s).shared.clone(),
    });
    let meta = Metadata {
        codec: ext.to_string(),
        ..Metadata::default()
    };
    match Decoder::open_stream(reader, ext, meta) {
        Ok(inner) => Box::into_raw(Box::new(RbDecoder { inner })),
        Err(_) => std::ptr::null_mut(),
    }
}
