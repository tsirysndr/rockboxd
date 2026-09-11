//! Desktop embedded daemon — headless/cpal build.
//!
//! Boots the C firmware (`apps/main.c::main_c`) on a dedicated thread.
//! The firmware starts its own kernel threads, one of which calls
//! `crates/server::start_servers` and binds gRPC on 127.0.0.1:6061.
//! After that the in-process `rb_*` gRPC client targets that port.

use std::ffi::CStr;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const STATE_STOPPED: i32 = 0;
const STATE_STARTING: i32 = 1;
const STATE_RUNNING: i32 = 2;

static STATE: AtomicI32 = AtomicI32::new(STATE_STOPPED);
static LOCAL_PORT: AtomicI32 = AtomicI32::new(0);

extern "C" {
    fn main_c() -> c_int;
    fn start_server();
    fn start_servers();
    fn start_broker();
}

#[used]
static _KEEPALIVE_START_SERVER: unsafe extern "C" fn() = start_server;
#[used]
static _KEEPALIVE_START_SERVERS: unsafe extern "C" fn() = start_servers;
#[used]
static _KEEPALIVE_START_BROKER: unsafe extern "C" fn() = start_broker;

#[used]
static _KEEPALIVE_ROCKBOX_SERVER: &[&str] = &rockbox_server::AUDIO_EXTENSIONS;

#[used]
static _KEEPALIVE_AIRPLAY: unsafe extern "C" fn(*const c_char, u16) =
    rockbox_airplay::pcm_airplay_set_host;
#[used]
static _KEEPALIVE_SLIM: extern "C" fn(u16) = rockbox_slim::pcm_squeezelite_set_slim_port;
#[used]
static _KEEPALIVE_CHROMECAST: unsafe extern "C" fn(*const c_char) =
    rockbox_chromecast::pcm::pcm_chromecast_set_device_host;
#[used]
static _KEEPALIVE_UPNP: extern "C" fn(u16) = rockbox_upnp::pcm_upnp_set_http_port;
#[used]
static _KEEPALIVE_CMAF: extern "C" fn(u16) = rockbox_cmaf::pcm_cmaf_set_http_port;
#[used]
static _KEEPALIVE_HLS: fn() = rockbox_hls::_link_hls;
#[used]
static _KEEPALIVE_CPAL: fn() = rockbox_cpal_sink::_link_cpal_sink;

#[used]
static _KEEPALIVE_RB_NET_OPEN: unsafe extern "C" fn(*const c_char) -> i32 =
    rbnetstream::rb_net_open;
#[used]
static _KEEPALIVE_RB_NET_READ: unsafe extern "C" fn(i32, *mut std::ffi::c_void, usize) -> i64 =
    rbnetstream::rb_net_read;
#[used]
static _KEEPALIVE_RB_NET_LEN: extern "C" fn(i32) -> i64 = rbnetstream::rb_net_len;
#[used]
static _KEEPALIVE_RB_NET_LSEEK: extern "C" fn(i32, i64, i32) -> i64 = rbnetstream::rb_net_lseek;
#[used]
static _KEEPALIVE_RB_NET_CLOSE: extern "C" fn(i32) = rbnetstream::rb_net_close;

/// Called by `crates/server` for HTTP-streamed tracks to index metadata.
#[no_mangle]
pub extern "C" fn save_remote_track_metadata(url: *const c_char) -> i32 {
    if url.is_null() {
        tracing::warn!("save_remote_track_metadata: null url");
        return -1;
    }
    let url = unsafe { CStr::from_ptr(url) };
    let url = match url.to_str() {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("save_remote_track_metadata: invalid utf-8: {e}");
            return -1;
        }
    };
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!("save_remote_track_metadata: runtime build failed: {e}");
            return -1;
        }
    };
    match rt.block_on(async {
        let pool = rockbox_library::create_connection_pool().await?;
        rockbox_library::audio_scan::save_audio_metadata(pool, url, None).await
    }) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("save_remote_track_metadata: {e}");
            -1
        }
    }
}

pub(crate) unsafe fn cstr_to_str<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

