// Direct libsndio PCM sink for OpenBSD.
//
// OpenBSD has no ALSA and cpal has no sndio backend, so this crate talks to
// libsndio (sio_open/sio_setpar/sio_start/sio_write) directly — the native,
// server-friendly (sndiod) audio path. All real code is gated on the
// `sndio_backend` cfg (target_os = "openbsd", set from build.rs); every other
// target compiles down to the empty `_link_sndio_sink` stub.
//
// Design mirrors crates/alsa-sink: open sndio once in pcm_sndio_postinit() and
// keep a writer thread alive for the daemon lifetime. pcm_sndio_start/stop only
// toggle the `running` flag in the ring — no re-open, no thread re-create.
//
// Data flow:
//   firmware DMA thread
//     → pcm_sndio_push(data, size)   (blocks on back-pressure)
//       → ring buffer (VecDeque)
//         ← writer thread drains via sio_write when running=true

/// Force-linkage sentinel. Always present so crates/cli can reference it with
/// #[cfg(feature = "sndio-sink")] without needing a cfg(target_os) guard.
pub fn _link_sndio_sink() {}

// ── OpenBSD-only implementation ───────────────────────────────────────────────

#[cfg(sndio_backend)]
mod imp {
    use std::collections::VecDeque;
    use std::os::raw::{c_char, c_int, c_uint, c_void};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::thread::JoinHandle;
    use std::time::Duration;

    const RING_CAPACITY: usize = 512 * 1024; // 512 KB ≈ 3 s at 44.1 kHz stereo S16LE
                                             // Drain at most ~one sndio period worth of bytes per wakeup (16 KB ≈ 93 ms).
    const CHUNK_BYTES: usize = 16 * 1024;

    // ── libsndio FFI ──────────────────────────────────────────────────────────
    //
    // struct sio_par layout from <sndio.h> — must match exactly (repr(C)).
    #[repr(C)]
    struct SioPar {
        bits: c_uint,
        bps: c_uint,
        sig: c_uint,
        le: c_uint,
        msb: c_uint,
        rchan: c_uint,
        pchan: c_uint,
        rate: c_uint,
        bufsz: c_uint,
        xrun: c_uint,
        round: c_uint,
        appbufsz: c_uint,
        __pad: [c_int; 3],
        __magic: c_uint,
    }

    const SIO_PLAY: c_uint = 1;
    const SIO_IGNORE: c_uint = 0; // xrun policy: pad underruns with silence
    const SIO_DEVANY: &[u8] = b"default\0";

    extern "C" {
        fn sio_open(name: *const c_char, mode: c_uint, nbio: c_int) -> *mut c_void;
        fn sio_close(hdl: *mut c_void);
        fn sio_initpar(par: *mut SioPar);
        fn sio_setpar(hdl: *mut c_void, par: *mut SioPar) -> c_int;
        fn sio_getpar(hdl: *mut c_void, par: *mut SioPar) -> c_int;
        fn sio_start(hdl: *mut c_void) -> c_int;
        fn sio_write(hdl: *mut c_void, buf: *const c_void, len: usize) -> usize;
        fn sio_eof(hdl: *mut c_void) -> c_int;
    }

