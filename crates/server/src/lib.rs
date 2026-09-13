use anyhow::Error;
use tracing::{error, warn};

use lazy_static::lazy_static;
use rockbox_graphql::{
    schema::objects::{self, audio_status::AudioStatus, track::Track},
    simplebroker::SimpleBroker,
};
use rockbox_library::repo;
use rockbox_mpd::MpdServer;
use rockbox_sys::{self as rb, types::mp3_entry::Mp3Entry};
use sqlx::{Pool, Sqlite};
use std::{
    collections::{HashMap, HashSet},
    ffi::{c_char, c_int},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

// Set by playlist mutation HTTP handlers so the broker emits a StreamPlaylist
// event on the next tick even when index and amount haven't changed (e.g. shuffle).
pub(crate) static PLAYLIST_DIRTY: AtomicBool = AtomicBool::new(false);

pub mod analysis;
pub mod cache;
pub mod handlers;
pub mod http;
pub mod kv;
pub mod player_events;
pub mod rlimit;
pub mod scan;

// Force netstream FFI symbols into the staticlib output.
// These functions are called from C code (streamfd.c) but not from any Rust
// code, so rustc would otherwise drop the entire crate from librockbox_server.a.
#[allow(dead_code)]
mod _netstream {
    use rbnetstream::{rb_net_close, rb_net_len, rb_net_lseek, rb_net_open, rb_net_read};
    use std::ffi::{c_char, c_void};
    #[used]
    static FN_OPEN: unsafe extern "C" fn(*const c_char) -> i32 = rb_net_open;
    #[used]
    static FN_READ: unsafe extern "C" fn(i32, *mut c_void, usize) -> i64 = rb_net_read;
    #[used]
    static FN_LSEEK: extern "C" fn(i32, i64, i32) -> i64 = rb_net_lseek;
    #[used]
    static FN_LEN: extern "C" fn(i32) -> i64 = rb_net_len;
    #[used]
    static FN_CLOSE: extern "C" fn(i32) = rb_net_close;
}

pub const AUDIO_EXTENSIONS: [&str; 17] = [
    "mp3", "ogg", "flac", "m4a", "aac", "mp4", "alac", "wav", "wv", "mpc", "aiff", "ac3", "opus",
    "spx", "sid", "ape", "wma",
];

lazy_static! {
    pub static ref GLOBAL_MUTEX: Mutex<i32> = Mutex::new(0);
}

#[no_mangle]
pub extern "C" fn debugfn(args: *const c_char, value: c_int) {
    let c_str = unsafe { std::ffi::CStr::from_ptr(args) };
    let str_slice = c_str.to_str().unwrap();
    tracing::debug!("{} {}", str_slice, value);
}

#[no_mangle]
pub extern "C" fn start_server() {
    match rockbox_settings::load_settings(None) {
        Ok(_) => {}
        Err(e) => {
            warn!("Warning loading settings: {}", e);
        }
    }

    // Pre-initialize the UPnP tokio runtime before any other runtime starts.
    rockbox_upnp::init();

    // Run the HTTP server in its own Rust OS thread so actix gets a proper
    // multi-worker runtime instead of collapsing onto the Rockbox C thread.
    //
    // If run_http_server() returns Err, exit the whole process — the alternative
    // (log + thread exits, rest of rockbox keeps running) leaves :6063 silently
    // dead, which surfaces downstream as "error sending request for url
    // (http://127.0.0.1:6063/...)" from the GraphQL crate. Hard-exit so the
    // container restarts cleanly.
    thread::spawn(
        || match actix_rt::System::new().block_on(run_http_server()) {
            Ok(_) => {}
            Err(e) => {
                error!("Error starting HTTP server: {} — exiting", e);
                std::process::exit(1);
            }
        },
    );

    // Yield to the Rockbox cooperative scheduler periodically. Without this
    // the server_thread holds the Rockbox CPU token indefinitely and starves
    // every other kernel thread (broker, gRPC, etc.). The old accept loop
    // called rb::system::sleep(rb::HZ) on every idle iteration; we replicate
    // that contract here.
    loop {
        thread::sleep(std::time::Duration::from_millis(100));
        rb::system::sleep(rb::HZ);
    }
}

async fn run_http_server() -> Result<(), Error> {
    use crate::http::AppState;
    use actix_cors::Cors;
    use actix_web::{web, App, HttpServer};
    use rockbox_types::device::Device;

    let port = std::env::var("ROCKBOX_TCP_PORT").unwrap_or_else(|_| "6063".to_string());
    let addr = format!("0.0.0.0:{}", port);

    let pool = rockbox_library::create_connection_pool().await?;
    let fs_cache = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let metadata_cache = Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let devices = Arc::new(Mutex::new(scan::virtual_devices()));

    let current_device = {
        let active = rockbox_settings::read_settings().ok().and_then(|s| {
            let output = s.audio_output.as_deref().unwrap_or("builtin");
            let mut device = match output {
                "builtin" | "fifo" => scan::virtual_devices()
                    .into_iter()
                    .find(|d| d.service == output),
                "airplay" => {
                    let host = s.airplay_host.clone().unwrap_or_default();
                    Some(Device {
                        id: format!("airplay-{}", host),
                        name: if host.is_empty() {
                            "AirPlay".to_string()
                        } else {
                            format!("AirPlay ({})", host)
                        },
                        host: host.clone(),
                        ip: host,
                        port: s.airplay_port.unwrap_or(5000),
                        service: "airplay".to_string(),
                        app: "AirPlay".to_string(),
                        ..Default::default()
                    })
                }
                "squeezelite" => Some(Device {
                    id: "squeezelite".to_string(),
                    name: "Squeezelite".to_string(),
                    host: "localhost".to_string(),
                    ip: "127.0.0.1".to_string(),
                    port: s.squeezelite_port.unwrap_or(3483),
                    service: "squeezelite".to_string(),
                    app: "squeezelite".to_string(),
                    ..Default::default()
                }),
                "upnp" => {
                    let url = s.upnp_renderer_url.clone().unwrap_or_default();
                    Some(Device {
                        id: format!("upnp-{:.8}", format!("{:x}", md5::compute(url.as_bytes()))),
                        name: "UPnP/DLNA".to_string(),
                        host: "localhost".to_string(),
                        ip: "127.0.0.1".to_string(),
                        port: 0,
                        service: "upnp".to_string(),
                        app: "upnp".to_string(),
                        base_url: Some(url),
                        ..Default::default()
                    })
                }
                "chromecast" => {
                    let host = s.chromecast_host.clone().unwrap_or_default();
                    Some(Device {
                        id: format!("chromecast-{}", host),
                        name: if host.is_empty() {
                            "Chromecast".to_string()
                        } else {
                            format!("Chromecast ({})", host)
                        },
                        host: host.clone(),
                        ip: host,
                        port: s.chromecast_port.unwrap_or(8009),
                        service: "chromecast".to_string(),
                        app: "Chromecast".to_string(),
                        is_cast_device: true,
                        ..Default::default()
                    })
                }
                "cmaf" | "hls" | "dash" => Some(Device {
                    id: "cmaf".to_string(),
                    name: "HLS / DASH Stream".to_string(),
                    host: "localhost".to_string(),
                    ip: "127.0.0.1".to_string(),
                    port: s.cmaf_http_port.unwrap_or(7882),
                    service: "cmaf".to_string(),
                    app: "CMAF".to_string(),
                    ..Default::default()
                }),
                "snapcast_tcp" => {
                    let host = s.snapcast_tcp_host.clone().unwrap_or_default();
                    Some(Device {
                        id: format!("snapcast-{}", host),
                        name: if host.is_empty() {
                            "Snapcast".to_string()
                        } else {
                            format!("Snapcast ({})", host)
                        },
                        host: host.clone(),
                        ip: host,
                        port: s.snapcast_tcp_port.unwrap_or(4953),
                        service: "snapcast".to_string(),
                        app: "Snapcast".to_string(),
                        is_cast_device: true,
                        ..Default::default()
                    })
                }
                _ => scan::virtual_devices()
                    .into_iter()
                    .find(|d| d.service == "builtin"),
            };
            if let Some(ref mut d) = device {
                d.is_current_device = true;
            }
            device
        });
        Arc::new(Mutex::new(active))
    };

    let player = Arc::new(Mutex::new(None));
    let kv = Arc::new(Mutex::new(kv::build_tracks_kv(pool.clone()).await?));

    let playlist_store = rockbox_playlists::PlaylistStore::new(pool.clone());
    playlist_store.seed().await?;

    scan::scan_chromecast_devices(devices.clone());
    scan::scan_upnp_devices(devices.clone());
    scan::scan_airplay_devices(devices.clone());
    scan::scan_snapcast_servers(devices.clone());
    scan::scan_squeezelite_clients(devices.clone());
    player_events::listen_for_playback_changes(player.clone(), pool.clone());
    // Key/BPM backfill — one file a second until every local track has a key.
    analysis::start(pool.clone());

    let state = web::Data::new(AppState {
        pool,
        fs_cache,
        metadata_cache,
        devices,
        current_device,
        player,
        kv,
        playlist_store,
    });

    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .app_data(state.clone())
            .wrap(cors)
            // Albums — fixed routes before parametric
            .route(
                "/albums/filter",
                web::post().to(handlers::albums::filter_albums),
            )
            .route("/albums", web::get().to(handlers::albums::get_albums))
            .route("/albums/{id}", web::get().to(handlers::albums::get_album))
            .route(
                "/albums/{id}/tracks",
                web::get().to(handlers::albums::get_album_tracks),
            )
            // Artists — fixed routes before parametric
            .route(
                "/artists/filter",
                web::post().to(handlers::artists::filter_artists),
            )
            .route("/artists", web::get().to(handlers::artists::get_artists))
            .route(
                "/artists/{id}",
                web::get().to(handlers::artists::get_artist),
            )
            .route(
                "/artists/{id}/albums",
                web::get().to(handlers::artists::get_artist_albums),
            )
            .route(
                "/artists/{id}/tracks",
                web::get().to(handlers::artists::get_artist_tracks),
            )
            // Browse
            .route(
                "/browse/tree-entries",
                web::get().to(handlers::browse::get_tree_entries),
            )
            // Genres
            .route("/genres", web::get().to(handlers::genres::get_genres))
            .route("/genres/{id}", web::get().to(handlers::genres::get_genre))
            .route(
                "/genres/{id}/tracks",
                web::get().to(handlers::genres::get_genre_tracks),
            )
            .route(
                "/genres/{id}/albums",
                web::get().to(handlers::genres::get_genre_albums),
            )
            .route(
                "/genres/{id}/artists",
                web::get().to(handlers::genres::get_genre_artists),
            )
            // Player
            .route(
                "/player",
                web::get().to(handlers::player::get_current_player),
            )
            .route("/player/load", web::put().to(handlers::player::load))
            .route("/player/play", web::put().to(handlers::player::play))
            .route("/player/pause", web::put().to(handlers::player::pause))
            .route("/player/resume", web::put().to(handlers::player::resume))
            .route(
                "/player/ff-rewind",
                web::put().to(handlers::player::ff_rewind),
            )
            .route("/player/status", web::get().to(handlers::player::status))
            .route(
                "/player/current-track",
                web::get().to(handlers::player::current_track),
            )
            .route(
                "/player/next-track",
                web::get().to(handlers::player::next_track),
            )
            .route(
                "/player/flush-and-reload-tracks",
                web::put().to(handlers::player::flush_and_reload_tracks),
            )
            .route("/player/next", web::put().to(handlers::player::next))
            .route(
                "/player/previous",
                web::put().to(handlers::player::previous),
            )
            .route("/player/stop", web::put().to(handlers::player::stop))
            .route(
                "/player/file-position",
                web::get().to(handlers::player::get_file_position),
            )
            .route(
                "/player/volume",
                web::get().to(handlers::player::get_volume),
            )
            .route(
                "/player/volume",
                web::put().to(handlers::player::adjust_volume),
            )
            .route("/player/eq", web::put().to(handlers::dsp::set_eq))
            .route(
                "/player/crossfeed",
                web::put().to(handlers::dsp::set_crossfeed),
            )
            .route(
                "/player/dithering",
                web::put().to(handlers::dsp::set_dithering),
            )
            .route("/player/afr", web::put().to(handlers::dsp::set_afr))
            .route("/player/pbe", web::put().to(handlers::dsp::set_pbe))
            // Playlists — fixed routes before parametric ones
            .route(
                "/playlists/start",
                web::put().to(handlers::playlists::start_playlist),
            )
            .route(
                "/playlists/shuffle",
                web::put().to(handlers::playlists::shuffle_playlist),
            )
            .route(
                "/playlists/amount",
                web::get().to(handlers::playlists::get_playlist_amount),
            )
            .route(
                "/playlists/resume",
                web::put().to(handlers::playlists::resume_playlist),
            )
            .route(
                "/playlists/resume-track",
                web::put().to(handlers::playlists::resume_track),
            )
            .route(
                "/playlists",
                web::post().to(handlers::playlists::create_playlist),
            )
            .route(
                "/playlists/{id}/tracks",
                web::get().to(handlers::playlists::get_playlist_tracks),
            )
            .route(
                "/playlists/{id}/tracks",
                web::post().to(handlers::playlists::insert_tracks),
            )
            .route(
                "/playlists/{id}/tracks",
                web::delete().to(handlers::playlists::remove_tracks),
            )
            .route(
                "/playlists/{id}",
                web::get().to(handlers::playlists::get_playlist),
            )
            // Saved playlists — fixed routes before parametric ones
            .route(
                "/saved-playlists/folders",
                web::get().to(handlers::saved_playlists::list_playlist_folders),
            )
            .route(
                "/saved-playlists/folders",
                web::post().to(handlers::saved_playlists::create_playlist_folder),
            )
            .route(
                "/saved-playlists/folders/{id}",
                web::delete().to(handlers::saved_playlists::delete_playlist_folder),
            )
            .route(
                "/saved-playlists",
                web::get().to(handlers::saved_playlists::list_saved_playlists),
            )
            .route(
                "/saved-playlists",
                web::post().to(handlers::saved_playlists::create_saved_playlist),
            )
            .route(
                "/saved-playlists/{id}/tracks",
                web::get().to(handlers::saved_playlists::get_saved_playlist_tracks),
            )
            .route(
                "/saved-playlists/{id}/track-ids",
                web::get().to(handlers::saved_playlists::get_saved_playlist_track_ids),
            )
            .route(
                "/saved-playlists/{id}/tracks",
                web::post().to(handlers::saved_playlists::add_tracks_to_saved_playlist),
            )
            .route(
                "/saved-playlists/{id}/tracks/{track_id}",
                web::delete().to(handlers::saved_playlists::remove_track_from_saved_playlist),
            )
            .route(
                "/saved-playlists/{id}/play",
                web::post().to(handlers::saved_playlists::play_saved_playlist),
            )
            .route(
                "/saved-playlists/{id}",
                web::get().to(handlers::saved_playlists::get_saved_playlist),
            )
            .route(
                "/saved-playlists/{id}",
                web::put().to(handlers::saved_playlists::update_saved_playlist),
            )
            .route(
                "/saved-playlists/{id}",
                web::delete().to(handlers::saved_playlists::delete_saved_playlist),
            )
            // Smart playlists
            .route(
                "/smart-playlists",
                web::get().to(handlers::smart_playlists::list_smart_playlists),
            )
            .route(
                "/smart-playlists",
                web::post().to(handlers::smart_playlists::create_smart_playlist),
            )
            .route(
                "/smart-playlists/{id}/tracks",
                web::get().to(handlers::smart_playlists::get_smart_playlist_tracks),
            )
            .route(
                "/smart-playlists/{id}/play",
                web::post().to(handlers::smart_playlists::play_smart_playlist),
            )
            .route(
                "/smart-playlists/{id}",
                web::get().to(handlers::smart_playlists::get_smart_playlist),
            )
            .route(
                "/smart-playlists/{id}",
                web::put().to(handlers::smart_playlists::update_smart_playlist),
            )
            .route(
                "/smart-playlists/{id}",
                web::delete().to(handlers::smart_playlists::delete_smart_playlist),
            )
            // Track stats
            .route(
                "/track-stats/{id}/played",
                web::post().to(handlers::smart_playlists::record_track_played),
            )
            .route(
                "/track-stats/{id}/skipped",
                web::post().to(handlers::smart_playlists::record_track_skipped),
            )
            .route(
                "/track-stats/{id}",
                web::get().to(handlers::smart_playlists::get_track_stats),
            )
            // RSQL filters — QuerySpec body, full rows back
            .route(
                "/rsql/tracks",
                web::post().to(handlers::rsql::filter_tracks),
            )
            .route(
                "/rsql/albums",
                web::post().to(handlers::rsql::filter_albums),
            )
            .route(
                "/rsql/artists",
                web::post().to(handlers::rsql::filter_artists),
            )
            // Analytics — read-only projections of track_stats + play_history
            .route(
                "/analytics/most-played",
                web::get().to(handlers::analytics::most_played),
            )
            .route(
                "/analytics/most-skipped",
                web::get().to(handlers::analytics::most_skipped),
            )
            .route(
                "/analytics/never-played",
                web::get().to(handlers::analytics::never_played),
            )
            .route(
                "/analytics/recently-added",
                web::get().to(handlers::analytics::recently_added),
            )
            .route(
                "/analytics/recently-played",
                web::get().to(handlers::analytics::recently_played),
            )
            // Tracks — fixed route before parametric
            .route(
                "/tracks/stream-metadata",
                web::put().to(handlers::tracks::save_stream_track_metadata),
            )
            .route("/tracks", web::get().to(handlers::tracks::get_tracks))
            .route("/tracks/{id}", web::get().to(handlers::tracks::get_track))
            // System
            .route(
                "/version",
                web::get().to(handlers::system::get_rockbox_version),
            )
            .route("/status", web::get().to(handlers::system::get_status))
            .route(
                "/settings",
                web::get().to(handlers::settings::get_global_settings),
            )
            .route(
                "/settings",
                web::put().to(handlers::settings::update_global_settings),
            )
            .route(
                "/scan-library",
                web::put().to(handlers::system::scan_library),
            )
            .route("/search", web::get().to(handlers::search::search))
            // Devices
            .route("/devices", web::get().to(handlers::devices::get_devices))
            .route(
                "/devices/{id}",
                web::get().to(handlers::devices::get_device),
            )
            .route(
                "/devices/{id}/connect",
                web::put().to(handlers::devices::connect),
            )
            .route(
                "/devices/{id}/disconnect",
                web::put().to(handlers::devices::disconnect),
            )
            // Docs
            .route("/", web::get().to(handlers::docs::index))
            .route("/operations/{id}", web::get().to(handlers::docs::index))
            .route("/schemas/{id}", web::get().to(handlers::docs::index))
            .route("/openapi.json", web::get().to(handlers::docs::get_openapi))
            .configure(bluetooth_routes)
    })
    .workers(http::workers())
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}

