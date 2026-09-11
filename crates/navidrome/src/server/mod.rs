pub mod handlers;
pub mod response;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer};
use rockbox_library::create_connection_pool;
use rockbox_playlists::PlaylistStore;
use rockbox_settings::read_settings;
use sqlx::{Pool, Sqlite};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};

/// Worker threads for this server — see `rockbox_server::http::workers` for
/// why the daemon caps them instead of taking actix's one-per-CPU default.
fn http_workers() -> usize {
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

// ── Now-playing shared state ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NowPlayingInfo {
    pub track_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub path: String,
    pub elapsed_ms: u64,
    pub length_ms: u64,
    pub album_id: Option<String>,
    pub artist_id: Option<String>,
    pub album_art: Option<String>,
    pub bitrate: u32,
    pub year: i32,
    pub track_number: i32,
    pub username: String,
}

static NOW_PLAYING: OnceLock<Mutex<Option<NowPlayingInfo>>> = OnceLock::new();

fn now_playing_lock() -> &'static Mutex<Option<NowPlayingInfo>> {
    NOW_PLAYING.get_or_init(|| Mutex::new(None))
}

/// Called by the rockbox-server broker whenever the current track changes.
pub fn set_now_playing(info: Option<NowPlayingInfo>) {
    if let Ok(mut guard) = now_playing_lock().lock() {
        *guard = info;
    }
}

/// Read the latest now-playing snapshot.
pub fn get_now_playing() -> Option<NowPlayingInfo> {
    now_playing_lock().lock().ok()?.clone()
}

// ─────────────────────────────────────────────────────────────────────────────

pub struct SubsonicState {
    pub pool: Pool<Sqlite>,
    pub playlist_store: PlaylistStore,
    pub username: Arc<String>,
    pub password: Arc<String>,
    pub scan_running: Arc<AtomicBool>,
}

/// Validate Subsonic auth params against configured credentials.
/// Supports token auth (t + s) and plaintext (p, optionally "enc:<hex>").
pub fn check_auth(
    username: &str,
    password: &str,
    u: Option<&str>,
    p: Option<&str>,
    t: Option<&str>,
    s: Option<&str>,
) -> bool {
    let Some(req_user) = u else { return false };
    if req_user != username {
        return false;
    }

    if let (Some(token), Some(salt)) = (t, s) {
        let expected = format!("{:x}", md5::compute(format!("{}{}", password, salt)));
        return token == expected;
    }

    if let Some(plain) = p {
        let decoded = if let Some(hex) = plain.strip_prefix("enc:") {
            hex_decode(hex).unwrap_or_else(|| plain.to_string())
        } else {
            plain.to_string()
        };
        return decoded == password;
    }

    false
}

fn hex_decode(s: &str) -> Option<String> {
    if s.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect();
    bytes.and_then(|b| String::from_utf8(b).ok())
}