    /// Owns a `struct sio_hdl *`; closed on drop. Never leaves the writer thread.
    struct Sio(*mut c_void);
    impl Drop for Sio {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { sio_close(self.0) };
            }
        }
    }

    // ── Ring buffer ────────────────────────────────────────────────────────────

    struct Ring {
        buf: VecDeque<u8>,
        running: bool,
        shutdown: bool,
    }

    static RING: OnceLock<(Mutex<Ring>, Condvar)> = OnceLock::new();

    fn ring() -> &'static (Mutex<Ring>, Condvar) {
        RING.get_or_init(|| {
            (
                Mutex::new(Ring {
                    buf: VecDeque::with_capacity(RING_CAPACITY),
                    running: false,
                    shutdown: false,
                }),
                Condvar::new(),
            )
        })
    }

    // ── Writer thread state ──────────────────────────────────────────────────────

    static CURRENT_RATE: OnceLock<Mutex<u32>> = OnceLock::new();
    static WRITER_HANDLE: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();
    static WRITER_ALIVE: AtomicBool = AtomicBool::new(false);

    fn current_rate() -> &'static Mutex<u32> {
        CURRENT_RATE.get_or_init(|| Mutex::new(44100))
    }

    fn writer_handle() -> &'static Mutex<Option<JoinHandle<()>>> {
        WRITER_HANDLE.get_or_init(|| Mutex::new(None))
    }

    // ── sndio helpers ────────────────────────────────────────────────────────────

    fn open_sndio(rate: u32) -> Option<Sio> {
        let hdl = unsafe { sio_open(SIO_DEVANY.as_ptr() as *const c_char, SIO_PLAY, 0) };
        if hdl.is_null() {
            tracing::error!("pcm-sndio: sio_open('default') failed");
            return None;
        }
        let sio = Sio(hdl);

        let mut par: SioPar = unsafe { std::mem::zeroed() };
        unsafe { sio_initpar(&mut par) };
        par.bits = 16;
        par.bps = 2;
        par.sig = 1;
        par.le = 1;
        par.pchan = 2;
        par.rate = rate;
        par.xrun = SIO_IGNORE;
        par.appbufsz = rate / 10; // ~100 ms; the upstream ring absorbs the rest

        if unsafe { sio_setpar(sio.0, &mut par) } == 0 {
            tracing::error!("pcm-sndio: sio_setpar failed for {rate} Hz");
            return None;
        }
        if unsafe { sio_getpar(sio.0, &mut par) } == 0 {
            tracing::error!("pcm-sndio: sio_getpar failed");
            return None;
        }
        if par.bits != 16 || par.pchan != 2 {
            tracing::warn!(
                "pcm-sndio: device negotiated bits={} pchan={} (wanted 16/2)",
                par.bits,
                par.pchan
            );
        }
        if unsafe { sio_start(sio.0) } == 0 {
            tracing::error!("pcm-sndio: sio_start failed");
            return None;
        }
        tracing::info!(
            "pcm-sndio: opened 'default' at {} Hz stereo S16LE",
            par.rate
        );
        Some(sio)
    }

    // ── Writer thread — lives for the daemon lifetime ────────────────────────────

    fn run_writer(initial_rate: u32) {
        let mut sio: Option<Sio> = open_sndio(initial_rate);
        let mut rate = initial_rate;
        tracing::info!("pcm-sndio: writer thread started");

        loop {
            let chunk: Vec<u8> = {
                let (lock, cvar) = ring();
                let mut r = lock.lock().unwrap();
                loop {
                    if r.shutdown {
                        tracing::info!("pcm-sndio: writer thread shutting down");
                        WRITER_ALIVE.store(false, Ordering::Relaxed);
                        return;
                    }
                    if r.running && r.buf.len() >= 4 {
                        break;
                    }
                    r = cvar.wait(r).unwrap();
                }
                let n = r.buf.len().min(CHUNK_BYTES);
                r.buf.drain(..n).collect()
            };
            ring().1.notify_all(); // signal push() that ring space freed

            if chunk.is_empty() {
                continue;
            }

            // Reopen sndio if the sample rate changed (rare) or it was lost.
            let new_rate = *current_rate().lock().unwrap();
            if new_rate != rate || sio.is_none() {
                drop(sio.take()); // close the old handle (sio_close) first
                rate = new_rate;
                sio = open_sndio(rate);
            }

            let hdl = match sio.as_ref() {
                Some(s) => s.0,
                None => continue,
            };

            // sio_write in blocking mode writes everything unless the device
            // errors, in which case it returns a short count and sio_eof is set.
            let mut off = 0usize;
            while off < chunk.len() {
                let n = unsafe {
                    sio_write(
                        hdl,
                        chunk[off..].as_ptr() as *const c_void,
                        chunk.len() - off,
                    )
                };
                if n == 0 {
                    if unsafe { sio_eof(hdl) } != 0 {
                        tracing::warn!("pcm-sndio: sio_write hit eof, reopening");
                        sio = None;
                    }
                    break;
                }
                off += n;
            }
        }
    }

    // ── C ABI — called from firmware/target/hosted/headless/pcm-sndio.c ──────────

    #[no_mangle]
    pub extern "C" fn pcm_sndio_init() {
        let _ = ring();
        let _ = current_rate();
        let _ = writer_handle();
    }

    #[no_mangle]
    pub extern "C" fn pcm_sndio_postinit() {
        if WRITER_ALIVE.swap(true, Ordering::Relaxed) {
            return; // already running
        }
        let rate = *current_rate().lock().unwrap();
        let mut guard = writer_handle().lock().unwrap();
        *guard = Some(
            std::thread::Builder::new()
                .name("rockbox-sndio".into())
                .spawn(move || run_writer(rate))
                .expect("spawn sndio writer thread"),
        );
    }

    #[no_mangle]
    pub extern "C" fn pcm_sndio_set_sample_rate(rate_hz: u32) {
        *current_rate().lock().unwrap() = rate_hz;
    }

    /// Push `size` bytes of S16LE stereo PCM from the firmware DMA thread.
    /// Blocks when the ring is full (back-pressure).
    ///
    /// # Safety
    /// `addr` must be valid for `size` bytes for the duration of this call.
    #[no_mangle]
    pub unsafe extern "C" fn pcm_sndio_push(addr: *const u8, size: usize) {
        let data = unsafe { std::slice::from_raw_parts(addr, size) };
        let (lock, cvar) = ring();
        let mut r = lock.lock().unwrap();
        let mut stall_ms: u32 = 0;
        while r.running && r.buf.len() + size > RING_CAPACITY {
            let (new_r, timed_out) = cvar.wait_timeout(r, Duration::from_millis(200)).unwrap();
            r = new_r;
            if timed_out.timed_out() {
                stall_ms += 200;
                if stall_ms >= 3000 {
                    tracing::warn!("pcm-sndio: ring not draining for 3 s, aborting push");
                    r.running = false;
                    cvar.notify_all();
                    return;
                }
            }
        }
        if !r.running {
            return;
        }
        r.buf.extend(data.iter().copied());
        cvar.notify_all();
    }

    #[no_mangle]
    pub extern "C" fn pcm_sndio_start() {
        let (lock, cvar) = ring();
        let mut r = lock.lock().unwrap();
        r.running = true;
        cvar.notify_all();
    }

    #[no_mangle]
    pub extern "C" fn pcm_sndio_stop() {
        let (lock, cvar) = ring();
        let mut r = lock.lock().unwrap();
        r.running = false;
        cvar.notify_all();
    }

    #[no_mangle]
    pub extern "C" fn pcm_sndio_flush() {
        let (lock, cvar) = ring();
        let mut r = lock.lock().unwrap();
        r.buf.clear();
        cvar.notify_all();
    }

    #[no_mangle]
    pub extern "C" fn pcm_sndio_is_running() -> bool {
        ring().0.lock().unwrap().running
    }
}
