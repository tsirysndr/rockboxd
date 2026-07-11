// Direct libasound PCM sink for ARM Linux (arm-unknown-linux-gnueabihf).
// All real code is gated on cfg(target_os = "linux") — libasound is not
// available on macOS or WASM. pcm-alsa.c is only compiled for ARMHFHOST.
//
// Design: open ALSA once in pcm_alsa_postinit() and keep the writer thread
// alive for the lifetime of the daemon. pcm_alsa_start/stop only toggle the
// `running` flag in the ring — no re-open, no thread re-create. This
// eliminates the ~1 s ALSA-open latency that previously caused audible gaps
// every time pcmbuf ran dry during HTTP streaming.
//
// Data flow:
//   firmware DMA thread
//     → pcm_alsa_push(data, size)   (blocks on back-pressure)
//       → ring buffer (VecDeque)
//         ← writer thread drains via snd_pcm_writei when running=true

/// Force-linkage sentinel. Always present so crates/cli can reference it
/// with #[cfg(feature = "alsa-sink")] without needing a cfg(target_os) guard.
pub fn _link_alsa_sink() {}

// ── Linux-only implementation ─────────────────────────────────────────────────

#[cfg(alsa_backend)]
use alsa::pcm::{Access, Format, HwParams, PCM};
#[cfg(alsa_backend)]
use alsa::{Direction, ValueOr};
#[cfg(alsa_backend)]
use std::collections::VecDeque;
#[cfg(alsa_backend)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(alsa_backend)]
use std::sync::{Condvar, Mutex, OnceLock};
#[cfg(alsa_backend)]
use std::thread::JoinHandle;
#[cfg(alsa_backend)]
use std::time::Duration;

#[cfg(alsa_backend)]
const RING_CAPACITY: usize = 512 * 1024; // 512 KB ≈ 3 s at 44.1 kHz stereo S16LE
#[cfg(alsa_backend)]
const PERIOD_FRAMES: alsa::pcm::Frames = 1024; // ~23 ms @ 44.1 kHz
#[cfg(alsa_backend)]
const BUFFER_FRAMES: alsa::pcm::Frames = 8192; // ~185 ms

// ── Ring buffer ───────────────────────────────────────────────────────────────

#[cfg(alsa_backend)]
struct Ring {
    buf: VecDeque<u8>,
    running: bool,
    shutdown: bool, // set once at daemon shutdown to exit the writer thread
}

#[cfg(alsa_backend)]
static RING: OnceLock<(Mutex<Ring>, Condvar)> = OnceLock::new();

#[cfg(alsa_backend)]
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

// ── Writer thread state ───────────────────────────────────────────────────────

#[cfg(alsa_backend)]
static CURRENT_RATE: OnceLock<Mutex<u32>> = OnceLock::new();
#[cfg(alsa_backend)]
static WRITER_HANDLE: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();
// Set to true while the writer thread is alive so pcm_alsa_postinit is idempotent.
#[cfg(alsa_backend)]
static WRITER_ALIVE: AtomicBool = AtomicBool::new(false);

#[cfg(alsa_backend)]
fn current_rate() -> &'static Mutex<u32> {
    CURRENT_RATE.get_or_init(|| Mutex::new(44100))
}

#[cfg(alsa_backend)]
fn writer_handle() -> &'static Mutex<Option<JoinHandle<()>>> {
    WRITER_HANDLE.get_or_init(|| Mutex::new(None))
}

// ── ALSA helpers ──────────────────────────────────────────────────────────────

#[cfg(alsa_backend)]
fn open_pcm(rate: u32) -> Option<PCM> {
    let pcm = match PCM::new("default", Direction::Playback, false) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("pcm-alsa: open 'default' failed: {e}");
            return None;
        }
    };
    {
        let hwp = match HwParams::any(&pcm) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!("pcm-alsa: HwParams::any failed: {e}");
                return None;
            }
        };
        if hwp.set_access(Access::RWInterleaved).is_err()
            || hwp.set_format(Format::s16()).is_err()
            || hwp.set_channels(2).is_err()
            || hwp.set_rate(rate, ValueOr::Nearest).is_err()
        {
            tracing::error!("pcm-alsa: HwParams setup failed for {rate} Hz");
            return None;
        }
        let _ = hwp.set_buffer_size_near(BUFFER_FRAMES);
        let _ = hwp.set_period_size_near(PERIOD_FRAMES, ValueOr::Nearest);
        if let Err(e) = pcm.hw_params(&hwp) {
            tracing::error!("pcm-alsa: hw_params apply failed: {e}");
            return None;
        }
    }
    if let Err(e) = pcm.prepare() {
        tracing::error!("pcm-alsa: prepare failed: {e}");
        return None;
    }
    tracing::info!("pcm-alsa: opened 'default' at {rate} Hz stereo S16LE");
    Some(pcm)
}

// ── Writer thread — lives for the daemon lifetime ─────────────────────────────

