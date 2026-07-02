//! Jellyfin-compatible HTTP server. Mirrors the routes Finamp / Findroid /
//! Streamyfin / Amcfy / Symfonium hit, audio-only.

pub mod auth;
pub mod cover_art_archive;
pub mod discovery;
pub mod dto;
pub mod enrichment;
pub mod favorites;
pub mod handlers;
pub mod instant_mix;
pub mod lastfm;
pub mod lyrics;
pub mod mapping;
pub mod musicbrainz;
pub mod similar;
pub mod user_data;

use actix_cors::Cors;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use rockbox_library::create_connection_pool;
use rockbox_playlists::PlaylistStore;
use rockbox_settings::read_settings;
use sqlx::{Executor, Pool, Sqlite};
use std::path::PathBuf;
use std::sync::Arc;

pub struct JellyfinState {
    pub pool: Pool<Sqlite>,
    pub username: Arc<String>,
    pub password: Arc<String>,
    pub music_dir: PathBuf,
    pub server_id: String,
    pub server_name: String,
    pub user_id: Arc<String>,
    pub port: u16,
    /// Playlists CRUD backend — same store the Subsonic bridge uses so the
    /// two APIs see one another's writes.
    pub playlist_store: PlaylistStore,
    /// Last.fm client — populated only when `lastfm_api_key` is set in
    /// `settings.toml`. Similar endpoints short-circuit to empty when
    /// this is `None`.
    pub lastfm: Option<lastfm::LastFm>,
    /// MusicBrainz client — populated only when
    /// `musicbrainz_user_agent` is set. Used to canonicalize MBIDs
    /// coming back from Last.fm before local library matching.
    pub musicbrainz: Option<musicbrainz::MusicBrainz>,
    /// Cover Art Archive client — same UA gate as MusicBrainz.
    /// Backs the RemoteImage endpoints.
    pub caa: Option<cover_art_archive::CoverArtArchive>,
}

pub async fn start() -> anyhow::Result<()> {
    let settings = read_settings().unwrap_or_default();
    // Activation is gated on `jellyfin_port` being present in settings.toml.
    // Omit the key entirely to disable the server (the Subsonic gate is
    // separately controlled by `subsonic_password`).
    let Some(port) = settings.jellyfin_port else {
        tracing::info!("Jellyfin server: jellyfin_port not set in settings.toml, server disabled");
        return Ok(());
    };
    let username = settings
        .subsonic_username
        .clone()
        .unwrap_or_else(|| "admin".to_string());
    let password = settings.subsonic_password.clone().unwrap_or_default();
    let server_name = "rockbox".to_string();

    if password.is_empty() {
        tracing::info!(
            "Jellyfin server: subsonic_password not set in settings.toml, server disabled"
        );
        return Ok(());
    }

    let music_dir = std::env::var("ROCKBOX_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(format!("{home}/Music"))
        });

    let pool = create_connection_pool().await?;

    // Apply our migrations on top of the library schema.
    pool.execute(include_str!(
        "../../migrations/20260629000000_jellyfin_tables.sql"
    ))
    .await?;
    pool.execute(include_str!(
        "../../migrations/20260702000000_add_jf_favorites.sql"
    ))
    .await?;
    pool.execute(include_str!(
        "../../migrations/20260702000001_add_jf_user_data.sql"
    ))
    .await?;
    pool.execute(include_str!(
        "../../migrations/20260702000002_add_jf_artist_enrichment.sql"
    ))
    .await?;
    pool.execute(include_str!(
        "../../migrations/20260702000003_add_jf_album_enrichment.sql"
    ))
    .await?;

    let server_id = auth::ensure_server_id(&pool).await?;
    let user_id = mapping::user_guid(&username);
    let addr = format!("0.0.0.0:{port}");
    let playlist_store = PlaylistStore::new(pool.clone());
    let lastfm = lastfm::LastFm::from_key(settings.lastfm_api_key.clone());
    let musicbrainz = musicbrainz::MusicBrainz::from_ua(settings.musicbrainz_user_agent.clone());
    let caa = cover_art_archive::CoverArtArchive::from_ua(settings.musicbrainz_user_agent.clone());
    if lastfm.is_some() {
        tracing::info!("Jellyfin server: Last.fm Similar plugin enabled");
    }
    if musicbrainz.is_some() {
        tracing::info!("Jellyfin server: MusicBrainz canonicalization enabled");
    }
    if caa.is_some() {
        tracing::info!("Jellyfin server: Cover Art Archive plugin enabled");
    }

    let state = web::Data::new(JellyfinState {
        pool,
        username: Arc::new(username),
        password: Arc::new(password),
        music_dir,
        server_id: server_id.clone(),
        server_name: server_name.clone(),
        user_id: Arc::new(user_id),
        port,
        playlist_store,
        lastfm,
        musicbrainz,
        caa,
    });

    tracing::info!("Jellyfin API server listening on {addr} (id={server_id})");

    // Background tasks: mDNS announcement + UDP discovery listener.
    discovery::register_mdns("rockbox", port, &server_id);
    let dname = server_name.clone();
    let did = server_id.clone();
    tokio::spawn(async move {
        if let Err(e) = discovery::run(dname, did, port).await {
            tracing::error!("jellyfin discovery stopped: {e}");
        }
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(Cors::permissive())
            .configure(configure_routes)
            .default_service(web::to(log_unrouted))
    })
    .bind(&addr)?
    .run()
    .await?;

    Ok(())
}

