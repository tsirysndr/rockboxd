use std::ffi::{c_void, CString};

use crate::{PcmPlayCallbackType, PcmStatusCallbackType};

pub const PCM_SINK_BUILTIN: i32 = 0;
pub const PCM_SINK_FIFO: i32 = 1;
pub const PCM_SINK_AIRPLAY: i32 = 2;
pub const PCM_SINK_SQUEEZELITE: i32 = 3;
pub const PCM_SINK_UPNP: i32 = 4;
pub const PCM_SINK_CHROMECAST: i32 = 5;
pub const PCM_SINK_SNAPCAST_TCP: i32 = 6;
// PCM_SINK_AAUDIO = 7 on Android cdylib builds
// PCM_SINK_CPAL   = 7 on headless macOS/Linux builds
pub const PCM_SINK_CPAL: i32 = 7;
/// CMAF (HLS + DASH) AAC-LC output. Pinned to 8 in `firmware/export/pcm_sink.h`
/// so it never collides with the conditional slot 7 (CPAL / WEBAPI).
pub const PCM_SINK_CMAF: i32 = 8;
/// Direct libasound sink for arm-linux-gnueabihf. Pinned to 9.
pub const PCM_SINK_ALSA: i32 = 9;

pub fn apply_settings() {
    unsafe {
        crate::pcm_apply_settings();
    }
}

pub fn play_data(
    get_more: PcmPlayCallbackType,
    status_cb: PcmStatusCallbackType,
    start: *const *const c_void,
    size: usize,
) {
    unsafe { crate::pcm_play_data(get_more, status_cb, start, size) }
}

pub fn play_stop() {
    unsafe {
        crate::pcm_play_stop();
    }
}

pub fn set_frequency(frequency: u32) {
    unsafe { crate::pcm_set_frequency(frequency) }
}

pub fn is_playing() -> bool {
    let ret = unsafe { crate::pcm_is_playing() };
    ret != 0
}

pub fn is_initialized() -> bool {
    unsafe { crate::pcm_is_initialized() }
}

pub fn play_lock() {
    unsafe {
        crate::pcm_play_lock();
    }
}

pub fn play_unlock() {
    unsafe {
        crate::pcm_play_unlock();
    }
}

pub fn switch_sink(sink: i32) -> bool {
    unsafe { crate::pcm_switch_sink(sink) != 0 }
}

/// Output levels for a meter, measured on the PCM leaving the device.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Levels {
    /// 0..1 RMS over one output buffer.
    pub left: f32,
    pub right: f32,
    /// The same signal below roughly 200 Hz, which is what makes a meter move
    /// with the bass rather than with whatever is loudest.
    pub low_left: f32,
    pub low_right: f32,
    /// Coarse spectrum, low band to high — `PCM_METER_BANDS` in the firmware.
    pub bands: [f32; METER_BANDS],
}

/// Matches `PCM_METER_BANDS` in `firmware/export/pcm_meter.h`.
pub const METER_BANDS: usize = 16;

/// Read the levels the audio path last published. Zero while stopped, so a
/// meter reads as stopped rather than stuck at its last value.
pub fn levels() -> Levels {
    /// Matches `PCM_METER_SCALE` in `firmware/export/pcm_meter.h`.
    const SCALE: f32 = 32767.0;

    let (mut left, mut right, mut low_left, mut low_right) = (0u32, 0u32, 0u32, 0u32);
    unsafe {
        crate::pcm_meter_read(&mut left, &mut right, &mut low_left, &mut low_right);
    }

    let mut bands = [0u32; METER_BANDS];
    unsafe {
        crate::pcm_meter_read_bands(bands.as_mut_ptr());
    }

    Levels {
        left: (left as f32 / SCALE).min(1.0),
        right: (right as f32 / SCALE).min(1.0),
        low_left: (low_left as f32 / SCALE).min(1.0),
        low_right: (low_right as f32 / SCALE).min(1.0),
        // Narrow bands carry little of the total energy; scaled the same way
        // the bass band is, a little stronger, to reach the top of a bar.
        bands: bands.map(|b| (b as f32 * 4.0 / SCALE).min(1.0)),
    }
}

pub fn airplay_set_host(host: &str, port: u16) {
    let chost = CString::new(host).expect("host must not contain null bytes");
    unsafe { crate::pcm_airplay_set_host(chost.as_ptr(), port) }
    std::mem::forget(chost);
}