fn configure_environment(music_dir: &str, device_name: &str) {
    std::env::set_var("ROCKBOX_DEVICE_NAME", device_name);
    std::env::set_var("ROCKBOX_LIBRARY", music_dir);

    if std::env::var_os("ROCKBOX_PORT").is_none() {
        std::env::set_var("ROCKBOX_PORT", "6061");
    }
    if std::env::var_os("ROCKBOX_GRAPHQL_PORT").is_none() {
        std::env::set_var("ROCKBOX_GRAPHQL_PORT", "6062");
    }
    if std::env::var_os("ROCKBOX_TCP_PORT").is_none() {
        std::env::set_var("ROCKBOX_TCP_PORT", "6063");
    }
    if std::env::var_os("ROCKBOX_MPD_PORT").is_none() {
        std::env::set_var("ROCKBOX_MPD_PORT", "6600");
    }
}

fn install_subscriber() {
    std::panic::set_hook(Box::new(|info| {
        tracing::error!("Rust panic: {info}");
    }));
    let default_filter = "info,rockbox_embed=debug,rockbox_server=debug,rockbox_graphql=debug,\
         rockbox_library=debug,rockbox_sys=debug,rockbox_airplay=debug,\
         rockbox_chromecast=debug,rockbox_slim=debug,rockbox_upnp=debug";
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter)),
        )
        .try_init();
}

fn wait_for_grpc(port: u16, deadline: Instant) -> bool {
    let addr: SocketAddr = (Ipv4Addr::LOCALHOST, port).into();
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Boot the embedded rockbox daemon (headless/cpal build).
///
/// Spawns `main_c()` on a dedicated thread and waits up to 30 s for the
/// gRPC server to bind. Returns the gRPC port on success, or a negative
/// error code:
///   -22  invalid input
///   -110 timeout
///   -114 already starting / running
///
/// # Safety
/// `music_dir_ptr` and `device_name_ptr` must be NUL-terminated UTF-8 strings.
/// `music_dir_ptr` may be null (falls back to `$HOME/Music`).
#[no_mangle]
pub unsafe extern "C" fn rb_daemon_start(
    music_dir_ptr: *const c_char,
    device_name_ptr: *const c_char,
) -> c_int {
    if STATE
        .compare_exchange(
            STATE_STOPPED,
            STATE_STARTING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return -114;
    }

    install_subscriber();

    // Before the firmware opens anything: a GUI-launched process inherits
    // launchd's 256-fd soft limit on macOS, which the daemon's servers blow
    // through on boot. `start_servers` raises it too — this just gets it done
    // before firmware init rather than midway through.
    rockbox_server::rlimit::raise_fd_limit();

    let music_dir = if music_dir_ptr.is_null() {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        format!("{home}/Music")
    } else {
        match cstr_to_str(music_dir_ptr) {
            Some(s) => s.to_string(),
            None => {
                STATE.store(STATE_STOPPED, Ordering::Release);
                return -22;
            }
        }
    };

    let device_name = match cstr_to_str(device_name_ptr) {
        Some(s) => s.to_string(),
        None => {
            STATE.store(STATE_STOPPED, Ordering::Release);
            return -22;
        }
    };

    configure_environment(&music_dir, &device_name);

    let port: u16 = std::env::var("ROCKBOX_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(6061);

    tracing::info!("embed: spawning rockbox-engine thread");
    thread::Builder::new()
        .name("rockbox-engine".into())
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            tracing::info!("rockbox-engine: calling main_c()");
            let rc = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe { main_c() }));
            match rc {
                Ok(code) => tracing::warn!("rockbox-engine: main_c returned rc={code}"),
                Err(p) => tracing::error!(
                    "rockbox-engine: PANICKED — {:?}",
                    p.downcast_ref::<&str>()
                        .copied()
                        .or_else(|| p.downcast_ref::<String>().map(|s| s.as_str()))
                        .unwrap_or("<non-string panic>")
                ),
            }
            STATE.store(STATE_STOPPED, Ordering::Release);
            LOCAL_PORT.store(0, Ordering::Release);
        })
        .expect("spawn rockbox-engine thread");

    tracing::info!("embed: waiting up to 30s for gRPC :{port} to bind");
    if !wait_for_grpc(port, Instant::now() + Duration::from_secs(30)) {
        tracing::error!("embed: gRPC did not bind within 30s");
        STATE.store(STATE_STOPPED, Ordering::Release);
        return -110;
    }

    LOCAL_PORT.store(port as i32, Ordering::Release);
    STATE.store(STATE_RUNNING, Ordering::Release);
    tracing::info!("embed: started, gRPC bound :{port}");

    // Point the in-process client at the daemon unless the caller already
    // configured a different URL.
    let default_url = "http://127.0.0.1:6061";
    let already_set = crate::SERVER_URL
        .read()
        .map(|g| g.as_str() != default_url)
        .unwrap_or(false);
    if !already_set {
        let url = format!("http://127.0.0.1:{port}\0");
        let _ = crate::rb_set_server_url(url.as_ptr() as *const c_char);
    }

    let default_http = "http://127.0.0.1:6063";
    let http_set = crate::HTTP_URL
        .read()
        .map(|g| g.as_str() != default_http)
        .unwrap_or(false);
    if !http_set {
        let http_port = std::env::var("ROCKBOX_TCP_PORT").unwrap_or_else(|_| "6063".into());
        let http_url = format!("http://127.0.0.1:{http_port}\0");
        let _ = crate::rb_set_http_url(http_url.as_ptr() as *const c_char);
    }

    spawn_library_scan(false);

    port as i32
}

