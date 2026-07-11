//! Smoke tests for the flat C ABI. The standalone helpers (URL detection,
//! m3u read/write, resume peek) need no audio device; the player round-trip
//! (insert → queue) skips when no output device is available.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use rockbox_ffi::*;

/// Call with a C string argument built from `s`.
fn c(s: &str) -> CString {
    CString::new(s).unwrap()
}

/// Take ownership of a returned JSON C string and free it.
fn take_json(p: *mut c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    rb_string_free(p);
    Some(s)
}

#[test]
fn url_detection() {
    assert!(rb_is_url(c("https://example.com/a.flac").as_ptr()));
    assert!(rb_is_url(
        c("http://ec7.yesstreaming.net:1360/stream").as_ptr()
    ));
    assert!(!rb_is_url(c("/music/a.flac").as_ptr()));
    assert!(!rb_is_url(std::ptr::null()));
}

#[test]
fn m3u_write_then_read() {
    let path = std::env::temp_dir().join(format!("rbffi_{}.m3u8", std::process::id()));
    let cpath = c(path.to_str().unwrap());
    let json = c(r#"["/music/a.flac","/music/b c.mp3"]"#);

    assert_eq!(rb_m3u_write_json(cpath.as_ptr(), json.as_ptr()), 0);

    let read = take_json(rb_m3u_read_json(cpath.as_ptr())).expect("read back");
    // Entries are objects with a `path` field.
    assert!(read.contains("/music/a.flac"));
    assert!(read.contains("/music/b c.mp3"));
    assert!(read.contains("\"path\""));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn resume_peek_absent_is_null() {
    let missing = c("/definitely/not/here.m3u8");
    assert!(rb_load_resume_json(missing.as_ptr()).is_null());
}

#[test]
fn player_insert_and_queue_roundtrip() {
    // Needs an output device; skip otherwise.
    let p = rb_player_new();
    if p.is_null() {
        eprintln!("no output device — skipping");
        return;
    }

    rb_player_set_queue_json(p, c(r#"["a.flac","b.flac"]"#).as_ptr());
    // Insert "next" (position 2) after the (stopped) current head.
    rb_player_insert_json(p, c(r#"["x.flac"]"#).as_ptr(), 2, 0);
    // Append (position 3).
    rb_player_insert_json(p, c(r#"["z.flac"]"#).as_ptr(), 3, 0);

    // Commands run on the engine thread, so poll the queue until it settles.
    let mut paths = Vec::new();
    for _ in 0..100 {
        if let Some(json) = take_json(rb_player_queue_json(p)) {
            if let Ok(v) = serde_json::from_str::<Vec<String>>(&json) {
                if v.len() == 4 {
                    paths = v;
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(paths.len(), 4, "2 initial + 2 inserted");
    assert_eq!(paths.first().map(String::as_str), Some("a.flac"));
    assert_eq!(paths.last().map(String::as_str), Some("z.flac"));
    assert!(paths.contains(&"x.flac".to_string()));

    // Export it and read it back through the m3u path.
    let out = std::env::temp_dir().join(format!("rbffi_q_{}.m3u8", std::process::id()));
    assert_eq!(
        rb_player_export_m3u(p, c(out.to_str().unwrap()).as_ptr()),
        0
    );
    let entries = take_json(rb_m3u_read_json(c(out.to_str().unwrap()).as_ptr())).unwrap();
    assert!(entries.contains("z.flac"));

    let _ = std::fs::remove_file(&out);
    rb_player_free(p);
}
