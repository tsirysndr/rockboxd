//! Process file-descriptor limit.
//!
//! The daemon opens far more descriptors than a plain CLI tool: five actix
//! HTTP servers (REST, GraphQL, Subsonic, Jellyfin, S3), a tonic gRPC server,
//! the MPD server, an mDNS socket per advertised service, and one SQLite pool
//! per server thread — WAL mode costs three fds per connection (`.db`,
//! `-wal`, `-shm`).
//!
//! macOS starts every process at a 256-fd *soft* limit (`RLIMIT_NOFILE`), and
//! a Finder-launched `.app` has no shell profile to raise it. That is not
//! enough, and the failure is fatal: actix-server unwraps the `EMFILE` when
//! spawning a worker, and a panic on that thread aborts the process.
//!
//! Raising the soft limit toward the hard limit is exactly what login shells
//! and launchd do; we just do it in-process, before any server binds.

/// Descriptors we ask for. Comfortably above what the daemon uses at peak
/// (a few hundred) without being greedy.
#[cfg(unix)]
const TARGET_FDS: libc::rlim_t = 16_384;

/// Raise `RLIMIT_NOFILE` to `TARGET_FDS`, or as close as the kernel allows.
///
/// Never fatal — a daemon that could not raise its limit still runs, it just
/// runs closer to the edge, so failures are logged and swallowed.
#[cfg(unix)]
pub fn raise_fd_limit() {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            tracing::warn!(
                "getrlimit(RLIMIT_NOFILE) failed: {}",
                std::io::Error::last_os_error()
            );
            return;
        }

        let ceiling = hard_ceiling(lim.rlim_max);
        let want = TARGET_FDS.min(ceiling);
        if lim.rlim_cur >= want {
            tracing::debug!("fd limit already {} (>= {want})", lim.rlim_cur);
            return;
        }

        let previous = lim.rlim_cur;
        // Step down on failure: macOS rejects anything above
        // kern.maxfilesperproc, and some sandboxes cap lower still.
        for candidate in [want, 10_240, 4_096, 1_024] {
            if candidate <= previous || candidate > ceiling {
                continue;
            }
            lim.rlim_cur = candidate;
            if libc::setrlimit(libc::RLIMIT_NOFILE, &lim) == 0 {
                tracing::info!("raised fd limit {previous} -> {candidate}");
                return;
            }
        }

        tracing::warn!(
            "could not raise fd limit above {previous}: {}",
            std::io::Error::last_os_error()
        );
    }
}

/// Largest soft limit the kernel will accept.
///
/// macOS reports `rlim_max` as `RLIM_INFINITY` but still refuses any
/// `setrlimit` above `kern.maxfilesperproc`, so ask sysctl for the real cap.
#[cfg(all(unix, target_vendor = "apple"))]
unsafe fn hard_ceiling(rlim_max: libc::rlim_t) -> libc::rlim_t {
    let mut maxfiles: libc::c_int = 0;
    let mut size = std::mem::size_of::<libc::c_int>();
    let ok = libc::sysctlbyname(
        b"kern.maxfilesperproc\0".as_ptr() as *const libc::c_char,
        &mut maxfiles as *mut _ as *mut libc::c_void,
        &mut size,
        std::ptr::null_mut(),
        0,
    ) == 0
        && maxfiles > 0;

    let sysctl_cap = if ok {
        maxfiles as libc::rlim_t
    } else {
        TARGET_FDS
    };
    if rlim_max == libc::RLIM_INFINITY {
        sysctl_cap
    } else {
        rlim_max.min(sysctl_cap)
    }
}

#[cfg(all(unix, not(target_vendor = "apple")))]
unsafe fn hard_ceiling(rlim_max: libc::rlim_t) -> libc::rlim_t {
    if rlim_max == libc::RLIM_INFINITY {
        TARGET_FDS
    } else {
        rlim_max
    }
}

#[cfg(not(unix))]
pub fn raise_fd_limit() {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn soft_limit() -> libc::rlim_t {
        unsafe {
            let mut lim: libc::rlimit = std::mem::zeroed();
            assert_eq!(libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim), 0);
            lim.rlim_cur
        }
    }

    /// Run under a deliberately low limit to see the raise happen:
    ///   bash -c 'ulimit -S -n 256; cargo test -p rockbox-server --lib rlimit'
    ///
    /// Use `-S`. Plain `ulimit -n 256` lowers the *hard* limit too, which no
    /// unprivileged process can raise again — the test then fails for a reason
    /// that has nothing to do with this code. launchd only lowers the soft one.
    #[test]
    fn raises_soft_limit_and_is_idempotent() {
        let before = soft_limit();
        raise_fd_limit();
        let after = soft_limit();

        // Never lowers, and always clears the 256 macOS default the daemon's
        // ~314-descriptor footprint blows through.
        assert!(after >= before, "limit went down: {before} -> {after}");
        assert!(after > 256, "still at or below the macOS default: {after}");

        // A second call is a no-op, not a downgrade — `rb_daemon_start` and
        // `start_servers` both call it.
        raise_fd_limit();
        assert_eq!(soft_limit(), after);
    }
}