fn bluetooth_routes(_cfg: &mut actix_web::web::ServiceConfig) {
    #[cfg(target_os = "linux")]
    {
        let cfg = _cfg;
        cfg.route(
            "/bluetooth/scan",
            actix_web::web::post().to(handlers::bluetooth::scan_bluetooth),
        )
        .route(
            "/bluetooth/devices",
            actix_web::web::get().to(handlers::bluetooth::get_bluetooth_devices),
        )
        .route(
            "/bluetooth/devices/{addr}/connect",
            actix_web::web::put().to(handlers::bluetooth::connect_bluetooth_device),
        )
        .route(
            "/bluetooth/devices/{addr}/disconnect",
            actix_web::web::put().to(handlers::bluetooth::disconnect_bluetooth_device),
        );
    }
}

#[no_mangle]
pub extern "C" fn start_servers() {
    // Before anything binds: the servers below open more descriptors than the
    // 256-fd soft limit macOS hands every process, and actix-server turns the
    // resulting EMFILE into a worker-thread panic that aborts the daemon.
    rlimit::raise_fd_limit();

    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        match runtime.block_on(rockbox_rpc::server::start()) {
            Ok(_) => {}
            Err(e) => {
                error!("Error starting server: {}", e);
            }
        }
    });

    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        match runtime.block_on(rockbox_navidrome::server::start()) {
            Ok(_) => {}
            Err(e) => {
                error!("Error starting Subsonic API server: {}", e);
            }
        }
    });

    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        match runtime.block_on(rockbox_jellyfin::server::start()) {
            Ok(_) => {}
            Err(e) => {
                error!("Error starting Jellyfin API server: {}", e);
            }
        }
    });

    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        match runtime.block_on(rockbox_s3::start()) {
            Ok(_) => {}
            Err(e) => {
                error!("Error starting S3 API server: {}", e);
            }
        }
    });

    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        match runtime.block_on(rockbox_graphql::server::start()) {
            Ok(_) => {}
            Err(e) => {
                error!("Error starting server: {}", e);
            }
        }
    });

    // Wait for the rpc server to start
    thread::sleep(std::time::Duration::from_millis(500));

    rockbox_discovery::register_services();

    #[cfg(target_os = "linux")]
    {
        use rockbox_mpris::MprisServer;
        thread::spawn(
            move || match async_std::task::block_on(MprisServer::start()) {
                Ok(_) => {}
                Err(e) => {
                    error!("Error starting mpris server: {}", e);
                }
            },
        );
    }

    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        match runtime.block_on(MpdServer::start()) {
            Ok(_) => {}
            Err(e) => {
                error!("Error starting mpd server: {}", e);
            }
        }
    });
}