/// Stop the embedded daemon. Currently a best-effort signal — the firmware
/// does not have a clean shutdown path. Returns 0 if it was running.
#[no_mangle]
pub extern "C" fn rb_daemon_stop() -> c_int {
    match STATE.compare_exchange(
        STATE_RUNNING,
        STATE_STOPPED,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => {
            LOCAL_PORT.store(0, Ordering::Release);
            0
        }
        Err(_) => -1,
    }
}

/// Returns the gRPC port of the running daemon, or 0 if not running.
#[no_mangle]
pub extern "C" fn rb_daemon_port() -> c_int {
    LOCAL_PORT.load(Ordering::Acquire)
}

/// Returns the daemon state: 0 = stopped, 1 = starting, 2 = running.
#[no_mangle]
pub extern "C" fn rb_daemon_state() -> c_int {
    STATE.load(Ordering::Acquire)
}

/// Force a full re-scan of the music library. Returns 0 immediately; scan
/// runs in the background. Returns -1 if the daemon is not running.
#[no_mangle]
pub extern "C" fn rb_rescan_library() -> c_int {
    if STATE.load(Ordering::Acquire) != STATE_RUNNING {
        return -1;
    }
    spawn_library_scan(true);
    0
}

fn spawn_library_scan(force_arg: bool) {
    thread::Builder::new()
        .name("rockbox-library-scan".into())
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            let path = std::env::var("ROCKBOX_LIBRARY").unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                format!("{home}/Music")
            });
            let force = force_arg
                || matches!(
                    std::env::var("ROCKBOX_UPDATE_LIBRARY").as_deref(),
                    Ok("1") | Ok("true")
                );
            tracing::info!("scan: target={path} force={force}");

            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("scan: tokio runtime build failed: {e}");
                    return;
                }
            };

            rt.block_on(async move {
                let pool = match rockbox_library::create_connection_pool().await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::error!("scan: open library DB failed: {e}");
                        return;
                    }
                };
                let count = rockbox_library::repo::track::all(pool.clone())
                    .await
                    .map(|t| t.len())
                    .unwrap_or(0);
                if count > 0 && !force {
                    tracing::info!("scan: library has {count} tracks, skipping (force=false)");
                    return;
                }
                tracing::info!("scan: scanning {path} ...");
                match rockbox_library::audio_scan::scan_audio_files(pool.clone(), path.into()).await
                {
                    Ok(files) => tracing::info!("scan: done, {} files", files.len()),
                    Err(e) => tracing::error!("scan: failed: {e}"),
                }
                if let Err(e) = rockbox_library::artists::update_metadata(pool.clone()).await {
                    tracing::warn!("scan: rocksky enrichment skipped: {e}");
                } else {
                    tracing::info!("scan: rocksky enrichment done");
                }
            });
        })
        .expect("spawn rockbox-library-scan thread");
}
