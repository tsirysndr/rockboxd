use std::ffi::{c_char, c_int, c_uint, c_void};

pub fn open_utf8() {
    unsafe {
        crate::open_utf8();
    }
}

/// Open a file, passing permission bits when creation is requested.
///
/// # Safety
/// `path` must point to a valid, NUL-terminated string.
pub unsafe fn open(path: *const c_char, flags: c_int, mode: c_uint) -> c_int {
    unsafe { crate::open(path, flags, mode) }
}

pub fn creat() {
    unsafe {
        crate::creat();
    }
}

pub fn close(fd: c_int) -> c_int {
    unsafe { crate::close(fd) }
}

/// # Safety
/// `buf` must point to at least `count` writable bytes.
pub unsafe fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize {
    unsafe { crate::read(fd, buf, count) }
}

pub fn lseek() {
    unsafe {
        crate::lseek();
    }
}

/// # Safety
/// `buf` must point to at least `count` readable bytes.
pub unsafe fn write(fd: c_int, buf: *const c_void, count: usize) -> isize {
    unsafe { crate::write(fd, buf, count) }
}

pub fn remove() {
    unsafe {
        crate::remove();
    }
}

pub fn rename() {
    unsafe {
        crate::rename();
    }
}

pub fn ftruncate() {
    unsafe {
        crate::ftruncate();
    }
}

pub fn fdprintf() {
    unsafe {
        crate::fdprintf();
    }
}

pub fn read_line() {
    unsafe {
        crate::read_line();
    }
}

pub fn settings_parseline() {
    unsafe {
        crate::settings_parseline();
    }
}

pub fn reload_directory() {
    unsafe {
        crate::reload_directory();
    }
}

pub fn create_numbered_filename() {
    unsafe {
        crate::create_numbered_filename();
    }
}

pub fn strip_extension() {
    unsafe {
        crate::strip_extension();
    }
}

pub fn crc_32() {
    unsafe {
        crate::crc_32();
    }
}

pub fn crc_32r() {
    unsafe {
        crate::crc_32r();
    }
}

pub fn filetype_get_attr() {
    unsafe {
        crate::filetype_get_attr();
    }
}

pub fn filetype_get_plugin() {
    unsafe {
        crate::filetype_get_plugin();
    }
}
