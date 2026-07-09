//! Small helpers shared by the FFI modules: string/buffer marshalling and
//! the deallocators the foreign side must call.

use std::ffi::{c_char, CStr, CString};

/// Borrow a C string as `&str`, or `None` if the pointer is null or the
/// bytes are not valid UTF-8.
pub(crate) fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

/// Move a Rust `String` into a heap C string the caller owns. The caller
/// MUST return it via [`rb_string_free`]. Returns null on an interior NUL.
pub(crate) fn into_cstring(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string previously handed out by any `*_json` / `*_label` call.
/// Null is ignored. Double-free is undefined — call exactly once.
#[no_mangle]
pub extern "C" fn rb_string_free(p: *mut c_char) {
    if !p.is_null() {
        unsafe { drop(CString::from_raw(p)) };
    }
}

/// Free an `i16` sample buffer previously returned by `rb_dsp_process`.
/// `len` must be the value written to its `out_len` out-parameter. Null is
/// ignored.
#[no_mangle]
pub extern "C" fn rb_buffer_free(p: *mut i16, len: usize) {
    if !p.is_null() {
        unsafe { drop(Vec::from_raw_parts(p, len, len)) };
    }
}