#[cfg(alsa_backend)]
fn run_writer(initial_rate: u32) {
    let mut pcm: Option<PCM> = open_pcm(initial_rate);
    let mut rate = initial_rate;
    tracing::info!("pcm-alsa: writer thread started");

    loop {
        // Wait until running=true with data, OR until shutdown.
        let chunk: Vec<u8> = {
            let (lock, cvar) = ring();
            let mut r = lock.lock().unwrap();
            loop {
                if r.shutdown {
                    if let Some(p) = pcm.take() {
                        let _ = p.drain();
                    }
                    tracing::info!("pcm-alsa: writer thread shutting down");
                    WRITER_ALIVE.store(false, Ordering::Relaxed);
                    return;
                }
                if r.running && r.buf.len() >= 4 {
                    break;
                }
                // Pause: running=false or buffer empty — wait for more data or
                // a start() / shutdown() signal. ALSA stays open; any underrun
                // is recovered transparently by try_recover on next write.
                r = cvar.wait(r).unwrap();
            }
            let n = r.buf.len().min(PERIOD_FRAMES as usize * 4 * 4);
            r.buf.drain(..n).collect()
        };
        ring().1.notify_all(); // signal push() that ring space freed

        if chunk.is_empty() {
            continue;
        }

        // Reopen ALSA if the sample rate changed (rare; no gap for same rate).
        let new_rate = *current_rate().lock().unwrap();
        if new_rate != rate || pcm.is_none() {
            if let Some(old) = pcm.take() {
                let _ = old.drain();
            }
            rate = new_rate;
            pcm = open_pcm(rate);
        }

        let p = match pcm.as_ref() {
            Some(p) => p,
            None => continue,
        };

        let mut reopen = false;
        match p.io_i16() {
            Ok(io) => {
                let samples_i16 = unsafe {
                    std::slice::from_raw_parts(chunk.as_ptr() as *const i16, chunk.len() / 2)
                };
                let frames_total = samples_i16.len() / 2;
                let mut offset = 0usize;
                while offset < frames_total {
                    match io.writei(&samples_i16[offset * 2..]) {
                        Ok(n) => offset += n,
                        Err(e) => {
                            tracing::warn!("pcm-alsa: writei at frame {offset}: {e}, recovering");
                            if let Err(re) = p.try_recover(e, true) {
                                tracing::error!("pcm-alsa: recover failed: {re}");
                                reopen = true;
                                break;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("pcm-alsa: io_i16: {e}");
                reopen = true;
            }
        }

        if reopen {
            pcm = None;
        }
    }
}

// ── C ABI — called from firmware/target/hosted/headless/pcm-alsa.c ───────────

#[cfg(alsa_backend)]
#[no_mangle]
pub extern "C" fn pcm_alsa_init() {
    let _ = ring();
    let _ = current_rate();
    let _ = writer_handle();
}

/// Open ALSA and start the persistent writer thread. Called once after
/// kernel init; subsequent calls are no-ops if the thread is already alive.
#[cfg(alsa_backend)]
#[no_mangle]
pub extern "C" fn pcm_alsa_postinit() {
    if WRITER_ALIVE.swap(true, Ordering::Relaxed) {
        return; // already running
    }
    let rate = *current_rate().lock().unwrap();
    let mut guard = writer_handle().lock().unwrap();
    *guard = Some(
        std::thread::Builder::new()
            .name("rockbox-alsa".into())
            .spawn(move || run_writer(rate))
            .expect("spawn alsa writer thread"),
    );
}

#[cfg(alsa_backend)]
#[no_mangle]
pub extern "C" fn pcm_alsa_set_sample_rate(rate_hz: u32) {
    *current_rate().lock().unwrap() = rate_hz;
}

/// Push `size` bytes of S16LE stereo PCM from the firmware DMA thread.
/// Blocks when the ring is full (back-pressure). Returns immediately if
/// the ring is stopped (running=false) so the DMA thread can exit cleanly.
///
/// # Safety
/// `addr` must be valid for `size` bytes for the duration of this call.
#[cfg(alsa_backend)]
#[no_mangle]
pub unsafe extern "C" fn pcm_alsa_push(addr: *const u8, size: usize) {
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
                tracing::warn!("pcm-alsa: ring not draining for 3 s, aborting push");
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

/// Arm the ring for playback. The persistent writer thread wakes up and
/// starts draining immediately — no ALSA re-open, no thread creation.
#[cfg(alsa_backend)]
#[no_mangle]
pub extern "C" fn pcm_alsa_start() {
    let (lock, cvar) = ring();
    let mut r = lock.lock().unwrap();
    r.running = true;
    cvar.notify_all();
}

/// Pause the ring. The writer thread sees running=false and blocks on the
/// condvar; ALSA stays open. Any underrun on next write is recovered by
/// try_recover, typically transparent to the listener.
#[cfg(alsa_backend)]
#[no_mangle]
pub extern "C" fn pcm_alsa_stop() {
    let (lock, cvar) = ring();
    let mut r = lock.lock().unwrap();
    r.running = false;
    cvar.notify_all();
}

#[cfg(alsa_backend)]
#[no_mangle]
pub extern "C" fn pcm_alsa_flush() {
    let (lock, cvar) = ring();
    let mut r = lock.lock().unwrap();
    r.buf.clear();
    cvar.notify_all();
}

#[cfg(alsa_backend)]
#[no_mangle]
pub extern "C" fn pcm_alsa_is_running() -> bool {
    ring().0.lock().unwrap().running
}
