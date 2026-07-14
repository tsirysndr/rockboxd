//! Flat C ABI over [`rockbox_codecs`] — decode a local audio file to
//! interleaved-stereo `i16` PCM, one chunk at a time.
//!
//! The Rust [`rockbox_codecs::Decoder`] also has streaming / seekable
//! constructors, but those take Rust `Read`/`Seek` trait objects that can't
//! cross a flat C ABI, so only the file-based `open` path is exposed here.
//! The codec state is global: only one `RbDecoder` can decode at a time —
//! a second `rb_decoder_open` blocks until the first is freed.

use crate::meta::MetadataJson;
use crate::util::{cstr, into_cstring};
use rockbox_codecs::Decoder;
use std::os::raw::c_char;
use std::time::Duration;

/// Opaque decoder handle. Create with [`rb_decoder_open`], destroy with
/// [`rb_decoder_free`].
pub struct RbDecoder {
    inner: Decoder,
}

/// Open the audio file at `path` and start decoding. Returns null if `path`
/// is null/invalid, the file cannot be opened, no parser recognises it, or
/// the matching codec isn't compiled in. Free with [`rb_decoder_free`].
///
/// Blocks while another `RbDecoder` is still open (the codec state is
/// global).
#[no_mangle]
pub extern "C" fn rb_decoder_open(path: *const c_char) -> *mut RbDecoder {
    let Some(path) = cstr(path) else {
        return std::ptr::null_mut();
    };
    match Decoder::open(path) {
        Ok(inner) => Box::into_raw(Box::new(RbDecoder { inner })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Destroy a decoder handle (halting decode if still in progress). Null is
/// ignored.
#[no_mangle]
pub extern "C" fn rb_decoder_free(d: *mut RbDecoder) {
    if !d.is_null() {
        unsafe { drop(Box::from_raw(d)) };
    }
}

/// The open file's tags and stream properties as a JSON string (same shape
/// as `rb_meta_read_json`), to free with `rb_string_free`. Null on a null
/// handle or serialisation failure.
#[no_mangle]
pub extern "C" fn rb_decoder_metadata_json(d: *mut RbDecoder) -> *mut c_char {
    if d.is_null() {
        return std::ptr::null_mut();
    }
    let d = unsafe { &*d };
    let json = MetadataJson::from(d.inner.metadata());
    match serde_json::to_string(&json) {
        Ok(s) => into_cstring(s),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Pull the next buffer of decoded PCM (interleaved-stereo `i16`). Writes the
/// sample count (`frames * 2`) to `*out_len` and the codec's current output
/// rate in Hz to `*out_sample_rate`. Returns null at end of track or after a
/// codec error (check [`rb_decoder_finished`]), and on a null handle. Free a
/// non-null buffer with `rb_buffer_free(ptr, out_len)`.
#[no_mangle]
pub extern "C" fn rb_decoder_next_chunk(
    d: *mut RbDecoder,
    out_len: *mut usize,
    out_sample_rate: *mut u32,
) -> *mut i16 {
    if !out_len.is_null() {
        unsafe { *out_len = 0 };
    }
    if !out_sample_rate.is_null() {
        unsafe { *out_sample_rate = 0 };
    }
    if d.is_null() {
        return std::ptr::null_mut();
    }
    let d = unsafe { &mut *d };
    let Some(chunk) = d.inner.next_chunk() else {
        return std::ptr::null_mut();
    };
    // Boxed slice guarantees capacity == length so `rb_buffer_free` can
    // rebuild the Vec with `cap = len`.
    let boxed = chunk.pcm.into_boxed_slice();
    let len = boxed.len();
    let ptr = Box::into_raw(boxed) as *mut i16;
    if !out_len.is_null() {
        unsafe { *out_len = len };
    }
    if !out_sample_rate.is_null() {
        unsafe { *out_sample_rate = chunk.sample_rate };
    }
    ptr
}

/// Request a seek to `ms` milliseconds (picked up at the codec's next command
/// poll; PCM already queued from before the seek drains first). No-op on a
/// null handle.
#[no_mangle]
pub extern "C" fn rb_decoder_seek_ms(d: *mut RbDecoder, ms: u64) {
    if d.is_null() {
        return;
    }
    let d = unsafe { &mut *d };
    d.inner.seek(Duration::from_millis(ms));
}

/// Playback position last reported by the codec, in milliseconds (0 on a
/// null handle).
#[no_mangle]
pub extern "C" fn rb_decoder_elapsed_ms(d: *mut RbDecoder) -> u64 {
    if d.is_null() {
        return 0;
    }
    let d = unsafe { &*d };
    d.inner.elapsed().as_millis() as u64
}

/// Whether decoding has finished. Returns `true` once the track has ended,
/// `false` while still decoding (and on a null handle). When it returns
/// `true` and `out_code` is non-null, `*out_code` receives the codec exit
/// code: `0` for a clean end of track, negative for a codec error.
#[no_mangle]
pub extern "C" fn rb_decoder_finished(d: *mut RbDecoder, out_code: *mut i32) -> bool {
    if d.is_null() {
        return false;
    }
    let d = unsafe { &*d };
    match d.inner.status() {
        Some(code) => {
            if !out_code.is_null() {
                unsafe { *out_code = code };
            }
            true
        }
        None => false,
    }
}