/// Log every request that no registered route matched. Lets us see exactly
/// what URL a client (Moonfin, Findroid, etc.) hits when something appears
/// missing — `tracing::warn!` so it shows up at default RUST_LOG level.
async fn log_unrouted(req: HttpRequest) -> HttpResponse {
    tracing::warn!(
        "jellyfin: 404 {} {}{}",
        req.method(),
        req.path(),
        if req.query_string().is_empty() {
            String::new()
        } else {
            format!("?{}", req.query_string())
        },
    );
    HttpResponse::NotFound().finish()
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.route("/", web::get().to(handlers::index))
        .route("/", web::head().to(handlers::index))
        // System
        .route(
            "/System/Info/Public",
            web::get().to(handlers::system_info_public),
        )
        .route("/System/Info", web::get().to(handlers::system_info))
        .route("/System/Endpoint", web::get().to(handlers::system_endpoint))
        .route(
            "/System/Configuration/branding",
            web::get().to(handlers::branding_config),
        )
        .route(
            "/Branding/Configuration",
            web::get().to(handlers::branding_config),
        )
        .route(
            "/Branding/Css",
            web::get().to(|| async {
                actix_web::HttpResponse::Ok()
                    .content_type("text/css")
                    .body("")
            }),
        )
        // Users / auth — register PascalCase + common lowercase variants.
        .route(
            "/Users/AuthenticateByName",
            web::post().to(handlers::authenticate_by_name),
        )
        .route(
            "/Users/authenticatebyname",
            web::post().to(handlers::authenticate_by_name),
        )
        .route(
            "/Users/authenticateByName",
            web::post().to(handlers::authenticate_by_name),
        )
        .route("/Users/Public", web::get().to(handlers::users_public))
        .route("/Users", web::get().to(handlers::users_list))
        .route("/Users/Me", web::get().to(handlers::users_me))
        .route("/Users/{id}", web::get().to(handlers::user_by_id))
        // Views / libraries
        .route("/Users/{id}/Views", web::get().to(handlers::user_views))
        .route("/UserViews", web::get().to(handlers::user_views_query))
        .route(
            "/Library/MediaFolders",
            web::get().to(handlers::media_folders),
        )
        .route(
            "/Library/VirtualFolders",
            web::get().to(handlers::library_virtual_folders),
        )
        .route("/Library/Refresh", web::post().to(handlers::no_content))
        // Items — specific paths before /Items/{id}.
        .route("/Items", web::get().to(handlers::items))
        .route("/Items/Suggestions", web::get().to(handlers::empty_items))
        .route("/Items/Resume", web::get().to(handlers::empty_items))
        .route("/Items/Latest", web::get().to(handlers::items_latest))
        .route("/Items/Prefixes", web::get().to(handlers::items_prefixes))
        // Item-detail rails that clients probe. We have no extras / similar /
        // intros / chapter markers, so empty results are correct — but they
        // must be ROUTED so they don't show up in the unrouted-404 log.
        .route(
            "/Items/{id}/SpecialFeatures",
            web::get().to(handlers::empty_array),
        )
        .route(
            "/Items/{id}/Ancestors",
            web::get().to(handlers::empty_array),
        )
        // RemoteImages — Cover Art Archive-backed when the MB UA is
        // configured. Empty otherwise. Registered before /Items/{id}.
        .route(
            "/Items/{id}/RemoteImages",
            web::get().to(handlers::remote_images),
        )
        .route(
            "/Items/{id}/RemoteImages/Providers",
            web::get().to(handlers::remote_image_providers),
        )
        .route(
            "/Items/{id}/RemoteImages/Download",
            web::post().to(handlers::download_remote_image),
        )
        // Similar — Last.fm-backed when configured, empty otherwise.
        // Registered here so the specific /Items/{id}/Similar path beats
        // the catch-all /Items/{id} GET below.
        .route(
            "/Items/{id}/Similar",
            web::get().to(handlers::similar_items),
        )
        .route(
            "/Artists/{id}/Similar",
            web::get().to(handlers::similar_artists_endpoint),
        )
        .route(
            "/Albums/{id}/Similar",
            web::get().to(handlers::similar_albums_endpoint),
        )
        .route(
            "/Users/{uid}/Items/{id}/Intros",
            web::get().to(handlers::empty_items),
        )
        .route("/MediaSegments/{id}", web::get().to(handlers::empty_items))
        .route(
            "/Items/{id}/File",
            web::get().to(handlers::item_file_stream),
        )
        .route(
            "/Items/{id}/File",
            web::head().to(handlers::item_file_stream),
        )
        .route(
            "/Items/{id}/Download",
            web::get().to(handlers::item_file_stream),
        )
        // InstantMix — generic (/Items/{id}/InstantMix) plus the legacy
        // per-kind aliases. Registered here so they beat the catch-all
        // /Items/{id} GET below.
        .route(
            "/Items/{id}/InstantMix",
            web::get().to(handlers::instant_mix_by_item),
        )
        .route(
            "/Songs/{id}/InstantMix",
            web::get().to(handlers::instant_mix_songs),
        )
        .route(
            "/Albums/{id}/InstantMix",
            web::get().to(handlers::instant_mix_albums),
        )
        .route(
            "/Artists/InstantMix",
            web::get().to(handlers::instant_mix_artists_query),
        )
        .route(
            "/Artists/{id}/InstantMix",
            web::get().to(handlers::instant_mix_artists),
        )
        .route(
            "/Playlists/{id}/InstantMix",
            web::get().to(handlers::instant_mix_playlists),
        )
        .route(
            "/MusicGenres/{name}/InstantMix",
            web::get().to(handlers::instant_mix_music_genre),
        )
        .route("/Items/{id}", web::get().to(handlers::item_by_id))
        // DELETE /Items/{id} — Jellyfin uses this to delete playlists (they
        // are `BaseItem`s). Tracks/albums/artists are read-only over this API
        // and get 403 here.
        .route("/Items/{id}", web::delete().to(handlers::delete_item))
        .route("/Users/{id}/Items", web::get().to(handlers::user_items))
        .route(
            "/Users/{id}/Items/Resume",
            web::get().to(handlers::empty_items),
        )
        .route(
            "/Users/{id}/Items/Latest",
            web::get().to(handlers::items_latest),
        )
        .route(
            "/Users/{uid}/Items/{id}",
            web::get().to(handlers::user_item_by_id),
        )
        // Legacy /UserItems/* aliases.
        .route("/UserItems/Resume", web::get().to(handlers::empty_items))
        .route("/UserItems/Latest", web::get().to(handlers::empty_array))
        // Search
        .route("/Search/Hints", web::get().to(handlers::search_hints))
        // /Shows/* — no series concept here.
        .route("/Shows/NextUp", web::get().to(handlers::empty_items))
        .route("/Shows/Upcoming", web::get().to(handlers::empty_items))
        // Artists
        .route("/Artists", web::get().to(handlers::artists))
        .route("/Artists/AlbumArtists", web::get().to(handlers::artists))
        // Some clients ask for the artist alpha-jump rail via `/Artists/Prefixes`
        // instead of `/Items/Prefixes?IncludeItemTypes=MusicArtist`. Same data.
        .route(
            "/Artists/Prefixes",
            web::get().to(handlers::artists_prefixes),
        )
        .route("/Artists/{name}", web::get().to(handlers::artist_by_name))
        // Images — register both Items and items (lowercase used by Findroid).
        .route(
            "/Items/{id}/Images/{kind}",
            web::get().to(handlers::item_image),
        )
        .route(
            "/Items/{id}/Images/{kind}/{idx}",
            web::get().to(handlers::item_image_by_index),
        )
        .route(
            "/items/{id}/Images/{kind}",
            web::get().to(handlers::item_image),
        )
        .route(
            "/items/{id}/Images/{kind}/{idx}",
            web::get().to(handlers::item_image_by_index),
        )
        .route(
            "/Items/{id}/Images/{kind}",
            web::head().to(handlers::item_image),
        )
        // Playback
        .route(
            "/Items/{id}/PlaybackInfo",
            web::get().to(handlers::playback_info),
        )
        .route(
            "/Items/{id}/PlaybackInfo",
            web::post().to(handlers::playback_info),
        )
        .route("/Audio/{id}/stream", web::get().to(handlers::audio_stream))
        .route("/Audio/{id}/stream", web::head().to(handlers::audio_stream))
        .route(
            "/Audio/{id}/stream.{ext}",
            web::get().to(handlers::audio_stream_ext),
        )
        .route(
            "/Audio/{id}/stream.{ext}",
            web::head().to(handlers::audio_stream_ext),
        )
        .route(
            "/Audio/{id}/universal",
            web::get().to(handlers::audio_universal),
        )
        .route(
            "/Audio/{id}/universal",
            web::head().to(handlers::audio_universal),
        )
        // Lyrics — sidecar .lrc/.txt next to the audio file. Registered
        // alongside the other /Audio/{id}/… routes; the specific suffix
        // means no risk of shadowing stream/universal.
        .route("/Audio/{id}/Lyrics", web::get().to(handlers::get_lyrics))
        .route(
            "/Audio/{id}/Lyrics",
            web::post().to(handlers::upload_lyrics),
        )
        .route(
            "/Audio/{id}/Lyrics",
            web::delete().to(handlers::delete_lyrics),
        )
        .route(
            "/Audio/{id}/RemoteSearch/Lyrics",
            web::get().to(handlers::remote_search_lyrics),
        )
        .route(
            "/Audio/{id}/RemoteSearch/Lyrics/{lyricId}",
            web::post().to(handlers::remote_download_lyrics),
        )
        .route(
            "/Providers/Lyrics",
            web::get().to(handlers::lyric_providers),
        )
        // Favorites — spec-modern + legacy (pre-10.9) paths. Both accept
        // the same set of item kinds (Audio/MusicAlbum/MusicArtist/Playlist)
        // and return the freshly-updated UserItemDataDto.
        .route(
            "/UserFavoriteItems/{id}",
            web::post().to(handlers::mark_favorite),
        )
        .route(
            "/UserFavoriteItems/{id}",
            web::delete().to(handlers::unmark_favorite),
        )
        .route(
            "/Users/{uid}/FavoriteItems/{id}",
            web::post().to(handlers::mark_favorite_legacy),
        )
        .route(
            "/Users/{uid}/FavoriteItems/{id}",
            web::delete().to(handlers::unmark_favorite_legacy),
        )
        // UserData — GET rolls up IsFavorite + play stats + rating + likes;
        // POST accepts UpdateUserItemDataDto (partial patch — unset fields
        // preserve stored state per spec).
        .route(
            "/UserItems/{id}/UserData",
            web::get().to(handlers::get_user_data),
        )
        .route(
            "/UserItems/{id}/UserData",
            web::post().to(handlers::update_user_data),
        )
        .route(
            "/Users/{uid}/Items/{id}/UserData",
            web::get().to(handlers::get_user_data_legacy),
        )
        .route(
            "/Users/{uid}/Items/{id}/UserData",
            web::post().to(handlers::update_user_data_legacy),
        )
        // Sessions / scrobble
        .route("/Sessions", web::get().to(handlers::sessions_list))
        .route(
            "/Sessions/Capabilities/Full",
            web::post().to(handlers::sessions_capabilities),
        )
        .route(
            "/Sessions/Playing",
            web::post().to(handlers::sessions_playing),
        )
        .route(
            "/Sessions/Playing/Progress",
            web::post().to(handlers::sessions_playing_progress),
        )
        .route(
            "/Sessions/Playing/Stopped",
            web::post().to(handlers::sessions_playing_stopped),
        )
        .route(
            "/Users/{uid}/PlayedItems/{id}",
            web::post().to(handlers::user_played_item),
        )
        .route(
            "/Users/{uid}/PlayedItems/{id}",
            web::delete().to(handlers::user_played_item),
        )
        // ScheduledTasks
        .route("/ScheduledTasks", web::get().to(handlers::empty_array))
        .route(
            "/ScheduledTasks/Running/{id}",
            web::post().to(handlers::no_content),
        )
        .route(
            "/ScheduledTasks/Running/{id}",
            web::delete().to(handlers::no_content),
        )
        // Common probes
        .route(
            "/DisplayPreferences/{id}",
            web::get().to(handlers::displaypreferences),
        )
        // Playlists — modelled after the Jellyfin OpenAPI Playlists tag.
        // Order matters: specific `/Playlists/{id}/…` paths must precede the
        // catch-all `/Playlists/{id}` GET.
        .route("/Playlists", web::get().to(handlers::playlists_list))
        .route(
            "/Playlists",
            web::post().to(handlers::create_playlist_endpoint),
        )
        .route(
            "/Playlists/{id}/Items",
            web::get().to(handlers::playlist_items),
        )
        .route(
            "/Playlists/{id}/Items",
            web::post().to(handlers::add_playlist_items),
        )
        .route(
            "/Playlists/{id}/Items",
            web::delete().to(handlers::remove_playlist_items),
        )
        .route(
            "/Playlists/{id}/Items/{item_id}/Move/{new_index}",
            web::post().to(handlers::move_playlist_item),
        )
        .route(
            "/Playlists/{id}/Users",
            web::get().to(handlers::playlist_users),
        )
        .route(
            "/Playlists/{id}",
            web::get().to(handlers::get_playlist_endpoint),
        )
        .route(
            "/Playlists/{id}",
            web::post().to(handlers::update_playlist_endpoint),
        )
        // Genres — sorted list + per-name lookup + legacy user-scoped
        // variant. Both /Genres and /MusicGenres share the same body
        // since rockbox is audio-only.
        .route("/Genres", web::get().to(handlers::genres))
        .route("/MusicGenres", web::get().to(handlers::genres))
        .route("/Genres/{name}", web::get().to(handlers::genre_by_name))
        .route(
            "/MusicGenres/{name}",
            web::get().to(handlers::genre_by_name),
        )
        .route(
            "/Users/{uid}/Genres/{name}",
            web::get().to(handlers::user_genre_by_name),
        )
        // Filters — clients call these to populate genre / year
        // dropdowns before firing a filtered /Items request.
        .route(
            "/Items/Filters",
            web::get().to(handlers::items_filters_legacy),
        )
        .route("/Items/Filters2", web::get().to(handlers::items_filters2))
        .route(
            "/Users/{uid}/Items/Filters",
            web::get().to(handlers::user_items_filters),
        )
        // Library-summary counts — audio-only server so only song /
        // album / artist counts are populated.
        .route("/Items/Counts", web::get().to(handlers::item_counts))
        // /System/Ping is the canonical Jellyfin heartbeat — plain text body.
        .route("/System/Ping", web::get().to(handlers::system_ping))
        .route("/System/Ping", web::head().to(handlers::system_ping))
        // Endpoints we deliberately 404 but want routed (no log noise):
        //  - /socket: WebSocket live updates; clients fall back to polling
        //  - /Moonfin/Ping: Moonfin's own client-side probe (not in spec)
        //  - /Users/{id}/Images/*: no user avatars stored
        .route("/socket", web::get().to(handlers::not_found))
        .route("/Moonfin/Ping", web::get().to(handlers::not_found))
        .route(
            "/Users/{id}/Images/{kind}",
            web::get().to(handlers::not_found),
        );
}
