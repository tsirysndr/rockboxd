//! Embedded rockboxd boot — mirrors gpui/src/startup.rs.
//!
//! When `librockboxd.a` was linked (embedded_daemon cfg, see build.rs), a
//! missing local daemon is booted in-process via `rb_daemon_start()`. When
//! it wasn't, we only probe: the app then works as a remote control for an
//! externally started rockboxd.

use std::net::TcpStream;
use std::time::{Duration, Instant};

const LOCALHOST_GRPC: &str = "127.0.0.1:6061";
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);

#[cfg(embedded_daemon)]
extern "C" {
    fn rb_daemon_start(
        music_dir_ptr: *const std::os::raw::c_char,
        device_name_ptr: *const std::os::raw::c_char,
    ) -> std::os::raw::c_int;
}

/// Returns true if gRPC port 6061 is already accepting connections.
pub fn is_running() -> bool {
    TcpStream::connect_timeout(&LOCALHOST_GRPC.parse().unwrap(), CONNECT_TIMEOUT).is_ok()
}

/// Ensure a daemon is reachable. Returns the gRPC port (positive) or a
/// negative error code. Blocks up to 30 s while the embedded daemon binds.
pub fn ensure_running() -> i32 {
    // Respect an explicit remote host — never boot a local daemon then.
    if std::env::var("ROCKBOX_HOST").map(|h| h != "127.0.0.1" && h != "localhost") == Ok(true) {
        return 6061;
    }
    if is_running() {
        wait_for_graphql();
        return 6061;
    }
    #[cfg(embedded_daemon)]
    {
        tracing::info!("no local rockboxd — booting embedded daemon");
        let device_name = b"Rockbox Desktop\0".as_ptr() as *const std::os::raw::c_char;
        let port = unsafe { rb_daemon_start(std::ptr::null(), device_name) };
        if port > 0 {
            wait_for_graphql();
        } else {
            tracing::error!("embedded daemon failed to start: {port}");
        }
        port
    }
    #[cfg(not(embedded_daemon))]
    {
        tracing::warn!("no local rockboxd and no embedded daemon linked");
        -1
    }
}

/// Wait up to 10 s for the GraphQL server (album art, covers) to bind.
/// Not fatal — art is just missing until it comes up.
fn wait_for_graphql() {
    let graphql_port = std::env::var("ROCKBOX_GRAPHQL_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(6062);
    let addr: std::net::SocketAddr = (std::net::Ipv4Addr::LOCALHOST, graphql_port).into();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(50)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