pub async fn start() -> anyhow::Result<()> {
    let settings = read_settings().unwrap_or_default();
    let username = settings
        .subsonic_username
        .unwrap_or_else(|| "admin".to_string());
    let password = settings.subsonic_password.unwrap_or_default();
    let port = settings.subsonic_port.unwrap_or(4533);

    if password.is_empty() {
        tracing::info!("Subsonic server: no password configured, server disabled");
        return Ok(());
    }

    let addr = format!("0.0.0.0:{}", port);
    tracing::info!("Subsonic API server listening on {addr}");

    let pool = create_connection_pool().await?;
    let playlist_store = PlaylistStore::new(pool.clone());
    playlist_store.seed().await?;

    let state = web::Data::new(SubsonicState {
        pool,
        playlist_store,
        username: Arc::new(username),
        password: Arc::new(password),
        scan_running: Arc::new(AtomicBool::new(false)),
    });

    HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .app_data(state.clone())
            .wrap(cors)
            // System
            .route("/rest/ping{_:(\\.view)?}", web::get().to(handlers::ping))
            .route("/rest/ping{_:(\\.view)?}", web::post().to(handlers::ping))
            .route(
                "/rest/getUser{_:(\\.view)?}",
                web::get().to(handlers::get_user),
            )
            .route(
                "/rest/getUser{_:(\\.view)?}",
                web::post().to(handlers::get_user),
            )
            .route(
                "/rest/getMusicFolders{_:(\\.view)?}",
                web::get().to(handlers::get_music_folders),
            )
            .route(
                "/rest/getMusicFolders{_:(\\.view)?}",
                web::post().to(handlers::get_music_folders),
            )
            .route(
                "/rest/getScanStatus{_:(\\.view)?}",
                web::get().to(handlers::get_scan_status),
            )
            .route(
                "/rest/getScanStatus{_:(\\.view)?}",
                web::post().to(handlers::get_scan_status),
            )
            .route(
                "/rest/startScan{_:(\\.view)?}",
                web::get().to(handlers::start_scan),
            )
            .route(
                "/rest/startScan{_:(\\.view)?}",
                web::post().to(handlers::start_scan),
            )
            // Library — ID3-tag browsing
            .route(
                "/rest/getArtists{_:(\\.view)?}",
                web::get().to(handlers::get_artists),
            )
            .route(
                "/rest/getArtists{_:(\\.view)?}",
                web::post().to(handlers::get_artists),
            )
            .route(
                "/rest/getArtist{_:(\\.view)?}",
                web::get().to(handlers::get_artist),
            )
            .route(
                "/rest/getArtist{_:(\\.view)?}",
                web::post().to(handlers::get_artist),
            )
            .route(
                "/rest/getAlbum{_:(\\.view)?}",
                web::get().to(handlers::get_album),
            )
            .route(
                "/rest/getAlbum{_:(\\.view)?}",
                web::post().to(handlers::get_album),
            )
            .route(
                "/rest/getSong{_:(\\.view)?}",
                web::get().to(handlers::get_song),
            )
            .route(
                "/rest/getSong{_:(\\.view)?}",
                web::post().to(handlers::get_song),
            )
            // Library — folder browsing (older clients)
            .route(
                "/rest/getIndexes{_:(\\.view)?}",
                web::get().to(handlers::get_indexes),
            )
            .route(
                "/rest/getIndexes{_:(\\.view)?}",
                web::post().to(handlers::get_indexes),
            )
            .route(
                "/rest/getMusicDirectory{_:(\\.view)?}",
                web::get().to(handlers::get_music_directory),
            )
            .route(
                "/rest/getMusicDirectory{_:(\\.view)?}",
                web::post().to(handlers::get_music_directory),
            )
            // Genres
            .route(
                "/rest/getGenres{_:(\\.view)?}",
                web::get().to(handlers::get_genres),
            )
            .route(
                "/rest/getGenres{_:(\\.view)?}",
                web::post().to(handlers::get_genres),
            )
            .route(
                "/rest/getSongsByGenre{_:(\\.view)?}",
                web::get().to(handlers::get_songs_by_genre),
            )
            .route(
                "/rest/getSongsByGenre{_:(\\.view)?}",
                web::post().to(handlers::get_songs_by_genre),
            )
            // Lists
            .route(
                "/rest/getAlbumList2{_:(\\.view)?}",
                web::get().to(handlers::get_album_list2),
            )
            .route(
                "/rest/getAlbumList2{_:(\\.view)?}",
                web::post().to(handlers::get_album_list2),
            )
            .route(
                "/rest/getRandomSongs{_:(\\.view)?}",
                web::get().to(handlers::get_random_songs),
            )
            .route(
                "/rest/getRandomSongs{_:(\\.view)?}",
                web::post().to(handlers::get_random_songs),
            )
            .route(
                "/rest/getStarred2{_:(\\.view)?}",
                web::get().to(handlers::get_starred2),
            )
            .route(
                "/rest/getStarred2{_:(\\.view)?}",
                web::post().to(handlers::get_starred2),
            )
            // Playback
            .route(
                "/rest/stream{_:(\\.view)?}",
                web::get().to(handlers::stream),
            )
            .route(
                "/rest/stream{_:(\\.view)?}",
                web::post().to(handlers::stream),
            )
            .route(
                "/rest/download{_:(\\.view)?}",
                web::get().to(handlers::stream),
            )
            .route(
                "/rest/download{_:(\\.view)?}",
                web::post().to(handlers::stream),
            )
            .route(
                "/rest/getCoverArt{_:(\\.view)?}",
                web::get().to(handlers::get_cover_art),
            )
            .route(
                "/rest/getCoverArt{_:(\\.view)?}",
                web::post().to(handlers::get_cover_art),
            )
            .route(
                "/rest/scrobble{_:(\\.view)?}",
                web::get().to(handlers::scrobble),
            )
            .route(
                "/rest/scrobble{_:(\\.view)?}",
                web::post().to(handlers::scrobble),
            )
            .route(
                "/rest/getNowPlaying{_:(\\.view)?}",
                web::get().to(handlers::get_now_playing),
            )
            .route(
                "/rest/getNowPlaying{_:(\\.view)?}",
                web::post().to(handlers::get_now_playing),
            )
            .route(
                "/rest/updateNowPlaying{_:(\\.view)?}",
                web::get().to(handlers::update_now_playing),
            )
            .route(
                "/rest/updateNowPlaying{_:(\\.view)?}",
                web::post().to(handlers::update_now_playing),
            )
            // Search
            .route(
                "/rest/search3{_:(\\.view)?}",
                web::get().to(handlers::search3),
            )
            .route(
                "/rest/search3{_:(\\.view)?}",
                web::post().to(handlers::search3),
            )
            // Playlists
            .route(
                "/rest/getPlaylists{_:(\\.view)?}",
                web::get().to(handlers::get_playlists),
            )
            .route(
                "/rest/getPlaylists{_:(\\.view)?}",
                web::post().to(handlers::get_playlists),
            )
            .route(
                "/rest/getPlaylist{_:(\\.view)?}",
                web::get().to(handlers::get_playlist),
            )
            .route(
                "/rest/getPlaylist{_:(\\.view)?}",
                web::post().to(handlers::get_playlist),
            )
            .route(
                "/rest/createPlaylist{_:(\\.view)?}",
                web::get().to(handlers::create_playlist),
            )
            .route(
                "/rest/createPlaylist{_:(\\.view)?}",
                web::post().to(handlers::create_playlist),
            )
            .route(
                "/rest/updatePlaylist{_:(\\.view)?}",
                web::get().to(handlers::update_playlist),
            )
            .route(
                "/rest/updatePlaylist{_:(\\.view)?}",
                web::post().to(handlers::update_playlist),
            )
            .route(
                "/rest/deletePlaylist{_:(\\.view)?}",
                web::get().to(handlers::delete_playlist),
            )
            .route(
                "/rest/deletePlaylist{_:(\\.view)?}",
                web::post().to(handlers::delete_playlist),
            )
            // Starring
            .route("/rest/star{_:(\\.view)?}", web::get().to(handlers::star))
            .route("/rest/star{_:(\\.view)?}", web::post().to(handlers::star))
            .route(
                "/rest/unstar{_:(\\.view)?}",
                web::get().to(handlers::unstar),
            )
            .route(
                "/rest/unstar{_:(\\.view)?}",
                web::post().to(handlers::unstar),
            )
            // Artist / album info
            .route(
                "/rest/getArtistInfo{_:(\\.view)?}",
                web::get().to(handlers::get_artist_info),
            )
            .route(
                "/rest/getArtistInfo{_:(\\.view)?}",
                web::post().to(handlers::get_artist_info),
            )
            .route(
                "/rest/getArtistInfo2{_:(\\.view)?}",
                web::get().to(handlers::get_artist_info2),
            )
            .route(
                "/rest/getArtistInfo2{_:(\\.view)?}",
                web::post().to(handlers::get_artist_info2),
            )
            .route(
                "/rest/getAlbumInfo{_:(\\.view)?}",
                web::get().to(handlers::get_album_info2),
            )
            .route(
                "/rest/getAlbumInfo{_:(\\.view)?}",
                web::post().to(handlers::get_album_info2),
            )
            .route(
                "/rest/getAlbumInfo2{_:(\\.view)?}",
                web::get().to(handlers::get_album_info2),
            )
            .route(
                "/rest/getAlbumInfo2{_:(\\.view)?}",
                web::post().to(handlers::get_album_info2),
            )
            .route(
                "/rest/getSimilarSongs{_:(\\.view)?}",
                web::get().to(handlers::get_similar_songs2),
            )
            .route(
                "/rest/getSimilarSongs{_:(\\.view)?}",
                web::post().to(handlers::get_similar_songs2),
            )
            .route(
                "/rest/getSimilarSongs2{_:(\\.view)?}",
                web::get().to(handlers::get_similar_songs2),
            )
            .route(
                "/rest/getSimilarSongs2{_:(\\.view)?}",
                web::post().to(handlers::get_similar_songs2),
            )
            .route(
                "/rest/getTopSongs{_:(\\.view)?}",
                web::get().to(handlers::get_top_songs),
            )
            .route(
                "/rest/getTopSongs{_:(\\.view)?}",
                web::post().to(handlers::get_top_songs),
            )
            .route(
                "/rest/getLyrics{_:(\\.view)?}",
                web::get().to(handlers::get_lyrics),
            )
            .route(
                "/rest/getLyrics{_:(\\.view)?}",
                web::post().to(handlers::get_lyrics),
            )
            // Aliases for older API versions
            .route(
                "/rest/getAlbumList{_:(\\.view)?}",
                web::get().to(handlers::get_album_list),
            )
            .route(
                "/rest/getAlbumList{_:(\\.view)?}",
                web::post().to(handlers::get_album_list),
            )
            .route(
                "/rest/getStarred{_:(\\.view)?}",
                web::get().to(handlers::get_starred),
            )
            .route(
                "/rest/getStarred{_:(\\.view)?}",
                web::post().to(handlers::get_starred),
            )
            .route(
                "/rest/search2{_:(\\.view)?}",
                web::get().to(handlers::search2),
            )
            .route(
                "/rest/search2{_:(\\.view)?}",
                web::post().to(handlers::search2),
            )
    })
    .workers(http_workers())
    .bind(addr)?
    .run()
    .await?;

    Ok(())
}
