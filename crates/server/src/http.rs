use rockbox_library::entity::track::Track;
use rockbox_playlists::PlaylistStore;
use rockbox_sys::types::{mp3_entry::Mp3Entry, tree::Entry};
use rockbox_traits::Player;
use rockbox_types::device::Device;
use sqlx::Sqlite;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::kv::KV;

/// Worker threads for an actix `HttpServer`.
///
/// Actix defaults to one worker per CPU. The daemon runs five HTTP servers
/// (REST, GraphQL, Subsonic, Jellyfin, S3), so on a 10-core machine that is 50
/// arbiters — each an event loop with its own kqueue/epoll and waker fds —
/// before a single request has been served. These servers are LAN-local and
/// nowhere near CPU-bound; four threads each is ample (and leaves headroom for
/// the stream/cover-art handlers that still read files synchronously on the
/// worker) while keeping the daemon well inside the 256-fd soft limit macOS
/// starts processes with.
///
/// Override with `ROCKBOX_HTTP_WORKERS`.
pub fn workers() -> usize {
    std::env::var("ROCKBOX_HTTP_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(4))
                .unwrap_or(4)
        })
}

pub struct AppState {
    pub pool: sqlx::Pool<Sqlite>,
    pub fs_cache: Arc<tokio::sync::Mutex<HashMap<String, Vec<Entry>>>>,
    pub metadata_cache: Arc<tokio::sync::Mutex<HashMap<String, Mp3Entry>>>,
    pub devices: Arc<Mutex<Vec<Device>>>,
    pub current_device: Arc<Mutex<Option<Device>>>,
    pub player: Arc<Mutex<Option<Box<dyn Player + Send>>>>,
    pub kv: Arc<Mutex<KV<Track>>>,
    pub playlist_store: PlaylistStore,
}