#[no_mangle]
pub extern "C" fn start_broker() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let pool = rt
        .block_on(rockbox_library::create_connection_pool())
        .unwrap();

    let mut metadata_cache: HashMap<String, Mp3Entry> = HashMap::new();
    let mut last_playlist_index: i32 = i32::MIN;
    let mut last_playlist_amount: i32 = i32::MIN;
    let mut last_entries: Vec<Mp3Entry> = Vec::new();
    let mut did_initial_restore = false;

    let mut current_scrobble_track: Option<Track> = None; // The track we are monitoring for scrobble
    let mut scrobbled_tracks: HashSet<String> = HashSet::new(); // Simple unique ID to prevent duplicates (use track.id if available)

    // Track stats auto-recording: detect play/skip on track change
    let playlist_store = rockbox_playlists::PlaylistStore::new(pool.clone());
    let mut last_stats_track_id: Option<String> = None;
    let mut last_stats_elapsed: u64 = 0;
    let mut last_stats_length: u64 = 0;

    // Username for the getNowPlaying Subsonic endpoint — read once at startup.
    let subsonic_username = rockbox_settings::read_settings()
        .ok()
        .and_then(|s| s.subsonic_username)
        .unwrap_or_else(|| "admin".to_string());

    loop {
        let mutex = GLOBAL_MUTEX.lock().unwrap();
        if *mutex == 1 {
            drop(mutex);
            thread::sleep(std::time::Duration::from_millis(100));
            rb::system::sleep(rb::HZ);
            continue;
        }

        drop(mutex);

        // On the first tick after firmware is ready, restore the saved playlist
        // from the control file so the queue is visible before the user presses play.
        if !did_initial_restore {
            did_initial_restore = true;
            let status = rb::system::get_global_status();
            if status.resume_index != -1 && rb::playlist::amount() == 0 {
                rb::playlist::resume();
            }
        }

        let playback_status: AudioStatus = rb::playback::status().into();
        SimpleBroker::publish(playback_status);

        match rb::playback::current_track() {
            Some(current_track) => {
                // audio_current_track()->path can diverge from the playlist
                // filename for HTTP stream tracks.  Use the playlist entry's
                // filename (same key the playlist section uses) so DB lookups
                // hit the right record and album_art is resolved correctly.
                let playlist_index = rb::playlist::index();
                let lookup_path = if playlist_index >= 0 {
                    let info = rb::playlist::get_track_info(playlist_index);
                    if !info.filename.is_empty() {
                        info.filename
                    } else {
                        current_track.path.clone()
                    }
                } else {
                    current_track.path.clone()
                };

                let hash = format!("{:x}", md5::compute(lookup_path.as_bytes()));
                let db_metadata = rt
                    .block_on(repo::track::find_by_md5(pool.clone(), &hash))
                    .ok()
                    .flatten();

                let mut track: Track = current_track.into();
                // audio_current_track()->path can be empty or a tmp path for
                // HTTP stream tracks. Use the playlist entry's URL so the
                // client-side coverArtUrlFromStreamUrl can derive ND cover art.
                if track.path.is_empty() && !lookup_path.is_empty() {
                    track.path = lookup_path.clone();
                }

                if let Some(metadata) = db_metadata {
                    // When the URL-keyed record has no album_art (it was saved
                    // from the HTTP stream which has no embedded art), fall back
                    // to the local track identified by the UUID in the URL path.
                    let album_art = if metadata.album_art.is_some() {
                        metadata.album_art.clone()
                    } else {
                        let uuid = lookup_path.rsplit('/').next().unwrap_or("");
                        if !uuid.is_empty() {
                            rt.block_on(repo::track::find(pool.clone(), uuid))
                                .ok()
                                .flatten()
                                .and_then(|t| t.album_art)
                        } else {
                            None
                        }
                    };
                    track.id = Some(metadata.id.clone());
                    track.album_art = album_art;
                    track.album_id = Some(metadata.album_id);
                    track.artist_id = Some(metadata.artist_id);
                    if track.title.is_empty() {
                        track.title = metadata.title.clone();
                    }
                    if track.artist.is_empty() {
                        track.artist = metadata.artist.clone();
                    }
                    if track.album.is_empty() {
                        track.album = metadata.album.clone();
                    }
                    if track.album_artist.is_empty() {
                        track.album_artist = metadata.album_artist.clone();
                    }
                    SimpleBroker::publish(track.clone());
                    rockbox_navidrome::server::set_now_playing(Some(
                        rockbox_navidrome::server::NowPlayingInfo {
                            track_id: track.id.clone(),
                            title: track.title.clone(),
                            artist: track.artist.clone(),
                            album: track.album.clone(),
                            path: track.path.clone(),
                            elapsed_ms: track.elapsed,
                            length_ms: track.length,
                            album_id: track.album_id.clone(),
                            artist_id: track.artist_id.clone(),
                            album_art: track.album_art.clone(),
                            bitrate: track.bitrate,
                            year: track.year,
                            track_number: track.tracknum,
                            username: subsonic_username.clone(),
                        },
                    ));

                    let track_changed = if let Some(ref current) = current_scrobble_track {
                        current.path != track.path
                    } else {
                        true
                    };

                    if track_changed {
                        // Auto-record play or skip for the previous track (direct DB write,
                        // no HTTP roundtrip — avoids blocking the broker loop).
                        if let Some(prev_id) = last_stats_track_id.take() {
                            if last_stats_length > 10_000 && last_stats_elapsed > 2_000 {
                                let ratio = last_stats_elapsed as f64 / last_stats_length as f64;
                                // The broker knows how much was heard, so the
                                // history row records it — the "was it a skip"
                                // rule can then be revisited over old data.
                                let _ = rt.block_on(playlist_store.record_listen(
                                    &prev_id,
                                    ratio < 0.40,
                                    last_stats_elapsed,
                                    last_stats_length,
                                ));
                            }
                        }
                        current_scrobble_track = Some(track.clone());
                    }

                    // Update tracking state for the current track
                    last_stats_track_id = Some(metadata.id.clone());
                    last_stats_elapsed = track.elapsed;
                    last_stats_length = track.length;

                    // Check progress for scrobbling (only if we have a track to monitor)
                    if let Some(ref monitored_track) = current_scrobble_track {
                        if monitored_track.path == track.path {
                            let elapsed_ms = track.elapsed;
                            let length_ms = track.length;

                            if length_ms > 30_000 &&  // optional: ignore very short tracks per Last.fm rules
                                    elapsed_ms as f64 / length_ms as f64 >= 0.40
                            {
                                if !scrobbled_tracks.contains(&metadata.id) {
                                    let cloned_pool = pool.clone();
                                    let cloned_track = monitored_track.clone();
                                    thread::spawn(move || {
                                        let rt = tokio::runtime::Builder::new_current_thread()
                                            .enable_all()
                                            .build()
                                            .unwrap();
                                        match rt
                                            .block_on(scrobble(cloned_track, cloned_pool.clone()))
                                        {
                                            Ok(_) => {}
                                            Err(e) => error!("{}", e),
                                        }
                                    });
                                    scrobbled_tracks.insert(metadata.id.clone());
                                }
                            } else {
                                scrobbled_tracks.clear();
                            }
                        }
                    }
                } else {
                    // Track not in DB (e.g. an HTTP stream URL not yet scanned).
                    // Still publish so clients receive elapsed/status updates.
                    rockbox_navidrome::server::set_now_playing(Some(
                        rockbox_navidrome::server::NowPlayingInfo {
                            track_id: track.id.clone(),
                            title: track.title.clone(),
                            artist: track.artist.clone(),
                            album: track.album.clone(),
                            path: track.path.clone(),
                            elapsed_ms: track.elapsed,
                            length_ms: track.length,
                            album_id: track.album_id.clone(),
                            artist_id: track.artist_id.clone(),
                            album_art: track.album_art.clone(),
                            bitrate: track.bitrate,
                            year: track.year,
                            track_number: track.tracknum,
                            username: subsonic_username.clone(),
                        },
                    ));
                    SimpleBroker::publish(track);
                }
            }
            None => {
                current_scrobble_track = None; // reset on no track
            }
        };

        // Detect what changed.  Consuming DIRTY here covers mutations (shuffle,
        // insert, remove) that leave amount/index unchanged.
        let amount = rb::playlist::amount();
        let current_index = rb::playlist::index();
        let dirty = PLAYLIST_DIRTY.swap(false, Ordering::Relaxed);

        let index_changed = current_index != last_playlist_index;
        let content_changed = amount != last_playlist_amount || dirty;

        if !index_changed && !content_changed {
            thread::sleep(std::time::Duration::from_millis(100));
            rb::system::sleep(rb::HZ / 10);
            continue;
        }

        last_playlist_index = current_index;
        last_playlist_amount = amount;

        let entries: Vec<Mp3Entry> = if content_changed {
            // Collect filenames while still holding the mutex (no I/O).
            let mut track_infos: Vec<(String, String)> = Vec::with_capacity(amount as usize);
            for i in 0..amount {
                let info = rb::playlist::get_track_info(i);
                let hash = format!("{:x}", md5::compute(info.filename.as_bytes()));
                track_infos.push((hash, info.filename));
            }

            // Build entries, reading from disk only for files not yet in cache.
            let mut built: Vec<Mp3Entry> = Vec::with_capacity(track_infos.len());
            for (hash, filename) in &track_infos {
                if let Some(entry) = metadata_cache.get(hash) {
                    built.push(entry.clone());
                    continue;
                }

                let is_http = filename.starts_with("http://") || filename.starts_with("https://");
                let db_track = rt
                    .block_on(repo::track::find_by_md5(pool.clone(), hash))
                    .unwrap();

                let entry = if is_http {
                    // For HTTP stream URLs, do NOT call get_metadata(-1, url) — that
                    // opens a live HTTP connection for every queued track and blocks
                    // the broker loop. Instead, build the entry from the database
                    // using the saved track metadata keyed by the URL hash.
                    let mut e = Mp3Entry::default();
                    e.path = filename.clone();
                    if let Some(ref t) = db_track {
                        e.title = t.title.clone();
                        e.artist = t.artist.clone();
                        e.album = t.album.clone();
                        e.albumartist = t.album_artist.clone();
                        e.length = t.length as u64;
                        e.bitrate = t.bitrate;
                        e.frequency = t.frequency as u64;
                        // URL-keyed record may have no album_art (stream has no
                        // embedded art). Fall back to the local track by UUID.
                        e.album_art = if t.album_art.is_some() {
                            t.album_art.clone()
                        } else {
                            let uuid = filename.rsplit('/').next().unwrap_or("");
                            if !uuid.is_empty() {
                                rt.block_on(repo::track::find(pool.clone(), uuid))
                                    .ok()
                                    .flatten()
                                    .and_then(|local| local.album_art)
                            } else {
                                None
                            }
                        };
                        e.album_id = Some(t.album_id.clone());
                        e.artist_id = Some(t.artist_id.clone());
                        e.genre_id = Some(t.genre_id.clone());
                        e.id = Some(t.id.clone());
                    }
                    e
                } else {
                    let mut e = rb::metadata::get_metadata(-1, filename);
                    if db_track.is_some() {
                        e.album_art = db_track.as_ref().map(|t| t.album_art.clone()).flatten();
                        e.album_id = db_track.as_ref().map(|t| t.album_id.clone());
                        e.artist_id = db_track.as_ref().map(|t| t.artist_id.clone());
                        e.genre_id = db_track.as_ref().map(|t| t.genre_id.clone());
                    }
                    e
                };

                metadata_cache.insert(hash.clone(), entry.clone());
                built.push(entry);
            }
            last_entries = built.clone();
            built
        } else {
            // Only the current index changed — reuse the cached entry list.
            // No get_track_info loop, no disk reads, no DB queries.
            last_entries.clone()
        };

        SimpleBroker::publish(objects::playlist::Playlist {
            amount,
            index: current_index,
            max_playlist_size: rb::playlist::max_playlist_size(),
            first_index: rb::playlist::first_index(),
            last_insert_pos: rb::playlist::last_insert_pos(),
            seed: rb::playlist::seed(),
            last_shuffled_start: rb::playlist::last_shuffled_start(),
            tracks: entries.into_iter().map(|t| t.into()).collect(),
        });

        thread::sleep(std::time::Duration::from_millis(100));
        rb::system::sleep(rb::HZ);
    }
}

async fn scrobble(track: Track, pool: Pool<Sqlite>) -> Result<(), Error> {
    let album_id = track.album_id.unwrap();
    let track = repo::track::find(pool.clone(), &track.id.unwrap()).await?;
    let album = repo::album::find(pool, &album_id).await?;

    if let Some(track) = track {
        if let Some(album) = album {
            match rockbox_rocksky::scrobble(track, album).await {
                Ok(_) => {}
                Err(e) => error!("Failed to scrobble {}", e),
            };
        }
    }

    Ok(())
}
