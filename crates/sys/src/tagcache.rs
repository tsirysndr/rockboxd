use std::ffi::{c_char, c_long, c_void};

use crate::{Mp3Entry, TagcacheSearch, TagcacheStat};

pub fn search(tcs: *mut TagcacheSearch, tag: i32) -> bool {
    let ret = unsafe { crate::tagcache_search(tcs, tag) };
    ret != 0
}

pub fn search_set_uniqbuf(tcs: *mut TagcacheSearch, buffer: *mut c_void, length: i64) {
    unsafe { crate::tagcache_search_set_uniqbuf(tcs, buffer, length as c_long) }
}

pub fn search_add_filter(tcs: *mut TagcacheSearch, tag: i32, seek: i32) -> bool {
    let ret = unsafe { crate::tagcache_search_add_filter(tcs, tag, seek) };
    ret != 0
}

pub fn get_next(tcs: *mut TagcacheSearch, buf: *mut c_char, size: i64) -> bool {
    let ret = unsafe { crate::tagcache_get_next(tcs, buf, size as c_long) };
    ret != 0
}

pub fn get_numeric(tcs: *mut TagcacheSearch, tag: i32) -> i64 {
    unsafe { crate::tagcache_get_numeric(tcs, tag) as i64 }
}

pub fn get_stat() -> TagcacheStat {
    unsafe { crate::tagcache_get_stat() }
}

pub fn commit_finalize() {
    unsafe {
        crate::tagcache_commit_finalize();
    }
}

pub fn search_albumart_files(
    id3: *mut Mp3Entry,
    size_string: *const c_char,
    buf: *mut c_char,
    buflen: i32,
) -> bool {
    let ret = unsafe { crate::search_albumart_files(id3, size_string, buf, buflen) };
    ret != 0
}