pub fn airplay_add_receiver(host: &str, port: u16) {
    let chost = CString::new(host).expect("host must not contain null bytes");
    unsafe { crate::pcm_airplay_add_receiver(chost.as_ptr(), port) }
    std::mem::forget(chost);
}

pub fn airplay_clear_receivers() {
    unsafe { crate::pcm_airplay_clear_receivers() }
}

pub fn fifo_set_path(path: &str) {
    let cpath = CString::new(path).expect("path must not contain null bytes");
    unsafe { crate::pcm_fifo_set_path(cpath.as_ptr()) }
    // Keep alive until C code finishes using it — it's only read during init,
    // so leaking is acceptable here for a startup-time config call.
    std::mem::forget(cpath);
}

pub fn squeezelite_set_slim_port(port: u16) {
    unsafe { crate::pcm_squeezelite_set_slim_port(port) }
}

pub fn squeezelite_set_http_port(port: u16) {
    unsafe { crate::pcm_squeezelite_set_http_port(port) }
}

pub fn upnp_set_http_port(port: u16) {
    unsafe { crate::pcm_upnp_set_http_port(port) }
}

pub fn upnp_set_renderer_url(url: &str) {
    let curl = std::ffi::CString::new(url).expect("url must not contain null bytes");
    unsafe { crate::pcm_upnp_set_renderer_url(curl.as_ptr()) }
    std::mem::forget(curl);
}

pub fn upnp_clear_renderer_url() {
    unsafe { crate::pcm_upnp_set_renderer_url(std::ptr::null()) }
}

/// Reset renderer-side state so the next play always sends SetAVTransportURI+Play.
/// Call this before switch_sink(PCM_SINK_UPNP) so output switching works live.
pub fn upnp_reset_renderer() {
    unsafe { crate::pcm_upnp_reset_renderer() }
}

pub fn chromecast_set_http_port(port: u16) {
    unsafe { crate::pcm_chromecast_set_http_port(port) }
}

pub fn chromecast_set_device_host(host: &str) {
    let chost = std::ffi::CString::new(host).expect("host must not contain null bytes");
    unsafe { crate::pcm_chromecast_set_device_host(chost.as_ptr()) }
    std::mem::forget(chost);
}

pub fn chromecast_set_device_port(port: u16) {
    unsafe { crate::pcm_chromecast_set_device_port(port) }
}

pub fn chromecast_teardown() {
    unsafe { crate::pcm_chromecast_teardown() }
}

pub fn tcp_set_host(host: &str) {
    let chost = CString::new(host).expect("host must not contain null bytes");
    unsafe { crate::pcm_tcp_set_host(chost.as_ptr()) }
    std::mem::forget(chost);
}

pub fn tcp_set_port(port: u16) {
    unsafe { crate::pcm_tcp_set_port(port) }
}

pub fn cmaf_set_http_port(port: u16) {
    unsafe { crate::pcm_cmaf_set_http_port(port) }
}

pub fn cmaf_set_bitrate(bps: u32) {
    unsafe { crate::pcm_cmaf_set_bitrate(bps) }
}

/// Start the CMAF HTTP server and encoder pipeline eagerly, before any
/// audio actually plays. Idempotent — safe to call repeatedly.
///
/// Without this the HTTP server only binds on the first `sink_dma_start`
/// (which happens on the first track), so HLS clients that try to open
/// `http://host:7882/hls/master.m3u8` immediately after the user picks the
/// CMAF device get connection-refused. Call this at sink-select time
/// (settings load + device-picker connect) and the endpoint is reachable
/// as soon as the device is active.
pub fn cmaf_start() {
    unsafe { crate::pcm_cmaf_start() };
}

/// Set (or clear via `None`) the directory the CMAF sink mirrors segments
/// and manifests into. None = in-memory only.
pub fn cmaf_set_segment_dir(dir: Option<&str>) {
    match dir {
        Some(d) if !d.is_empty() => {
            let cd = CString::new(d).expect("dir must not contain null bytes");
            unsafe { crate::pcm_cmaf_set_segment_dir(cd.as_ptr()) }
            std::mem::forget(cd);
        }
        _ => unsafe { crate::pcm_cmaf_set_segment_dir(std::ptr::null()) },
    }
}
