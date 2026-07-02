//! Jellyfin endpoint handlers. Audio-only — this server is built on top of
//! rockbox-library's track/album/artist tables.

use actix_web::{web, HttpRequest, HttpResponse};
use chrono::Utc;
use rockbox_library::entity::{album::Album, artist::Artist, track::Track};
use rockbox_library::repo;
use rockbox_playlists::Playlist;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

use super::auth::{self, AuthedUser, EmbyAuth};
use super::dto::{
    AuthenticationResult, BaseItemDto, ImageBlurHashes, ImageProviderInfo, ImageTags, ItemsResult,
    MediaSource, MediaStream, NameGuidPair, PlaybackInfoResponse, PlaylistCreationResult,
    PublicSystemInfo, RemoteImageInfo, RemoteImageResult, SessionInfoDto, SystemInfo,
    UserConfiguration, UserDto, UserItemDataDto, UserPolicy, ViewsResult, JELLYFIN_API_VERSION,
};
use super::mapping;
use super::JellyfinState;

const TICKS_PER_MS: i64 = 10_000;
const TICKS_PER_SEC: i64 = TICKS_PER_MS * 1000;

fn now_iso() -> String {
    let now = Utc::now();
    let ticks = now.timestamp_subsec_nanos() / 100;
    format!("{}.{ticks:07}", now.format("%Y-%m-%dT%H:%M:%S"))
}

fn parse_auth(req: &HttpRequest) -> EmbyAuth {
    for name in ["x-emby-authorization", "authorization"] {
        if let Some(v) = req.headers().get(name) {
            if let Ok(s) = v.to_str() {
                return auth::parse_emby_auth_header(s);
            }
        }
    }
    EmbyAuth::default()
}

fn server_base(state: &JellyfinState, req: &HttpRequest) -> String {
    if let Some(h) = req.headers().get("host").and_then(|v| v.to_str().ok()) {
        let scheme = req.connection_info().scheme().to_string();
        return format!("{scheme}://{h}");
    }
    format!("http://0.0.0.0:{}", state.port)
}

// ── Root index ────────────────────────────────────────────────────────────────

pub async fn index(state: web::Data<JellyfinState>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body(format!(
            "rockbox-jellyfin — Jellyfin-compatible API\nserver: {} ({})\n",
            state.server_name, state.server_id
        ))
}

// ── System ───────────────────────────────────────────────────────────────────

fn jellyfin_os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "Darwin",
        "linux" => "Linux",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "netbsd" => "NetBSD",
        "android" => "Android",
        "ios" => "iOS",
        other => other,
    }
}

fn public_info(state: &JellyfinState, req: &HttpRequest) -> PublicSystemInfo {
    PublicSystemInfo {
        local_address: Some(server_base(state, req)),
        server_name: Some(state.server_name.clone()),
        version: Some(JELLYFIN_API_VERSION.to_string()),
        product_name: Some("Jellyfin Server".to_string()),
        operating_system: Some(jellyfin_os_name().to_string()),
        id: Some(state.server_id.clone()),
        startup_wizard_completed: Some(true),
    }
}

pub async fn system_info_public(state: web::Data<JellyfinState>, req: HttpRequest) -> HttpResponse {
    HttpResponse::Ok().json(public_info(&state, &req))
}

pub async fn system_info(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let pub_info = public_info(&state, &req);
    HttpResponse::Ok().json(SystemInfo {
        local_address: pub_info.local_address,
        server_name: pub_info.server_name,
        version: pub_info.version,
        product_name: pub_info.product_name,
        operating_system: pub_info.operating_system,
        id: pub_info.id,
        startup_wizard_completed: pub_info.startup_wizard_completed,
        operating_system_display_name: Some(jellyfin_os_name().to_string()),
        package_name: Some("rockbox".to_string()),
        has_pending_restart: false,
        is_shutting_down: false,
        supports_library_monitor: true,
        web_socket_port_number: state.port as i32,
        completed_installations: Some(vec![]),
        can_self_restart: false,
        can_launch_web_browser: false,
        program_data_path: Some(String::new()),
        web_path: Some(String::new()),
        items_by_name_path: Some(String::new()),
        cache_path: Some(String::new()),
        log_path: Some(String::new()),
        internal_metadata_path: Some(String::new()),
        transcoding_temp_path: Some(String::new()),
        cast_receiver_applications: Some(vec![]),
        has_update_available: false,
        encoder_location: Some("Default".to_string()),
        system_architecture: Some(std::env::consts::ARCH.to_string()),
    })
}

pub async fn system_endpoint(_user: AuthedUser, req: HttpRequest) -> HttpResponse {
    let info = req.connection_info();
    let addr = info.realip_remote_addr().unwrap_or("");
    HttpResponse::Ok().json(json!({
        "IsLocal": true,
        "IsInNetwork": true,
        "RemoteAddress": addr,
    }))
}

// ── Users / Auth ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticateBody {
    pub username: Option<String>,
    pub pw: Option<String>,
    pub password: Option<String>,
}

fn build_user(state: &JellyfinState) -> UserDto {
    UserDto {
        name: Some(state.username.as_str().to_string()),
        server_id: Some(state.server_id.clone()),
        server_name: Some(state.server_name.clone()),
        id: state.user_id.as_str().to_string(),
        primary_image_tag: None,
        primary_image_aspect_ratio: None,
        has_password: Some(true),
        has_configured_password: Some(true),
        has_configured_easy_password: Some(false),
        enable_auto_login: Some(false),
        last_login_date: Some(now_iso()),
        last_activity_date: Some(now_iso()),
        configuration: Some(UserConfiguration::default()),
        policy: Some(UserPolicy::admin()),
    }
}

pub async fn authenticate_by_name(
    state: web::Data<JellyfinState>,
    req: HttpRequest,
    body: web::Json<AuthenticateBody>,
) -> HttpResponse {
    let body = body.into_inner();
    let username = body.username.unwrap_or_default();
    let password = body.pw.or(body.password).unwrap_or_default();

    if username != *state.username || password != *state.password {
        return HttpResponse::Unauthorized()
            .json(json!({"Message": "Invalid username or password"}));
    }

    let parsed = parse_auth(&req);
    let token = auth::random_hex(16);
    let now = now_iso();
    if let Err(e) =
        auth::store_token(&state.pool, &token, state.user_id.as_str(), &parsed, &now).await
    {
        tracing::error!("jellyfin: store_token: {e}");
        return HttpResponse::InternalServerError().finish();
    }

    let user = build_user(&state);
    let session = SessionInfoDto {
        play_state: None,
        additional_users: Some(vec![]),
        capabilities: None,
        remote_end_point: Some(
            req.connection_info()
                .realip_remote_addr()
                .unwrap_or("")
                .to_string(),
        ),
        playable_media_types: vec!["Audio".into()],
        id: Some(auth::random_hex(16)),
        user_id: user.id.clone(),
        user_name: user.name.clone(),
        client: parsed.client.clone(),
        last_activity_date: now.clone(),
        last_playback_check_in: now.clone(),
        last_paused_date: None,
        device_name: parsed.device.clone(),
        device_type: None,
        now_playing_item: None,
        now_viewing_item: None,
        device_id: parsed.device_id.clone(),
        application_version: parsed.version.clone(),
        transcoding_info: None,
        is_active: true,
        supports_media_control: false,
        supports_remote_control: false,
        now_playing_queue: Some(vec![]),
        has_custom_device_name: false,
        playlist_item_id: None,
        server_id: Some(state.server_id.clone()),
        user_primary_image_tag: None,
        supported_commands: vec![],
    };

    HttpResponse::Ok().json(AuthenticationResult {
        user: Some(user),
        session_info: Some(session),
        access_token: Some(token),
        server_id: Some(state.server_id.clone()),
    })
}

pub async fn users_public(state: web::Data<JellyfinState>) -> HttpResponse {
    HttpResponse::Ok().json(vec![build_user(&state)])
}

pub async fn users_list(_user: AuthedUser, state: web::Data<JellyfinState>) -> HttpResponse {
    HttpResponse::Ok().json(vec![build_user(&state)])
}

pub async fn users_me(_user: AuthedUser, state: web::Data<JellyfinState>) -> HttpResponse {
    HttpResponse::Ok().json(build_user(&state))
}

pub async fn user_by_id(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    _path: web::Path<String>,
) -> HttpResponse {
    HttpResponse::Ok().json(build_user(&state))
}

// ── Views (top-level library list) ───────────────────────────────────────────

fn music_library_view(state: &JellyfinState) -> BaseItemDto {
    BaseItemDto {
        id: mapping::library_guid(),
        server_id: Some(state.server_id.clone()),
        name: Some("Music".to_string()),
        item_type: "CollectionFolder",
        media_type: "Unknown",
        is_folder: Some(true),
        collection_type: Some("music"),
        location_type: Some("FileSystem"),
        ..Default::default()
    }
}

/// Virtual "Playlists" library. Jellyfin's reference server auto-creates
/// this view so clients (Moonfin, Findroid, official web) render playlists
/// as a top-level tile alongside Music. `CollectionType` is the spec enum
/// value `"playlists"` (plural).
fn playlists_library_view(state: &JellyfinState) -> BaseItemDto {
    BaseItemDto {
        id: mapping::playlists_library_guid(),
        server_id: Some(state.server_id.clone()),
        name: Some("Playlists".to_string()),
        item_type: "CollectionFolder",
        media_type: "Unknown",
        is_folder: Some(true),
        collection_type: Some("playlists"),
        location_type: Some("FileSystem"),
        ..Default::default()
    }
}

fn all_library_views(state: &JellyfinState) -> Vec<BaseItemDto> {
    vec![music_library_view(state), playlists_library_view(state)]
}

pub async fn user_views(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    _path: web::Path<String>,
) -> HttpResponse {
    let views = all_library_views(&state);
    let total = views.len() as i32;
    HttpResponse::Ok().json(ViewsResult {
        items: views,
        total_record_count: total,
        start_index: 0,
    })
}

pub async fn user_views_query(_user: AuthedUser, state: web::Data<JellyfinState>) -> HttpResponse {
    let views = all_library_views(&state);
    let total = views.len() as i32;
    HttpResponse::Ok().json(ViewsResult {
        items: views,
        total_record_count: total,
        start_index: 0,
    })
}

pub async fn media_folders(_user: AuthedUser, state: web::Data<JellyfinState>) -> HttpResponse {
    let views = all_library_views(&state);
    let total = views.len() as i32;
    HttpResponse::Ok().json(ViewsResult {
        items: views,
        total_record_count: total,
        start_index: 0,
    })
}

pub async fn library_virtual_folders(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
) -> HttpResponse {
    HttpResponse::Ok().json(vec![
        json!({
            "Name": "Music",
            "Locations": [state.music_dir.to_string_lossy()],
            "CollectionType": "music",
            "ItemId": mapping::library_guid(),
        }),
        json!({
            "Name": "Playlists",
            "Locations": [],
            "CollectionType": "playlists",
            "ItemId": mapping::playlists_library_guid(),
        }),
    ])
}

// ── Items query (custom parser — handles repeated keys + Pascal/camelCase) ──

#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct ItemsQuery {
    pub parent_id: Option<String>,
    pub include_item_types: Option<String>,
    pub media_types: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub name_less_than: Option<String>,
    pub recursive: Option<bool>,
    pub search_term: Option<String>,
    pub ids: Option<String>,
    pub album_artist_ids: Option<String>,
    pub artist_ids: Option<String>,
    pub album_ids: Option<String>,
    pub start_index: Option<i64>,
    pub limit: Option<i64>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub user_id: Option<String>,
    pub enable_user_data: Option<bool>,
    pub enable_total_record_count: Option<bool>,
    pub enable_images: Option<bool>,
    pub fields: Option<String>,
    /// `IsFavorite=true|false` OR `Filters=IsFavorite,IsLiked,…` — both
    /// mean "restrict to favorited items". `None` = no filter applied.
    pub is_favorite: Option<bool>,
}

fn collect_query(req: &HttpRequest) -> std::collections::HashMap<String, Vec<String>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for pair in req.query_string().split('&').filter(|s| !s.is_empty()) {
        let mut it = pair.splitn(2, '=');
        let key = it.next().unwrap_or("");
        let val = it.next().unwrap_or("");
        let decoded = urlencoding::decode(val)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| val.to_string());
        out.entry(key.to_string()).or_default().push(decoded);
    }
    out
}

fn parse_items_query(req: &HttpRequest) -> ItemsQuery {
    let q = collect_query(req);
    let one = |k: &str| q.get(k).and_then(|v| v.first()).cloned();
    let csv = |k: &str| q.get(k).map(|v| v.join(","));
    let parse_bool = |k: &str| one(k).and_then(|s| s.parse::<bool>().ok());
    let parse_i64 = |k: &str| one(k).and_then(|s| s.parse::<i64>().ok());
    ItemsQuery {
        parent_id: one("parentId").or_else(|| one("ParentId")),
        include_item_types: csv("includeItemTypes").or_else(|| csv("IncludeItemTypes")),
        media_types: csv("mediaTypes").or_else(|| csv("MediaTypes")),
        name_starts_with: one("nameStartsWith").or_else(|| one("NameStartsWith")),
        name_starts_with_or_greater: one("nameStartsWithOrGreater")
            .or_else(|| one("NameStartsWithOrGreater")),
        name_less_than: one("nameLessThan").or_else(|| one("NameLessThan")),
        recursive: parse_bool("recursive"),
        search_term: one("searchTerm").or_else(|| one("SearchTerm")),
        ids: csv("ids").or_else(|| csv("Ids")),
        album_artist_ids: csv("albumArtistIds").or_else(|| csv("AlbumArtistIds")),
        artist_ids: csv("artistIds").or_else(|| csv("ArtistIds")),
        album_ids: csv("albumIds").or_else(|| csv("AlbumIds")),
        start_index: parse_i64("startIndex").or_else(|| parse_i64("StartIndex")),
        limit: parse_i64("limit").or_else(|| parse_i64("Limit")),
        sort_by: one("sortBy").or_else(|| one("SortBy")),
        sort_order: one("sortOrder").or_else(|| one("SortOrder")),
        user_id: one("userId").or_else(|| one("UserId")),
        enable_user_data: parse_bool("enableUserData"),
        enable_total_record_count: parse_bool("enableTotalRecordCount"),
        enable_images: parse_bool("enableImages"),
        fields: csv("fields").or_else(|| csv("Fields")),
        is_favorite: {
            // Prefer explicit IsFavorite=…; fall back to Filters=IsFavorite.
            let direct = parse_bool("isFavorite").or_else(|| parse_bool("IsFavorite"));
            direct.or_else(|| {
                let filters = csv("filters").or_else(|| csv("Filters"))?;
                if filters
                    .split(',')
                    .any(|f| f.trim().eq_ignore_ascii_case("IsFavorite"))
                {
                    Some(true)
                } else {
                    None
                }
            })
        },
    }
}

// ── Track/album/artist → BaseItemDto ────────────────────────────────────────

/// Build a `UserItemDataDto` for a given (kind, native_id). Merges
/// `IsFavorite` from [`super::favorites`] with playback / rating /
/// likes from [`super::user_data`]; for tracks the latter also folds
/// in rockbox-engine playback counters via `track_stats`.
async fn user_data_for(
    state: &JellyfinState,
    kind: &str,
    native_id: &str,
    item_guid: &str,
) -> UserItemDataDto {
    let is_favorite = super::favorites::is_favorite(&state.pool, kind, native_id).await;
    let ud = super::user_data::get(&state.pool, kind, native_id).await;
    UserItemDataDto {
        rating: ud.rating,
        played_percentage: None,
        unplayed_item_count: None,
        playback_position_ticks: ud.playback_position_ticks,
        play_count: ud.play_count,
        is_favorite,
        likes: ud.likes,
        last_played_date: ud.last_played_date,
        played: ud.played,
        key: item_guid.to_string(),
        item_id: item_guid.to_string(),
    }
}

async fn artist_to_dto(state: &JellyfinState, a: &Artist) -> BaseItemDto {
    let id = mapping::remember_artist(&state.pool, a)
        .await
        .unwrap_or_else(|_| mapping::guid(mapping::KIND_ARTIST, &a.id));
    let user_data = user_data_for(state, mapping::KIND_ARTIST, &a.id, &id).await;
    // Cache-only enrichment on the list path: overview + tag genres come
    // straight from `jf_artist_enrichment`. Fresh fetches happen only in
    // the detail handlers so lists stay fast.
    let enrichment = super::enrichment::get_artist(&state.pool, &a.id).await;
    let overview = enrichment.as_ref().and_then(|e| e.bio.clone());
    let genres = enrichment
        .as_ref()
        .filter(|e| !e.tags.is_empty())
        .map(|e| e.tags.clone());
    BaseItemDto {
        id,
        server_id: Some(state.server_id.clone()),
        name: Some(a.name.clone()),
        item_type: "MusicArtist",
        media_type: "Unknown",
        is_folder: Some(true),
        sort_name: Some(a.name.clone()),
        location_type: Some("FileSystem"),
        image_tags: Some(ImageTags {
            primary: a.image.clone().map(|_| a.id.clone()),
        }),
        overview,
        genres,
        user_data: Some(user_data),
        ..Default::default()
    }
}

async fn album_to_dto(state: &JellyfinState, al: &Album) -> BaseItemDto {
    let id = mapping::remember_album(&state.pool, al)
        .await
        .unwrap_or_else(|_| mapping::guid(mapping::KIND_ALBUM, &al.id));
    let artist_guid = mapping::guid(mapping::KIND_ARTIST, &al.artist_id);
    let tracks = repo::album_tracks::find_by_album(state.pool.clone(), &al.id)
        .await
        .unwrap_or_default();
    let song_count = tracks.len() as i32;
    let duration_ms: i64 = tracks.iter().map(|t| t.length as i64).sum();
    let user_data = user_data_for(state, mapping::KIND_ALBUM, &al.id, &id).await;
    // Cache-only enrichment on the list path — the detail handlers
    // refresh from Last.fm before falling through to us.
    let enrichment = super::enrichment::get_album(&state.pool, &al.id).await;
    let overview = enrichment.as_ref().and_then(|e| e.description.clone());
    let genres = enrichment
        .as_ref()
        .filter(|e| !e.tags.is_empty())
        .map(|e| e.tags.clone());
    BaseItemDto {
        id: id.clone(),
        server_id: Some(state.server_id.clone()),
        name: Some(al.title.clone()),
        item_type: "MusicAlbum",
        media_type: "Unknown",
        is_folder: Some(true),
        production_year: if al.year > 0 {
            Some(al.year as i32)
        } else {
            None
        },
        premiere_date: if al.year > 0 {
            Some(format!("{:04}-01-01T00:00:00.0000000", al.year))
        } else {
            None
        },
        album: Some(al.title.clone()),
        album_id: Some(id),
        album_artist: Some(al.artist.clone()),
        album_artists: Some(vec![NameGuidPair {
            name: Some(al.artist.clone()),
            id: artist_guid.clone(),
        }]),
        artist_items: Some(vec![NameGuidPair {
            name: Some(al.artist.clone()),
            id: artist_guid.clone(),
        }]),
        artists: Some(vec![al.artist.clone()]),
        parent_id: Some(artist_guid),
        sort_name: Some(al.title.clone()),
        run_time_ticks: Some(duration_ms * TICKS_PER_MS),
        song_count: Some(song_count),
        child_count: Some(song_count),
        location_type: Some("FileSystem"),
        image_tags: Some(ImageTags {
            primary: al.album_art.clone().map(|_| al.id.clone()),
        }),
        overview,
        genres,
        user_data: Some(user_data),
        ..Default::default()
    }
}

async fn track_to_dto(state: &JellyfinState, t: &Track) -> BaseItemDto {
    let id = mapping::remember_track(&state.pool, t)
        .await
        .unwrap_or_else(|_| mapping::guid(mapping::KIND_TRACK, &t.id));
    let album_guid = mapping::guid(mapping::KIND_ALBUM, &t.album_id);
    let artist_guid = mapping::guid(mapping::KIND_ARTIST, &t.artist_id);
    // rockbox-library stores length in milliseconds.
    let run_time_ticks = (t.length as i64) * TICKS_PER_MS;
    let container = std::path::Path::new(&t.path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "mp3".to_string());

    let audio_stream = MediaStream {
        codec: Some(container.clone()),
        stream_type: "Audio",
        index: 0,
        is_default: true,
        channels: Some(2),
        sample_rate: Some(t.frequency as i32),
        bit_rate: Some((t.bitrate as i32) * 1000),
        video_range: "Unknown",
        video_range_type: "Unknown",
        audio_spatial_format: "None",
        is_interlaced: false,
        is_forced: false,
        is_hearing_impaired: false,
        is_original: false,
        is_external: false,
        is_text_subtitle_stream: false,
        supports_external_stream: false,
        ..Default::default()
    };

    let user_data = user_data_for(state, mapping::KIND_TRACK, &t.id, &id).await;

    let media_source = MediaSource {
        protocol: "File",
        id: Some(id.clone()),
        path: Some(t.path.clone()),
        source_type: "Default",
        container: Some(container.clone()),
        size: Some(t.filesize as i64),
        name: Some(t.title.clone()),
        is_remote: false,
        run_time_ticks: Some(run_time_ticks),
        read_at_native_framerate: false,
        ignore_dts: false,
        ignore_index: false,
        gen_pts_input: false,
        supports_transcoding: false,
        supports_direct_stream: true,
        supports_direct_play: true,
        is_infinite_stream: false,
        use_most_compatible_transcoding_profile: false,
        requires_opening: false,
        requires_closing: false,
        requires_looping: false,
        supports_probing: false,
        media_streams: Some(vec![audio_stream.clone()]),
        formats: None,
        bitrate: Some((t.bitrate as i32) * 1000),
        transcoding_sub_protocol: "http",
        default_audio_stream_index: Some(0),
        default_subtitle_stream_index: None,
        has_segments: false,
    };

    BaseItemDto {
        id: id.clone(),
        server_id: Some(state.server_id.clone()),
        name: Some(t.title.clone()),
        item_type: "Audio",
        media_type: "Audio",
        is_folder: Some(false),
        production_year: t.year.map(|y| y as i32),
        index_number: t.track_number.map(|n| n as i32),
        parent_index_number: Some(t.disc_number as i32),
        run_time_ticks: Some(run_time_ticks),
        container: Some(container),
        path: Some(t.path.clone()),
        album: Some(t.album.clone()),
        album_id: Some(album_guid.clone()),
        album_artist: Some(t.album_artist.clone()),
        album_artists: Some(vec![NameGuidPair {
            name: Some(t.album_artist.clone()),
            id: artist_guid.clone(),
        }]),
        artist_items: Some(vec![NameGuidPair {
            name: Some(t.artist.clone()),
            id: artist_guid.clone(),
        }]),
        artists: Some(vec![t.artist.clone()]),
        parent_id: Some(album_guid),
        genres: t.genre.as_ref().map(|g| vec![g.clone()]),
        location_type: Some("FileSystem"),
        media_sources: Some(vec![media_source.clone()]),
        media_source_count: Some(1),
        media_streams: Some(vec![audio_stream]),
        image_tags: Some(ImageTags {
            primary: t.album_art.clone().map(|_| t.album_id.clone()),
        }),
        image_blur_hashes: Some(ImageBlurHashes::default()),
        user_data: Some(user_data),
        ..Default::default()
    }
}

async fn playlist_to_dto(state: &JellyfinState, p: &Playlist) -> BaseItemDto {
    let id = mapping::remember_playlist(&state.pool, &p.id)
        .await
        .unwrap_or_else(|_| mapping::guid(mapping::KIND_PLAYLIST, &p.id));
    // Sum durations by walking the track list — small O(n) per playlist,
    // which is fine given typical playlist sizes.
    let track_ids = state
        .playlist_store
        .get_track_ids(&p.id)
        .await
        .unwrap_or_default();
    let mut run_time_ticks: i64 = 0;
    for tid in &track_ids {
        if let Ok(Some(t)) = repo::track::find(state.pool.clone(), tid).await {
            run_time_ticks += (t.length as i64) * TICKS_PER_MS;
        }
    }
    let child_count = track_ids.len() as i32;
    let user_data = user_data_for(state, mapping::KIND_PLAYLIST, &p.id, &id).await;
    BaseItemDto {
        id: id.clone(),
        server_id: Some(state.server_id.clone()),
        name: Some(p.name.clone()),
        item_type: "Playlist",
        media_type: "Audio",
        is_folder: Some(true),
        collection_type: Some("playlist"),
        location_type: Some("FileSystem"),
        sort_name: Some(p.name.clone()),
        date_created: Some(now_iso()),
        overview: p.description.clone(),
        child_count: Some(child_count),
        song_count: Some(child_count),
        run_time_ticks: Some(run_time_ticks),
        parent_id: Some(mapping::playlists_library_guid()),
        user_data: Some(user_data),
        ..Default::default()
    }
}

fn includes(types_csv: &Option<String>, want: &str) -> bool {
    match types_csv {
        None => false,
        Some(s) => s.split(',').any(|p| p.trim().eq_ignore_ascii_case(want)),
    }
}

async fn resolve_native(state: &JellyfinState, guid: &str) -> Option<(String, String)> {
    mapping::lookup(&state.pool, guid).await.ok().flatten()
}

// ── Items endpoints ─────────────────────────────────────────────────────────

pub async fn items(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    items_impl(state, parse_items_query(&req)).await
}

pub async fn user_items(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    _path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    items_impl(state, parse_items_query(&req)).await
}

async fn items_impl(state: web::Data<JellyfinState>, q: ItemsQuery) -> HttpResponse {
    let library_id = mapping::library_guid();
    let playlists_id = mapping::playlists_library_guid();

    // `?IsFavorite=true` / `Filters=IsFavorite` — return the union of
    // favorited items across the requested kinds. Honours
    // `IncludeItemTypes`; without one, returns favorites of all kinds.
    if q.is_favorite == Some(true) {
        return list_favorites(&state, &q).await;
    }

    // `/Items?userId=X` with no filters = list libraries.
    if q.parent_id.is_none()
        && q.ids.is_none()
        && q.search_term.is_none()
        && q.include_item_types.is_none()
        && q.media_types.is_none()
        && q.album_artist_ids.is_none()
        && q.artist_ids.is_none()
        && q.album_ids.is_none()
    {
        let views = all_library_views(&state);
        let total = views.len() as i32;
        return HttpResponse::Ok().json(ItemsResult {
            items: views,
            total_record_count: total,
            start_index: 0,
        });
    }

    // Free-text search.
    if let Some(term) = q.search_term.as_deref().filter(|s| !s.is_empty()) {
        let limit = q.limit.unwrap_or(50).max(1);
        let no_filter = q.include_item_types.is_none();
        let pattern = like_pattern(term);
        let mut dtos: Vec<BaseItemDto> = Vec::new();
        if no_filter || includes(&q.include_item_types, "MusicArtist") {
            let where_clause = ("name LIKE ?1".to_string(), vec![pattern.clone()]);
            if let Ok(rows) = repo::artist::filter(state.pool.clone(), where_clause).await {
                for a in rows.into_iter().take(limit as usize) {
                    dtos.push(artist_to_dto(&state, &a).await);
                }
            }
        }
        if no_filter || includes(&q.include_item_types, "MusicAlbum") {
            let where_clause = ("title LIKE ?1".to_string(), vec![pattern.clone()]);
            if let Ok(rows) = repo::album::filter(state.pool.clone(), where_clause).await {
                for a in rows.into_iter().take(limit as usize) {
                    dtos.push(album_to_dto(&state, &a).await);
                }
            }
        }
        if no_filter || includes(&q.include_item_types, "Audio") {
            let where_clause = ("title LIKE ?1".to_string(), vec![pattern]);
            if let Ok(rows) = repo::track::filter(state.pool.clone(), where_clause).await {
                for t in rows.into_iter().take(limit as usize) {
                    dtos.push(track_to_dto(&state, &t).await);
                }
            }
        }
        let total = dtos.len() as i32;
        return HttpResponse::Ok().json(ItemsResult {
            items: dtos,
            total_record_count: total,
            start_index: 0,
        });
    }

    // Explicit Ids= lookup.
    if let Some(ids) = q.ids.as_ref() {
        let mut out: Vec<BaseItemDto> = Vec::new();
        for raw in ids.split(',') {
            let g = mapping::normalize_guid(raw.trim());
            if let Some((kind, native)) = resolve_native(&state, &g).await {
                match kind.as_str() {
                    "track" => {
                        if let Ok(Some(t)) = repo::track::find(state.pool.clone(), &native).await {
                            out.push(track_to_dto(&state, &t).await);
                        }
                    }
                    "album" => {
                        if let Ok(Some(a)) = repo::album::find(state.pool.clone(), &native).await {
                            out.push(album_to_dto(&state, &a).await);
                        }
                    }
                    "artist" => {
                        if let Ok(Some(a)) = repo::artist::find(state.pool.clone(), &native).await {
                            out.push(artist_to_dto(&state, &a).await);
                        }
                    }
                    "playlist" => {
                        if let Ok(Some(p)) = state.playlist_store.get(&native).await {
                            out.push(playlist_to_dto(&state, &p).await);
                        }
                    }
                    _ => {}
                }
            }
        }
        let total = out.len() as i32;
        return HttpResponse::Ok().json(ItemsResult {
            items: out,
            total_record_count: total,
            start_index: 0,
        });
    }

    // Filter by parent — artist parent → albums; album parent → tracks.
    if let Some(parent) = q.parent_id.as_ref() {
        let g = mapping::normalize_guid(parent);
        if g == library_id {
            return list_artists_or_albums_or_tracks(&state, &q).await;
        }
        if g == playlists_id {
            return list_playlists(&state, &q).await;
        }
        if let Some((kind, native)) = resolve_native(&state, &g).await {
            match kind.as_str() {
                "artist" => {
                    let albums = repo::album::find_by_artist(state.pool.clone(), &native)
                        .await
                        .unwrap_or_default();
                    let mut dtos = Vec::with_capacity(albums.len());
                    for a in &albums {
                        dtos.push(album_to_dto(&state, a).await);
                    }
                    let total = dtos.len() as i32;
                    return HttpResponse::Ok().json(ItemsResult {
                        items: dtos,
                        total_record_count: total,
                        start_index: 0,
                    });
                }
                "album" => {
                    let tracks = repo::album_tracks::find_by_album(state.pool.clone(), &native)
                        .await
                        .unwrap_or_default();
                    let mut dtos = Vec::with_capacity(tracks.len());
                    for t in &tracks {
                        dtos.push(track_to_dto(&state, t).await);
                    }
                    let total = dtos.len() as i32;
                    return HttpResponse::Ok().json(ItemsResult {
                        items: dtos,
                        total_record_count: total,
                        start_index: 0,
                    });
                }
                "playlist" => {
                    // Tapping a playlist tile: return its tracks. Delegates
                    // to playlist_items via a direct build to avoid re-lookup.
                    return playlist_items_json(&state, &native, &q).await;
                }
                "genre" => {
                    // Tapping a genre tile: return its tracks (default) or
                    // albums / artists when the client asks for that kind.
                    return list_by_genre(&state, &q, &native).await;
                }
                _ => {}
            }
        }
        return HttpResponse::Ok().json(ItemsResult {
            items: vec![],
            total_record_count: 0,
            start_index: 0,
        });
    }

    // No parent: `?IncludeItemTypes=Playlist` returns the flat playlists list.
    if includes(&q.include_item_types, "Playlist") {
        return list_playlists(&state, &q).await;
    }

    // No parent: `?IncludeItemTypes=MusicGenre` returns the flat genre list.
    if includes(&q.include_item_types, "MusicGenre") || includes(&q.include_item_types, "Genre") {
        return list_genres_q(&state, &q).await;
    }

    // No parent: filter by artistIds / albumArtistIds.
    let artist_filter = q.album_artist_ids.clone().or_else(|| q.artist_ids.clone());

    if includes(&q.include_item_types, "Audio") {
        if let Some(filter) = artist_filter.clone() {
            if let Some(first) = filter.split(',').next() {
                let g = mapping::normalize_guid(first);
                if let Some((kind, native)) = resolve_native(&state, &g).await {
                    if kind == "artist" {
                        let tracks =
                            repo::artist_tracks::find_by_artist(state.pool.clone(), &native)
                                .await
                                .unwrap_or_default();
                        let mut dtos = Vec::with_capacity(tracks.len());
                        for t in &tracks {
                            dtos.push(track_to_dto(&state, t).await);
                        }
                        let total = dtos.len() as i32;
                        return HttpResponse::Ok().json(ItemsResult {
                            items: dtos,
                            total_record_count: total,
                            start_index: 0,
                        });
                    }
                }
            }
        }
        // Plain all-songs.
        return list_artists_or_albums_or_tracks(&state, &q).await;
    }

    if includes(&q.include_item_types, "MusicAlbum") {
        if let Some(filter) = artist_filter {
            if let Some(first) = filter.split(',').next() {
                let g = mapping::normalize_guid(first);
                if let Some((kind, native)) = resolve_native(&state, &g).await {
                    if kind == "artist" {
                        let albums = repo::album::find_by_artist(state.pool.clone(), &native)
                            .await
                            .unwrap_or_default();
                        let mut dtos = Vec::with_capacity(albums.len());
                        for a in &albums {
                            dtos.push(album_to_dto(&state, a).await);
                        }
                        let total = dtos.len() as i32;
                        return HttpResponse::Ok().json(ItemsResult {
                            items: dtos,
                            total_record_count: total,
                            start_index: 0,
                        });
                    }
                }
            }
        }
        return list_artists_or_albums_or_tracks(&state, &q).await;
    }

    list_artists_or_albums_or_tracks(&state, &q).await
}

/// Recognize the `sortBy` values home rails use to build "Most
/// Played" / "Recently Played". Returns `Some((sql_order_clause,))` when
/// the client asked for something we can honour; otherwise falls back to
/// the default alphabetic ordering.
fn track_sort_order(q: &ItemsQuery) -> Option<&'static str> {
    let sort_by = q.sort_by.as_deref()?;
    let first = sort_by.split(',').next()?.trim();
    let descending = q
        .sort_order
        .as_deref()
        .map(|s| s.eq_ignore_ascii_case("Descending"))
        .unwrap_or(false);
    match first.to_ascii_lowercase().as_str() {
        "playcount" => Some(if descending {
            "t.play_count DESC, LOWER(track.title) ASC"
        } else {
            "t.play_count ASC, LOWER(track.title) ASC"
        }),
        "dateplayed" | "datelastplayed" => Some(if descending {
            "t.last_played DESC, LOWER(track.title) ASC"
        } else {
            "t.last_played ASC, LOWER(track.title) ASC"
        }),
        _ => None,
    }
}

/// Serve the "Most Played" / "Recently Played" rails via the /Items
/// generic endpoint. Joins `track` with `track_stats` so clients don't
/// need to bounce through /Users/UserData first.
async fn tracks_sorted_by_stats(
    state: &JellyfinState,
    order: &str,
    limit: i64,
    offset: i64,
) -> Vec<Track> {
    let sql = format!(
        "SELECT track.* FROM track LEFT JOIN track_stats t ON track.id = t.track_id
         WHERE track.is_remote = 0
         ORDER BY {order}
         LIMIT ?1 OFFSET ?2"
    );
    sqlx::query_as(&sql)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default()
}

async fn list_artists_or_albums_or_tracks(state: &JellyfinState, q: &ItemsQuery) -> HttpResponse {
    let limit = q.limit.unwrap_or(100).max(1);
    let offset = q.start_index.unwrap_or(0).max(0);
    let starts = q.name_starts_with.as_deref();
    let geq = q.name_starts_with_or_greater.as_deref();
    let lt = q.name_less_than.as_deref();

    if includes(&q.include_item_types, "Audio") {
        // Route through track_stats when a supported sortBy is set —
        // the MostPlayed / RecentlyPlayed rails are just /Items calls
        // with `sortBy=PlayCount|DatePlayed&sortOrder=Descending`.
        if let Some(order) = track_sort_order(q) {
            let tracks = tracks_sorted_by_stats(state, order, limit, offset).await;
            let total = repo::track::count_filtered(state.pool.clone(), None, None, None)
                .await
                .unwrap_or(0) as i32;
            let mut dtos = Vec::with_capacity(tracks.len());
            for t in &tracks {
                dtos.push(track_to_dto(state, t).await);
            }
            return HttpResponse::Ok().json(ItemsResult {
                items: dtos,
                total_record_count: total,
                start_index: offset as i32,
            });
        }
        let tracks = repo::track::filtered(state.pool.clone(), starts, geq, lt, limit, offset)
            .await
            .unwrap_or_default();
        let total = repo::track::count_filtered(state.pool.clone(), starts, geq, lt)
            .await
            .unwrap_or(0) as i32;
        let mut dtos = Vec::with_capacity(tracks.len());
        for t in &tracks {
            dtos.push(track_to_dto(state, t).await);
        }
        return HttpResponse::Ok().json(ItemsResult {
            items: dtos,
            total_record_count: total,
            start_index: offset as i32,
        });
    }
    if includes(&q.include_item_types, "MusicAlbum") {
        let albums = repo::album::filtered(state.pool.clone(), starts, geq, lt, limit, offset)
            .await
            .unwrap_or_default();
        let total = repo::album::count_filtered(state.pool.clone(), starts, geq, lt)
            .await
            .unwrap_or(0) as i32;
        let mut dtos = Vec::with_capacity(albums.len());
        for a in &albums {
            dtos.push(album_to_dto(state, a).await);
        }
        return HttpResponse::Ok().json(ItemsResult {
            items: dtos,
            total_record_count: total,
            start_index: offset as i32,
        });
    }
    let artists = repo::artist::filtered(state.pool.clone(), starts, geq, lt, limit, offset)
        .await
        .unwrap_or_default();
    let total = repo::artist::count_filtered(state.pool.clone(), starts, geq, lt)
        .await
        .unwrap_or(0) as i32;
    let mut dtos = Vec::with_capacity(artists.len());
    for a in &artists {
        dtos.push(artist_to_dto(state, a).await);
    }
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: offset as i32,
    })
}

fn like_pattern(term: &str) -> String {
    let escaped = term.replace('%', "\\%").replace('_', "\\_");
    format!("%{}%", escaped)
}

/// If `guid` matches the music library's virtual GUID, return its
/// CollectionFolder DTO. Some clients (e.g. Moonfin) first fetch the library
/// item itself when you tap its tile; without this early-return they got 404
/// and the whole library page failed to render.
fn library_view_for(state: &JellyfinState, guid: &str) -> Option<BaseItemDto> {
    if guid == mapping::library_guid() {
        Some(music_library_view(state))
    } else if guid == mapping::playlists_library_guid() {
        Some(playlists_library_view(state))
    } else {
        None
    }
}

pub async fn item_by_id(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    if let Some(view) = library_view_for(&state, &g) {
        return HttpResponse::Ok().json(view);
    }
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    match kind.as_str() {
        "track" => match repo::track::find(state.pool.clone(), &native).await {
            Ok(Some(t)) => HttpResponse::Ok().json(track_to_dto(&state, &t).await),
            _ => HttpResponse::NotFound().finish(),
        },
        "album" => match repo::album::find(state.pool.clone(), &native).await {
            Ok(Some(a)) => {
                // Detail view: refresh the enrichment cache from Last.fm
                // first so the DTO carries a fresh description + tags.
                let _ = super::enrichment::refresh_album(
                    &state.pool,
                    state.lastfm.as_ref(),
                    &a.id,
                    &a.artist,
                    &a.title,
                )
                .await;
                HttpResponse::Ok().json(album_to_dto(&state, &a).await)
            }
            _ => HttpResponse::NotFound().finish(),
        },
        "artist" => match repo::artist::find(state.pool.clone(), &native).await {
            Ok(Some(a)) => {
                // Detail view: refresh the enrichment cache from Last.fm
                // first so the DTO carries a fresh bio + tags.
                let _ = super::enrichment::refresh_artist(
                    &state.pool,
                    state.lastfm.as_ref(),
                    &a.id,
                    &a.name,
                )
                .await;
                HttpResponse::Ok().json(artist_to_dto(&state, &a).await)
            }
            _ => HttpResponse::NotFound().finish(),
        },
        "playlist" => match state.playlist_store.get(&native).await {
            Ok(Some(p)) => HttpResponse::Ok().json(playlist_to_dto(&state, &p).await),
            _ => HttpResponse::NotFound().finish(),
        },
        "genre" => match repo::genre::find(state.pool.clone(), &native).await {
            Ok(Some(g)) => HttpResponse::Ok().json(genre_to_dto(&state, &g).await),
            _ => HttpResponse::NotFound().finish(),
        },
        _ => HttpResponse::NotFound().finish(),
    }
}

pub async fn user_item_by_id(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (_user_id, item_id) = path.into_inner();
    let g = mapping::normalize_guid(&item_id);
    if let Some(view) = library_view_for(&state, &g) {
        return HttpResponse::Ok().json(view);
    }
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    match kind.as_str() {
        "track" => match repo::track::find(state.pool.clone(), &native).await {
            Ok(Some(t)) => HttpResponse::Ok().json(track_to_dto(&state, &t).await),
            _ => HttpResponse::NotFound().finish(),
        },
        "album" => match repo::album::find(state.pool.clone(), &native).await {
            Ok(Some(a)) => {
                // Detail view: refresh the enrichment cache from Last.fm
                // first so the DTO carries a fresh description + tags.
                let _ = super::enrichment::refresh_album(
                    &state.pool,
                    state.lastfm.as_ref(),
                    &a.id,
                    &a.artist,
                    &a.title,
                )
                .await;
                HttpResponse::Ok().json(album_to_dto(&state, &a).await)
            }
            _ => HttpResponse::NotFound().finish(),
        },
        "artist" => match repo::artist::find(state.pool.clone(), &native).await {
            Ok(Some(a)) => {
                // Detail view: refresh the enrichment cache from Last.fm
                // first so the DTO carries a fresh bio + tags.
                let _ = super::enrichment::refresh_artist(
                    &state.pool,
                    state.lastfm.as_ref(),
                    &a.id,
                    &a.name,
                )
                .await;
                HttpResponse::Ok().json(artist_to_dto(&state, &a).await)
            }
            _ => HttpResponse::NotFound().finish(),
        },
        "playlist" => match state.playlist_store.get(&native).await {
            Ok(Some(p)) => HttpResponse::Ok().json(playlist_to_dto(&state, &p).await),
            _ => HttpResponse::NotFound().finish(),
        },
        "genre" => match repo::genre::find(state.pool.clone(), &native).await {
            Ok(Some(g)) => HttpResponse::Ok().json(genre_to_dto(&state, &g).await),
            _ => HttpResponse::NotFound().finish(),
        },
        _ => HttpResponse::NotFound().finish(),
    }
}

// ── Playlists ───────────────────────────────────────────────────────────────

fn filter_playlists_by_query(playlists: Vec<Playlist>, q: &ItemsQuery) -> Vec<Playlist> {
    playlists
        .into_iter()
        .filter(|p| {
            let name = p.name.to_lowercase();
            if let Some(s) = q.name_starts_with.as_deref() {
                if !name.starts_with(&s.to_lowercase()) {
                    return false;
                }
            }
            if let Some(s) = q.name_starts_with_or_greater.as_deref() {
                if name.as_str() < s.to_lowercase().as_str() {
                    return false;
                }
            }
            if let Some(s) = q.name_less_than.as_deref() {
                if name.as_str() >= s.to_lowercase().as_str() {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// `?IsFavorite=true` — return the union of favorited items across
/// tracks/albums/artists/playlists. `IncludeItemTypes` narrows the set
/// (defaults to all four); `NameStartsWith*` and pagination are honoured
/// on the merged list.
async fn list_favorites(state: &JellyfinState, q: &ItemsQuery) -> HttpResponse {
    let no_types = q.include_item_types.is_none();
    let want_track = no_types
        || includes(&q.include_item_types, "Audio")
        || includes(&q.include_item_types, "Track");
    let want_album = no_types || includes(&q.include_item_types, "MusicAlbum");
    let want_artist = no_types
        || includes(&q.include_item_types, "MusicArtist")
        || includes(&q.include_item_types, "AlbumArtist");
    let want_playlist = no_types || includes(&q.include_item_types, "Playlist");

    let mut dtos: Vec<BaseItemDto> = Vec::new();

    if want_track {
        for tid in super::favorites::favorite_native_ids(&state.pool, mapping::KIND_TRACK).await {
            if let Ok(Some(t)) = repo::track::find(state.pool.clone(), &tid).await {
                dtos.push(track_to_dto(state, &t).await);
            }
        }
    }
    if want_album {
        for aid in super::favorites::favorite_native_ids(&state.pool, mapping::KIND_ALBUM).await {
            if let Ok(Some(a)) = repo::album::find(state.pool.clone(), &aid).await {
                dtos.push(album_to_dto(state, &a).await);
            }
        }
    }
    if want_artist {
        for aid in super::favorites::favorite_native_ids(&state.pool, mapping::KIND_ARTIST).await {
            if let Ok(Some(a)) = repo::artist::find(state.pool.clone(), &aid).await {
                dtos.push(artist_to_dto(state, &a).await);
            }
        }
    }
    if want_playlist {
        for pid in super::favorites::favorite_native_ids(&state.pool, mapping::KIND_PLAYLIST).await
        {
            if let Ok(Some(p)) = state.playlist_store.get(&pid).await {
                dtos.push(playlist_to_dto(state, &p).await);
            }
        }
    }

    // NameStartsWith*/NameLessThan filtering — apply post-hoc since the
    // dtos come from four different tables.
    let matches = |name: &str| -> bool {
        let lower = name.to_lowercase();
        if let Some(s) = q.name_starts_with.as_deref() {
            if !lower.starts_with(&s.to_lowercase()) {
                return false;
            }
        }
        if let Some(s) = q.name_starts_with_or_greater.as_deref() {
            if lower.as_str() < s.to_lowercase().as_str() {
                return false;
            }
        }
        if let Some(s) = q.name_less_than.as_deref() {
            if lower.as_str() >= s.to_lowercase().as_str() {
                return false;
            }
        }
        true
    };
    dtos.retain(|d| d.name.as_deref().map(matches).unwrap_or(true));

    let total = dtos.len() as i32;
    let start = q.start_index.unwrap_or(0).max(0) as usize;
    let limit = q.limit.unwrap_or(500).max(1) as usize;
    let page: Vec<BaseItemDto> = dtos.into_iter().skip(start).take(limit).collect();
    HttpResponse::Ok().json(ItemsResult {
        items: page,
        total_record_count: total,
        start_index: start as i32,
    })
}

async fn list_playlists(state: &JellyfinState, q: &ItemsQuery) -> HttpResponse {
    let playlists = state.playlist_store.list().await.unwrap_or_default();
    let playlists = filter_playlists_by_query(playlists, q);
    let total = playlists.len() as i32;
    let start = q.start_index.unwrap_or(0).max(0) as usize;
    let limit = q.limit.unwrap_or(500).max(1) as usize;
    let slice: Vec<&Playlist> = playlists.iter().skip(start).take(limit).collect();
    let mut dtos = Vec::with_capacity(slice.len());
    for p in slice {
        dtos.push(playlist_to_dto(state, p).await);
    }
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: start as i32,
    })
}

async fn playlist_items_json(
    state: &JellyfinState,
    playlist_native: &str,
    q: &ItemsQuery,
) -> HttpResponse {
    let track_ids = state
        .playlist_store
        .get_track_ids(playlist_native)
        .await
        .unwrap_or_default();
    let total = track_ids.len() as i32;
    let start = q.start_index.unwrap_or(0).max(0) as usize;
    let limit = q.limit.unwrap_or(500).max(1) as usize;

    let mut dtos: Vec<BaseItemDto> = Vec::new();
    for (pos, tid) in track_ids.iter().enumerate().skip(start).take(limit) {
        if let Ok(Some(t)) = repo::track::find(state.pool.clone(), tid).await {
            let mut dto = track_to_dto(state, &t).await;
            dto.playlist_item_id = Some(mapping::playlist_entry_guid(playlist_native, pos as i64));
            dtos.push(dto);
        }
    }
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: start as i32,
    })
}

/// Convert a `PlaylistItemId` (our synthesized entry GUID) back to a
/// 0-based position inside the playlist. Falls back to accepting a raw
/// integer position or a plain track GUID.
async fn entry_id_to_position(
    state: &JellyfinState,
    playlist_native: &str,
    entry_id: &str,
) -> Option<i64> {
    let normalized = mapping::normalize_guid(entry_id);
    let track_ids = state
        .playlist_store
        .get_track_ids(playlist_native)
        .await
        .ok()?;
    for (pos, tid) in track_ids.iter().enumerate() {
        let expected = mapping::playlist_entry_guid(playlist_native, pos as i64);
        if mapping::normalize_guid(&expected) == normalized {
            return Some(pos as i64);
        }
        let track_guid = mapping::guid(mapping::KIND_TRACK, tid);
        if mapping::normalize_guid(&track_guid) == normalized {
            return Some(pos as i64);
        }
    }
    entry_id.parse::<i64>().ok()
}

async fn resolve_track_native_ids(state: &JellyfinState, ids_csv: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in ids_csv.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let g = mapping::normalize_guid(trimmed);
        if let Some((kind, native)) = resolve_native(state, &g).await {
            if kind == "track" {
                out.push(native);
            }
        }
    }
    out
}

/// `GET /Playlists` — extension over the spec; some clients probe this to
/// enumerate playlists in the same shape as `/Items`.
pub async fn playlists_list(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    list_playlists(&state, &q).await
}

/// `POST /Playlists` — `CreatePlaylistDto` body OR query params. Response is
/// `PlaylistCreationResult { Id }`.
pub async fn create_playlist_endpoint(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
    body: Option<web::Json<Value>>,
) -> HttpResponse {
    let query = collect_query(&req);
    let q_one = |k: &str| {
        query
            .get(k)
            .and_then(|v| v.first())
            .cloned()
            .filter(|s| !s.is_empty())
    };
    let q_ids = || -> Vec<String> {
        query
            .get("ids")
            .or_else(|| query.get("Ids"))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .flat_map(|s| {
                s.split(',')
                    .map(|p| p.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|s| !s.is_empty())
            .collect()
    };

    let body_val = body.map(|b| b.into_inner());
    let name = body_val
        .as_ref()
        .and_then(|v| v.get("Name").and_then(|n| n.as_str()).map(String::from))
        .or_else(|| q_one("name"))
        .or_else(|| q_one("Name"))
        .unwrap_or_else(|| "New Playlist".to_string());

    let body_ids: Vec<String> = body_val
        .as_ref()
        .and_then(|v| v.get("Ids").and_then(|a| a.as_array()))
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let all_ids: Vec<String> = if body_ids.is_empty() {
        q_ids()
    } else {
        body_ids
    };
    let track_ids = resolve_track_native_ids(&state, &all_ids.join(",")).await;

    let playlist = match state.playlist_store.create(&name, None, None, None).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("jellyfin: create_playlist: {e}");
            return HttpResponse::InternalServerError().finish();
        }
    };
    if !track_ids.is_empty() {
        if let Err(e) = state
            .playlist_store
            .add_tracks(&playlist.id, &track_ids)
            .await
        {
            tracing::error!("jellyfin: add_tracks: {e}");
        }
    }
    let _ = mapping::remember_playlist(&state.pool, &playlist.id).await;
    let guid = mapping::guid(mapping::KIND_PLAYLIST, &playlist.id);
    HttpResponse::Ok().json(PlaylistCreationResult { id: guid })
}

/// `GET /Playlists/{id}` — playlist metadata as a `BaseItemDto`.
pub async fn get_playlist_endpoint(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::NotFound().finish();
    }
    match state.playlist_store.get(&native).await {
        Ok(Some(p)) => HttpResponse::Ok().json(playlist_to_dto(&state, &p).await),
        _ => HttpResponse::NotFound().finish(),
    }
}

/// `POST /Playlists/{id}` — `UpdatePlaylistDto` body. Supports rename and
/// full-replace of the item list; other spec fields (Users, IsPublic) are
/// accepted and ignored because rockbox has a single-user model.
pub async fn update_playlist_endpoint(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    body: Option<web::Json<Value>>,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::NotFound().finish();
    }
    let body = match body {
        Some(b) => b.into_inner(),
        None => return HttpResponse::NoContent().finish(),
    };
    let existing = match state.playlist_store.get(&native).await {
        Ok(Some(p)) => p,
        _ => return HttpResponse::NotFound().finish(),
    };
    let new_name = body
        .get("Name")
        .and_then(|n| n.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(&existing.name);
    if let Err(e) = state
        .playlist_store
        .update(
            &native,
            new_name,
            existing.description.as_deref(),
            existing.image.as_deref(),
            existing.folder_id.as_deref(),
        )
        .await
    {
        tracing::error!("jellyfin: update_playlist: {e}");
        return HttpResponse::InternalServerError().finish();
    }
    if let Some(ids) = body.get("Ids").and_then(|a| a.as_array()) {
        let csv: String = ids
            .iter()
            .filter_map(|v| v.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let track_ids = resolve_track_native_ids(&state, &csv).await;
        // No native "replace tracks" — clear then re-add.
        for tid in state
            .playlist_store
            .get_track_ids(&native)
            .await
            .unwrap_or_default()
        {
            let _ = state.playlist_store.remove_track(&native, &tid).await;
        }
        let _ = state.playlist_store.add_tracks(&native, &track_ids).await;
    }
    HttpResponse::NoContent().finish()
}

/// `GET /Playlists/{id}/Items` — `BaseItemDtoQueryResult` of the playlist's
/// tracks in order. Each track DTO carries a `PlaylistItemId` so clients can
/// reference specific entries for remove/move.
pub async fn playlist_items(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::NotFound().finish();
    }
    let q = parse_items_query(&req);
    playlist_items_json(&state, &native, &q).await
}

/// `POST /Playlists/{id}/Items?ids=…` — append tracks at the end.
pub async fn add_playlist_items(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::NotFound().finish();
    }
    let query = collect_query(&req);
    let ids_csv = query
        .get("ids")
        .or_else(|| query.get("Ids"))
        .cloned()
        .unwrap_or_default()
        .join(",");
    let track_ids = resolve_track_native_ids(&state, &ids_csv).await;
    if !track_ids.is_empty() {
        let _ = state.playlist_store.add_tracks(&native, &track_ids).await;
    }
    HttpResponse::NoContent().finish()
}

/// `DELETE /Playlists/{id}/Items?entryIds=…` — remove one or more entries.
/// EntryIds can be our synthesized `PlaylistItemId`, a raw track GUID, or a
/// 0-based position number.
pub async fn remove_playlist_items(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::NotFound().finish();
    }
    let query = collect_query(&req);
    let entry_ids: Vec<String> = query
        .get("entryIds")
        .or_else(|| query.get("EntryIds"))
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .flat_map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .collect();
    if entry_ids.is_empty() {
        return HttpResponse::NoContent().finish();
    }
    let current = state
        .playlist_store
        .get_track_ids(&native)
        .await
        .unwrap_or_default();
    let mut positions_to_remove: std::collections::BTreeSet<i64> =
        std::collections::BTreeSet::new();
    for eid in &entry_ids {
        if let Some(pos) = entry_id_to_position(&state, &native, eid).await {
            positions_to_remove.insert(pos);
        }
    }
    let kept: Vec<String> = current
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !positions_to_remove.contains(&(*i as i64)))
        .map(|(_, tid)| tid)
        .collect();
    // Rebuild the playlist to preserve position order deterministically —
    // remove_track collapses duplicates so we can't use it for arbitrary
    // position deletes.
    for tid in state
        .playlist_store
        .get_track_ids(&native)
        .await
        .unwrap_or_default()
    {
        let _ = state.playlist_store.remove_track(&native, &tid).await;
    }
    let _ = state.playlist_store.add_tracks(&native, &kept).await;
    HttpResponse::NoContent().finish()
}

/// `POST /Playlists/{id}/Items/{itemId}/Move/{newIndex}` — move `itemId`
/// (a `PlaylistItemId`) to `newIndex`.
pub async fn move_playlist_item(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String, i64)>,
) -> HttpResponse {
    let (playlist_id, item_id, new_index) = path.into_inner();
    let g = mapping::normalize_guid(&playlist_id);
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::NotFound().finish();
    }
    let Some(from) = entry_id_to_position(&state, &native, &item_id).await else {
        return HttpResponse::NotFound().finish();
    };
    let mut current = state
        .playlist_store
        .get_track_ids(&native)
        .await
        .unwrap_or_default();
    if from < 0 || (from as usize) >= current.len() {
        return HttpResponse::BadRequest().finish();
    }
    let target = new_index.max(0).min(current.len() as i64 - 1) as usize;
    let track = current.remove(from as usize);
    current.insert(target, track);
    for tid in state
        .playlist_store
        .get_track_ids(&native)
        .await
        .unwrap_or_default()
    {
        let _ = state.playlist_store.remove_track(&native, &tid).await;
    }
    let _ = state.playlist_store.add_tracks(&native, &current).await;
    HttpResponse::NoContent().finish()
}

/// `GET /Playlists/{id}/Users` — rockbox is single-user, so this is always
/// an empty list.
pub async fn playlist_users(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, _native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::NotFound().finish();
    }
    HttpResponse::Ok().json(Vec::<Value>::new())
}

/// `DELETE /Items/{id}` — Jellyfin's canonical playlist-delete path.
/// Tracks/albums/artists are read-only over this API.
pub async fn delete_item(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "playlist" {
        return HttpResponse::Forbidden().finish();
    }
    match state.playlist_store.delete(&native).await {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => {
            tracing::error!("jellyfin: delete_playlist: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

// ── Favorites ───────────────────────────────────────────────────────────────

/// Resolve an item GUID to a `(kind, native_id)` pair; only the four
/// kinds Jellyfin models per spec (`Audio`/`MusicAlbum`/`MusicArtist`/
/// `Playlist`) are accepted. Everything else returns `None` so the
/// handler can 404.
async fn resolve_favorite_target(
    state: &JellyfinState,
    guid: &str,
) -> Option<(&'static str, String)> {
    let g = mapping::normalize_guid(guid);
    let (kind, native) = resolve_native(state, &g).await?;
    let kind_static: &'static str = match kind.as_str() {
        "track" => mapping::KIND_TRACK,
        "album" => mapping::KIND_ALBUM,
        "artist" => mapping::KIND_ARTIST,
        "playlist" => mapping::KIND_PLAYLIST,
        _ => return None,
    };
    Some((kind_static, native))
}

fn user_data_dto(item_guid: String, is_favorite: bool) -> UserItemDataDto {
    UserItemDataDto {
        rating: None,
        played_percentage: None,
        unplayed_item_count: None,
        playback_position_ticks: 0,
        play_count: 0,
        is_favorite,
        likes: None,
        last_played_date: None,
        played: false,
        key: item_guid.clone(),
        item_id: item_guid,
    }
}

/// `POST /UserFavoriteItems/{itemId}` (Jellyfin 10.9+) and the legacy
/// `POST /Users/{userId}/FavoriteItems/{itemId}` — mark as favorite.
/// Returns the updated `UserItemDataDto`.
pub async fn mark_favorite(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    mark_favorite_impl(&state, path.into_inner()).await
}

pub async fn mark_favorite_legacy(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (_user_id, item_id) = path.into_inner();
    mark_favorite_impl(&state, item_id).await
}

async fn mark_favorite_impl(state: &JellyfinState, item_id: String) -> HttpResponse {
    let guid = mapping::normalize_guid(&item_id);
    let Some((kind, native)) = resolve_favorite_target(state, &guid).await else {
        return HttpResponse::NotFound().finish();
    };
    if let Err(e) = super::favorites::mark(&state.pool, kind, &native).await {
        tracing::error!("jellyfin: mark_favorite {kind}/{native}: {e}");
        return HttpResponse::InternalServerError().finish();
    }
    HttpResponse::Ok().json(user_data_dto(guid, true))
}

/// `DELETE /UserFavoriteItems/{itemId}` and the legacy
/// `DELETE /Users/{userId}/FavoriteItems/{itemId}` — unmark.
pub async fn unmark_favorite(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    unmark_favorite_impl(&state, path.into_inner()).await
}

pub async fn unmark_favorite_legacy(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (_user_id, item_id) = path.into_inner();
    unmark_favorite_impl(&state, item_id).await
}

async fn unmark_favorite_impl(state: &JellyfinState, item_id: String) -> HttpResponse {
    let guid = mapping::normalize_guid(&item_id);
    let Some((kind, native)) = resolve_favorite_target(state, &guid).await else {
        return HttpResponse::NotFound().finish();
    };
    if let Err(e) = super::favorites::unmark(&state.pool, kind, &native).await {
        tracing::error!("jellyfin: unmark_favorite {kind}/{native}: {e}");
        return HttpResponse::InternalServerError().finish();
    }
    HttpResponse::Ok().json(user_data_dto(guid, false))
}

// ── UserData ────────────────────────────────────────────────────────────────

fn user_data_dto_from(
    item_guid: String,
    is_favorite: bool,
    ud: super::user_data::UserData,
) -> UserItemDataDto {
    UserItemDataDto {
        rating: ud.rating,
        played_percentage: None,
        unplayed_item_count: None,
        playback_position_ticks: ud.playback_position_ticks,
        play_count: ud.play_count,
        is_favorite,
        likes: ud.likes,
        last_played_date: ud.last_played_date,
        played: ud.played,
        key: item_guid.clone(),
        item_id: item_guid,
    }
}

/// `GET /UserItems/{itemId}/UserData` (10.9+) and legacy
/// `GET /Users/{userId}/Items/{itemId}/UserData` — return the rolled-up
/// `UserItemDataDto` for `itemId`.
pub async fn get_user_data(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    get_user_data_impl(&state, path.into_inner()).await
}

pub async fn get_user_data_legacy(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (_user_id, item_id) = path.into_inner();
    get_user_data_impl(&state, item_id).await
}

async fn get_user_data_impl(state: &JellyfinState, item_id: String) -> HttpResponse {
    let guid = mapping::normalize_guid(&item_id);
    let Some((kind, native)) = resolve_favorite_target(state, &guid).await else {
        return HttpResponse::NotFound().finish();
    };
    let is_favorite = super::favorites::is_favorite(&state.pool, kind, &native).await;
    let ud = super::user_data::get(&state.pool, kind, &native).await;
    HttpResponse::Ok().json(user_data_dto_from(guid, is_favorite, ud))
}

/// `POST /UserItems/{itemId}/UserData` and legacy variant — apply
/// `UpdateUserItemDataDto`. Unset fields on the request are preserved
/// (Jellyfin's spec — only present fields overwrite).
///
/// `IsFavorite` is honoured here as a shortcut so clients can toggle
/// the favorite flag through this endpoint instead of
/// `/UserFavoriteItems/{id}`.
pub async fn update_user_data(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    body: Option<web::Json<Value>>,
) -> HttpResponse {
    update_user_data_impl(&state, path.into_inner(), body).await
}

pub async fn update_user_data_legacy(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
    body: Option<web::Json<Value>>,
) -> HttpResponse {
    let (_user_id, item_id) = path.into_inner();
    update_user_data_impl(&state, item_id, body).await
}

async fn update_user_data_impl(
    state: &JellyfinState,
    item_id: String,
    body: Option<web::Json<Value>>,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&item_id);
    let Some((kind, native)) = resolve_favorite_target(state, &guid).await else {
        return HttpResponse::NotFound().finish();
    };
    let body = body.map(|b| b.into_inner()).unwrap_or(Value::Null);

    // IsFavorite handled through the favorites store so both surfaces
    // (jf_favorites + shared favourites table) stay in sync.
    if let Some(is_fav) = body.get("IsFavorite").and_then(|v| v.as_bool()) {
        let result = if is_fav {
            super::favorites::mark(&state.pool, kind, &native).await
        } else {
            super::favorites::unmark(&state.pool, kind, &native).await
        };
        if let Err(e) = result {
            tracing::error!("jellyfin: update_user_data favorite: {e}");
        }
    }

    let patch = super::user_data::UserDataPatch {
        played: body.get("Played").and_then(|v| v.as_bool()),
        play_count: body
            .get("PlayCount")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32),
        playback_position_ticks: body.get("PlaybackPositionTicks").and_then(|v| v.as_i64()),
        last_played_date: body
            .get("LastPlayedDate")
            .and_then(|v| v.as_str())
            .map(String::from),
        likes: body.get("Likes").and_then(|v| v.as_bool()),
        rating: body.get("Rating").and_then(|v| v.as_f64()),
    };

    let ud = match super::user_data::update(&state.pool, kind, &native, patch).await {
        Ok(ud) => ud,
        Err(e) => {
            tracing::error!("jellyfin: update_user_data {kind}/{native}: {e}");
            return HttpResponse::InternalServerError().finish();
        }
    };
    let is_favorite = super::favorites::is_favorite(&state.pool, kind, &native).await;
    HttpResponse::Ok().json(user_data_dto_from(guid, is_favorite, ud))
}

// ── InstantMix ──────────────────────────────────────────────────────────────

fn instant_mix_limit(req: &HttpRequest) -> usize {
    let query = collect_query(req);
    query
        .get("limit")
        .or_else(|| query.get("Limit"))
        .and_then(|v| v.first())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(super::instant_mix::DEFAULT_LIMIT)
}

async fn build_instant_mix_response(
    state: &JellyfinState,
    kind: &'static str,
    native: String,
    limit: usize,
) -> HttpResponse {
    let tracks =
        super::instant_mix::generate(&state.pool, &state.playlist_store, kind, &native, limit)
            .await;
    let mut dtos = Vec::with_capacity(tracks.len());
    for t in &tracks {
        dtos.push(track_to_dto(state, t).await);
    }
    let total = dtos.len() as i32;
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: 0,
    })
}

/// `GET /Items/{itemId}/InstantMix` — spec's generic dispatcher.
/// Peeks at the item kind and delegates to the appropriate seed
/// algorithm.
pub async fn instant_mix_by_item(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_favorite_target(&state, &guid).await else {
        return HttpResponse::NotFound().finish();
    };
    build_instant_mix_response(&state, kind, native, instant_mix_limit(&req)).await
}

/// `GET /Songs/{itemId}/InstantMix` — legacy pre-10.9 path. Rejects
/// non-track ids so clients don't accidentally build a mix from
/// mismatched kinds.
pub async fn instant_mix_songs(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_TRACK, native)) => {
            build_instant_mix_response(&state, mapping::KIND_TRACK, native, instant_mix_limit(&req))
                .await
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

pub async fn instant_mix_albums(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_ALBUM, native)) => {
            build_instant_mix_response(&state, mapping::KIND_ALBUM, native, instant_mix_limit(&req))
                .await
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

pub async fn instant_mix_artists(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_ARTIST, native)) => {
            build_instant_mix_response(
                &state,
                mapping::KIND_ARTIST,
                native,
                instant_mix_limit(&req),
            )
            .await
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

pub async fn instant_mix_playlists(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_PLAYLIST, native)) => {
            build_instant_mix_response(
                &state,
                mapping::KIND_PLAYLIST,
                native,
                instant_mix_limit(&req),
            )
            .await
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

/// `GET /Artists/InstantMix?id=<guid>` — query-string variant used by
/// some clients (Amcfy, Symfonium) instead of the path form.
pub async fn instant_mix_artists_query(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let query = collect_query(&req);
    let Some(id) = query
        .get("id")
        .or_else(|| query.get("Id"))
        .and_then(|v| v.first())
    else {
        return HttpResponse::BadRequest().finish();
    };
    let guid = mapping::normalize_guid(id);
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_ARTIST, native)) => {
            build_instant_mix_response(
                &state,
                mapping::KIND_ARTIST,
                native,
                instant_mix_limit(&req),
            )
            .await
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

/// `GET /MusicGenres/{name}/InstantMix` — name-keyed genre seed.
pub async fn instant_mix_music_genre(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let name = path.into_inner();
    let limit = instant_mix_limit(&req);
    let tracks = super::instant_mix::generate_from_genre_name(&state.pool, &name, limit).await;
    let mut dtos = Vec::with_capacity(tracks.len());
    for t in &tracks {
        dtos.push(track_to_dto(&state, t).await);
    }
    let total = dtos.len() as i32;
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: 0,
    })
}

// ── Lyrics ──────────────────────────────────────────────────────────────────

async fn resolve_audio_track(state: &JellyfinState, guid: &str) -> Option<Track> {
    let g = mapping::normalize_guid(guid);
    let (kind, native) = resolve_native(state, &g).await?;
    if kind != "track" {
        return None;
    }
    repo::track::find(state.pool.clone(), &native).await.ok()?
}

/// `GET /Audio/{itemId}/Lyrics` — returns the parsed `LyricDto` for the
/// audio item, or 404 if no sidecar is on disk.
pub async fn get_lyrics(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let Some(track) = resolve_audio_track(&state, &path.into_inner()).await else {
        return HttpResponse::NotFound().finish();
    };
    let track_path = PathBuf::from(&track.path);
    let Some(sidecar) = super::lyrics::find_sidecar(&track_path) else {
        return HttpResponse::NotFound().finish();
    };
    match super::lyrics::parse_sidecar(&sidecar) {
        Some(dto) => HttpResponse::Ok().json(dto),
        None => HttpResponse::NotFound().finish(),
    }
}

/// `POST /Audio/{itemId}/Lyrics` — write the request body to a `.lrc`
/// sidecar next to the audio file. Accepts either raw LRC / plain text
/// or a `LyricDto` JSON body (distinguished by `Content-Type`).
pub async fn upload_lyrics(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
    body: web::Bytes,
) -> HttpResponse {
    let Some(track) = resolve_audio_track(&state, &path.into_inner()).await else {
        return HttpResponse::NotFound().finish();
    };
    let ct = req
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();
    let track_path = PathBuf::from(&track.path);
    if let Err(e) = super::lyrics::write_sidecar(&track_path, &body, &ct) {
        tracing::error!("jellyfin: upload_lyrics {}: {e}", track.id);
        return HttpResponse::InternalServerError().finish();
    }
    // Return the freshly-parsed lyrics so clients don't need a follow-up GET.
    let Some(sidecar) = super::lyrics::find_sidecar(&track_path) else {
        return HttpResponse::NoContent().finish();
    };
    match super::lyrics::parse_sidecar(&sidecar) {
        Some(dto) => HttpResponse::Ok().json(dto),
        None => HttpResponse::NoContent().finish(),
    }
}

/// `DELETE /Audio/{itemId}/Lyrics` — remove sidecars. Idempotent.
pub async fn delete_lyrics(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let Some(track) = resolve_audio_track(&state, &path.into_inner()).await else {
        return HttpResponse::NotFound().finish();
    };
    let track_path = PathBuf::from(&track.path);
    super::lyrics::delete_sidecar(&track_path);
    HttpResponse::NoContent().finish()
}

/// `GET /Audio/{itemId}/RemoteSearch/Lyrics` — no remote providers
/// wired; returns an empty result so clients stop retrying.
pub async fn remote_search_lyrics(
    _user: AuthedUser,
    _state: web::Data<JellyfinState>,
    _path: web::Path<String>,
) -> HttpResponse {
    HttpResponse::Ok().json(Vec::<Value>::new())
}

/// `POST /Audio/{itemId}/RemoteSearch/Lyrics/{lyricId}` — no remote
/// providers, so a download attempt always 404s.
pub async fn remote_download_lyrics(
    _user: AuthedUser,
    _state: web::Data<JellyfinState>,
    _path: web::Path<(String, String)>,
) -> HttpResponse {
    HttpResponse::NotFound().finish()
}

/// `GET /Providers/Lyrics` — no remote providers configured.
pub async fn lyric_providers(_user: AuthedUser) -> HttpResponse {
    HttpResponse::Ok().json(Vec::<Value>::new())
}

// ── Similar ─────────────────────────────────────────────────────────────────

fn similar_limit(req: &HttpRequest) -> usize {
    let query = collect_query(req);
    query
        .get("limit")
        .or_else(|| query.get("Limit"))
        .and_then(|v| v.first())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20)
}

async fn build_similar_response(
    state: &JellyfinState,
    kind: &'static str,
    native: String,
    limit: usize,
) -> HttpResponse {
    let result = super::similar::similar(
        &state.pool,
        state.lastfm.as_ref(),
        state.musicbrainz.as_ref(),
        kind,
        &native,
        limit,
    )
    .await;

    let mut dtos: Vec<BaseItemDto> = Vec::new();
    for a in &result.artists {
        dtos.push(artist_to_dto(state, a).await);
    }
    for a in &result.albums {
        dtos.push(album_to_dto(state, a).await);
    }
    for t in &result.tracks {
        dtos.push(track_to_dto(state, t).await);
    }
    let total = dtos.len() as i32;
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: 0,
    })
}

/// `GET /Items/{itemId}/Similar` — the OpenAPI spec's generic
/// dispatcher. Peeks at the item kind and calls into the plugin
/// orchestrator; returns an empty `ItemsResult` when Last.fm is not
/// configured or when the kind isn't one the orchestrator supports.
pub async fn similar_items(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_ARTIST, native)) => {
            build_similar_response(&state, mapping::KIND_ARTIST, native, similar_limit(&req)).await
        }
        Some((mapping::KIND_ALBUM, native)) => {
            build_similar_response(&state, mapping::KIND_ALBUM, native, similar_limit(&req)).await
        }
        Some((mapping::KIND_TRACK, native)) => {
            build_similar_response(&state, mapping::KIND_TRACK, native, similar_limit(&req)).await
        }
        // Anything else (playlists, unknown ids) → empty.
        _ => HttpResponse::Ok().json(ItemsResult {
            items: vec![],
            total_record_count: 0,
            start_index: 0,
        }),
    }
}

pub async fn similar_artists_endpoint(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_ARTIST, native)) => {
            build_similar_response(&state, mapping::KIND_ARTIST, native, similar_limit(&req)).await
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

pub async fn similar_albums_endpoint(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let guid = mapping::normalize_guid(&path.into_inner());
    match resolve_favorite_target(&state, &guid).await {
        Some((mapping::KIND_ALBUM, native)) => {
            build_similar_response(&state, mapping::KIND_ALBUM, native, similar_limit(&req)).await
        }
        _ => HttpResponse::NotFound().finish(),
    }
}

// ── RemoteImages ────────────────────────────────────────────────────────────

/// Resolve `itemId` to an album — the only kind rockbox currently
/// surfaces remote images for. Track ids fall back to their parent
/// album so clients can "change cover art" from a now-playing view.
async fn resolve_remote_image_album(state: &JellyfinState, guid: &str) -> Option<Album> {
    let g = mapping::normalize_guid(guid);
    let (kind, native) = resolve_native(state, &g).await?;
    match kind.as_str() {
        "album" => repo::album::find(state.pool.clone(), &native).await.ok()?,
        "track" => {
            let track = repo::track::find(state.pool.clone(), &native)
                .await
                .ok()??;
            repo::album::find(state.pool.clone(), &track.album_id)
                .await
                .ok()?
        }
        _ => None,
    }
}

async fn find_remote_images_for_album(
    state: &JellyfinState,
    album: &Album,
) -> Vec<RemoteImageInfo> {
    let (Some(mb), Some(caa)) = (state.musicbrainz.as_ref(), state.caa.as_ref()) else {
        return Vec::new();
    };
    let Some(mbid) = mb.search_release_group(&album.artist, &album.title).await else {
        return Vec::new();
    };
    let images = caa.release_group_images(&mbid).await;
    let mut out = Vec::with_capacity(images.len());
    for img in images {
        let image_type = if img.is_front || img.image_types.iter().any(|t| t == "Front") {
            "Primary"
        } else if img.image_types.iter().any(|t| t == "Back") {
            "Backdrop"
        } else if img.image_types.iter().any(|t| t == "Booklet") {
            "Thumb"
        } else {
            "Primary"
        };
        out.push(RemoteImageInfo {
            provider_name: super::cover_art_archive::PROVIDER_NAME.to_string(),
            url: img.url,
            thumbnail_url: img.thumbnail_url,
            image_type: image_type.to_string(),
            rating_type: "Score",
            ..Default::default()
        });
    }
    out
}

/// `GET /Items/{itemId}/RemoteImages` — returns candidate remote
/// images for the item. Empty result when either MB or CAA is
/// unconfigured, or when the seed isn't an album/track.
pub async fn remote_images(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(album) = resolve_remote_image_album(&state, &path.into_inner()).await else {
        return HttpResponse::Ok().json(RemoteImageResult::default());
    };
    let query = collect_query(&req);
    let want_type = query
        .get("type")
        .or_else(|| query.get("Type"))
        .and_then(|v| v.first())
        .cloned();
    let start = query
        .get("startIndex")
        .or_else(|| query.get("StartIndex"))
        .and_then(|v| v.first())
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = query
        .get("limit")
        .or_else(|| query.get("Limit"))
        .and_then(|v| v.first())
        .and_then(|s| s.parse::<usize>().ok());

    let mut images = find_remote_images_for_album(&state, &album).await;
    if let Some(t) = want_type.as_deref() {
        images.retain(|img| img.image_type.eq_ignore_ascii_case(t));
    }
    let total = images.len() as i32;
    let providers = if state.caa.is_some() {
        vec![super::cover_art_archive::PROVIDER_NAME.to_string()]
    } else {
        vec![]
    };
    let mut page: Vec<RemoteImageInfo> = images.into_iter().skip(start).collect();
    if let Some(n) = limit {
        page.truncate(n);
    }
    HttpResponse::Ok().json(RemoteImageResult {
        images: page,
        total_record_count: total,
        providers,
    })
}

/// `GET /Items/{itemId}/RemoteImages/Providers` — enumerate providers
/// active for this item.
pub async fn remote_image_providers(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let Some(_album) = resolve_remote_image_album(&state, &path.into_inner()).await else {
        return HttpResponse::Ok().json(Vec::<ImageProviderInfo>::new());
    };
    let mut providers = Vec::new();
    if state.caa.is_some() {
        providers.push(ImageProviderInfo {
            name: super::cover_art_archive::PROVIDER_NAME.to_string(),
            supported_images: vec!["Primary", "Backdrop", "Thumb"],
        });
    }
    HttpResponse::Ok().json(providers)
}

/// `POST /Items/{itemId}/RemoteImages/Download?type=…&imageUrl=…&providerName=…`
/// — fetch the image bytes, save under the covers directory, and
/// point the album's `album_art` at the new file.
pub async fn download_remote_image(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    let Some(album) = resolve_remote_image_album(&state, &path.into_inner()).await else {
        return HttpResponse::NotFound().finish();
    };
    let query = collect_query(&req);
    let Some(url) = query
        .get("imageUrl")
        .or_else(|| query.get("ImageUrl"))
        .and_then(|v| v.first())
        .cloned()
    else {
        return HttpResponse::BadRequest().finish();
    };

    // Reuse the CAA HTTP client (same UA / TLS stack) when configured;
    // fall back to a raw reqwest for spec compliance if not.
    let bytes = match state.caa.as_ref() {
        Some(_) => reqwest::get(&url).await,
        None => reqwest::get(&url).await,
    };
    let response = match bytes {
        Ok(r) if r.status().is_success() => r,
        _ => return HttpResponse::BadGateway().finish(),
    };
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let ext = match content_type.as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    };
    let body = match response.bytes().await {
        Ok(b) => b,
        Err(_) => return HttpResponse::BadGateway().finish(),
    };

    let filename = format!("{}.{ext}", album_key_hash(&album.id));
    let covers_dir = covers_directory();
    if let Err(e) = std::fs::create_dir_all(&covers_dir) {
        tracing::error!("jellyfin: create covers dir {covers_dir:?}: {e}");
        return HttpResponse::InternalServerError().finish();
    }
    let out_path = covers_dir.join(&filename);
    if let Err(e) = std::fs::write(&out_path, &body) {
        tracing::error!("jellyfin: write cover {out_path:?}: {e}");
        return HttpResponse::InternalServerError().finish();
    }
    if let Err(e) = repo::album::update_album_art(state.pool.clone(), &album.id, &filename).await {
        tracing::error!("jellyfin: update album_art for {}: {e}", album.id);
        return HttpResponse::InternalServerError().finish();
    }
    HttpResponse::NoContent().finish()
}

/// Reuse the same on-disk covers path the audio scanner writes to so
/// external tools already pointing at `~/.config/rockbox.org/covers`
/// keep working.
fn covers_directory() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(format!("{home}/.config/rockbox.org/covers"))
}

/// Stable per-album key using MD5, matching the naming scheme
/// `library/album_art.rs` uses so the scanner and Jellyfin write to
/// the same filename for the same album.
fn album_key_hash(album_id: &str) -> String {
    use md5::{Digest, Md5};
    let mut hasher = Md5::new();
    hasher.update(album_id.as_bytes());
    hex::encode(hasher.finalize())
}

// ── Genres ──────────────────────────────────────────────────────────────────

use rockbox_library::entity::genre::Genre;

/// Build a `MusicGenre` `BaseItemDto`. Reuses `remember_genre` so the
/// GUID sticks between requests and can be resolved through
/// `/Items/{id}`.
async fn genre_to_dto(state: &JellyfinState, g: &Genre) -> BaseItemDto {
    let id = mapping::remember_genre(&state.pool, &g.id)
        .await
        .unwrap_or_else(|_| mapping::guid(mapping::KIND_GENRE, &g.id));
    let track_count = repo::genre::find_tracks(state.pool.clone(), &g.id)
        .await
        .map(|v| v.len() as i32)
        .unwrap_or(0);
    let album_count = repo::genre::find_albums(state.pool.clone(), &g.id)
        .await
        .map(|v| v.len() as i32)
        .unwrap_or(0);
    BaseItemDto {
        id,
        server_id: Some(state.server_id.clone()),
        name: Some(g.name.clone()),
        item_type: "MusicGenre",
        media_type: "Unknown",
        is_folder: Some(true),
        sort_name: Some(g.name.clone()),
        location_type: Some("FileSystem"),
        overview: g.description.clone(),
        song_count: Some(track_count),
        album_count: Some(album_count),
        child_count: Some(track_count),
        ..Default::default()
    }
}

fn matches_name_range(
    name: &str,
    starts: Option<&str>,
    geq: Option<&str>,
    lt: Option<&str>,
) -> bool {
    let lower = name.to_lowercase();
    if let Some(s) = starts {
        if !lower.starts_with(&s.to_lowercase()) {
            return false;
        }
    }
    if let Some(s) = geq {
        if lower.as_str() < s.to_lowercase().as_str() {
            return false;
        }
    }
    if let Some(s) = lt {
        if lower.as_str() >= s.to_lowercase().as_str() {
            return false;
        }
    }
    true
}

async fn list_genres_q(state: &JellyfinState, q: &ItemsQuery) -> HttpResponse {
    let mut genres = repo::genre::all(state.pool.clone())
        .await
        .unwrap_or_default();

    if let Some(term) = q.search_term.as_deref().filter(|s| !s.is_empty()) {
        let needle = term.to_lowercase();
        genres.retain(|g| g.name.to_lowercase().contains(&needle));
    }
    let starts = q.name_starts_with.as_deref();
    let geq = q.name_starts_with_or_greater.as_deref();
    let lt = q.name_less_than.as_deref();
    if starts.is_some() || geq.is_some() || lt.is_some() {
        genres.retain(|g| matches_name_range(&g.name, starts, geq, lt));
    }
    genres.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let total = genres.len() as i32;
    let start = q.start_index.unwrap_or(0).max(0) as usize;
    let limit = q.limit.unwrap_or(500).max(1) as usize;
    let slice: Vec<&Genre> = genres.iter().skip(start).take(limit).collect();
    let mut dtos = Vec::with_capacity(slice.len());
    for g in slice {
        dtos.push(genre_to_dto(state, g).await);
    }
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: start as i32,
    })
}

/// `GET /Genres` / `GET /MusicGenres` — sorted list of genres with
/// child counts. Rockbox is audio-only so both routes share a body.
pub async fn genres(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    list_genres_q(&state, &q).await
}

async fn list_by_genre(state: &JellyfinState, q: &ItemsQuery, genre_native: &str) -> HttpResponse {
    if includes(&q.include_item_types, "MusicAlbum") {
        let albums = repo::genre::find_albums(state.pool.clone(), genre_native)
            .await
            .unwrap_or_default();
        let mut dtos = Vec::with_capacity(albums.len());
        for a in &albums {
            dtos.push(album_to_dto(state, a).await);
        }
        let total = dtos.len() as i32;
        return HttpResponse::Ok().json(ItemsResult {
            items: dtos,
            total_record_count: total,
            start_index: 0,
        });
    }
    if includes(&q.include_item_types, "MusicArtist")
        || includes(&q.include_item_types, "AlbumArtist")
    {
        let artists = repo::genre::find_artists(state.pool.clone(), genre_native)
            .await
            .unwrap_or_default();
        let mut dtos = Vec::with_capacity(artists.len());
        for a in &artists {
            dtos.push(artist_to_dto(state, a).await);
        }
        let total = dtos.len() as i32;
        return HttpResponse::Ok().json(ItemsResult {
            items: dtos,
            total_record_count: total,
            start_index: 0,
        });
    }
    // Default (Audio or unspecified): tracks in the genre.
    let tracks = repo::genre::find_tracks(state.pool.clone(), genre_native)
        .await
        .unwrap_or_default();
    let mut dtos = Vec::with_capacity(tracks.len());
    for t in &tracks {
        dtos.push(track_to_dto(state, t).await);
    }
    let total = dtos.len() as i32;
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: 0,
    })
}

async fn resolve_genre_by_name(state: &JellyfinState, name: &str) -> Option<Genre> {
    if let Ok(Some(g)) = repo::genre::find_by_name(state.pool.clone(), name).await {
        return Some(g);
    }
    let all = repo::genre::all(state.pool.clone()).await.ok()?;
    all.into_iter().find(|g| g.name.eq_ignore_ascii_case(name))
}

/// `GET /Genres/{name}` / `GET /MusicGenres/{name}` — resolve a genre
/// by name (with a case-insensitive fallback) and return the DTO.
pub async fn genre_by_name(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let name = path.into_inner();
    match resolve_genre_by_name(&state, &name).await {
        Some(g) => HttpResponse::Ok().json(genre_to_dto(&state, &g).await),
        None => HttpResponse::NotFound().finish(),
    }
}

/// `GET /Users/{userId}/Genres/{name}` — legacy pre-10.9 alias.
pub async fn user_genre_by_name(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (_uid, name) = path.into_inner();
    match resolve_genre_by_name(&state, &name).await {
        Some(g) => HttpResponse::Ok().json(genre_to_dto(&state, &g).await),
        None => HttpResponse::NotFound().finish(),
    }
}

// ── Filters ─────────────────────────────────────────────────────────────────

async fn compute_filters(state: &JellyfinState) -> (Vec<Genre>, Vec<i32>) {
    let mut genres = repo::genre::all(state.pool.clone())
        .await
        .unwrap_or_default();
    genres.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    // Years — distinct non-zero album years, ascending.
    let year_rows: Vec<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT year FROM album WHERE year IS NOT NULL AND year > 0 ORDER BY year",
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    let years = year_rows.into_iter().map(|(y,)| y as i32).collect();
    (genres, years)
}

/// `GET /Items/Filters` — legacy filter dump. Genres are flat strings;
/// tags / official ratings stay empty because rockbox doesn't track
/// them today (the fields still need to be present arrays per spec).
pub async fn items_filters_legacy(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
) -> HttpResponse {
    let (genres, years) = compute_filters(&state).await;
    HttpResponse::Ok().json(super::dto::QueryFiltersLegacy {
        genres: genres.into_iter().map(|g| g.name).collect(),
        tags: vec![],
        official_ratings: vec![],
        years,
    })
}

/// `GET /Items/Filters2` — 10.9+ variant. Genres come back as
/// `NameGuidPair`s so clients can round-trip a chip through
/// `?genreIds=<guid>` without a second name→guid lookup.
pub async fn items_filters2(_user: AuthedUser, state: web::Data<JellyfinState>) -> HttpResponse {
    let (genres, years) = compute_filters(&state).await;
    let mut genre_pairs: Vec<NameGuidPair> = Vec::with_capacity(genres.len());
    for g in &genres {
        let id = mapping::remember_genre(&state.pool, &g.id)
            .await
            .unwrap_or_else(|_| mapping::guid(mapping::KIND_GENRE, &g.id));
        genre_pairs.push(NameGuidPair {
            name: Some(g.name.clone()),
            id,
        });
    }
    HttpResponse::Ok().json(super::dto::QueryFilters {
        genres: genre_pairs,
        tags: vec![],
        official_ratings: vec![],
        years,
    })
}

/// `GET /Users/{userId}/Items/Filters` — legacy alias, same body as
/// [`items_filters_legacy`].
pub async fn user_items_filters(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    _path: web::Path<String>,
) -> HttpResponse {
    let (genres, years) = compute_filters(&state).await;
    HttpResponse::Ok().json(super::dto::QueryFiltersLegacy {
        genres: genres.into_iter().map(|g| g.name).collect(),
        tags: vec![],
        official_ratings: vec![],
        years,
    })
}

// ── Item counts ─────────────────────────────────────────────────────────────

/// `GET /Items/Counts` — library-summary strip. Rockbox is audio-only
/// so movie / series / episode / trailer / book / boxset counts stay
/// at 0. `ItemCount` sums the three audio kinds we do surface.
pub async fn item_counts(_user: AuthedUser, state: web::Data<JellyfinState>) -> HttpResponse {
    let song_count = repo::track::count_filtered(state.pool.clone(), None, None, None)
        .await
        .unwrap_or(0) as i32;
    let album_count = repo::album::count_filtered(state.pool.clone(), None, None, None)
        .await
        .unwrap_or(0) as i32;
    let artist_count = repo::artist::count_filtered(state.pool.clone(), None, None, None)
        .await
        .unwrap_or(0) as i32;
    HttpResponse::Ok().json(super::dto::ItemCounts {
        song_count,
        album_count,
        artist_count,
        item_count: song_count + album_count + artist_count,
        ..Default::default()
    })
}

// ── Artists endpoints ───────────────────────────────────────────────────────

pub async fn artists(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    let limit = q.limit.unwrap_or(500).max(1);
    let offset = q.start_index.unwrap_or(0).max(0);
    let starts = q.name_starts_with.as_deref();
    let geq = q.name_starts_with_or_greater.as_deref();
    let lt = q.name_less_than.as_deref();
    let artists = repo::artist::filtered(state.pool.clone(), starts, geq, lt, limit, offset)
        .await
        .unwrap_or_default();
    let total = repo::artist::count_filtered(state.pool.clone(), starts, geq, lt)
        .await
        .unwrap_or(0) as i32;
    let mut dtos = Vec::with_capacity(artists.len());
    for a in &artists {
        dtos.push(artist_to_dto(&state, a).await);
    }
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: offset as i32,
    })
}

/// `/Items/Prefixes?ParentId=<lib>&IncludeItemTypes=...` — populates a
/// client's alpha-jump rail. Returns the distinct uppercase first letters
/// that actually exist for the requested item type, with "#" for non-alpha
/// names. Response shape: `[{"Name":"A"}, …]`.
pub async fn items_prefixes(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    let parent_norm = q.parent_id.as_deref().map(mapping::normalize_guid);
    let parent_is_music = parent_norm
        .as_deref()
        .map(|g| g == mapping::library_guid())
        .unwrap_or(false);
    let parent_is_playlists = parent_norm
        .as_deref()
        .map(|g| g == mapping::playlists_library_guid())
        .unwrap_or(false);

    if parent_is_playlists || includes(&q.include_item_types, "Playlist") {
        let playlists = state.playlist_store.list().await.unwrap_or_default();
        let mut letters: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for p in &playlists {
            if let Some(c) = p.name.chars().next() {
                if c.is_ascii_alphabetic() {
                    letters.insert(c.to_ascii_uppercase().to_string());
                } else {
                    letters.insert("#".to_string());
                }
            }
        }
        let items: Vec<Value> = letters.into_iter().map(|n| json!({ "Name": n })).collect();
        return HttpResponse::Ok().json(items);
    }

    let letters = if includes(&q.include_item_types, "MusicAlbum") {
        repo::album::name_prefixes(state.pool.clone()).await
    } else if includes(&q.include_item_types, "Audio") {
        repo::track::name_prefixes(state.pool.clone()).await
    } else if includes(&q.include_item_types, "MusicArtist")
        || includes(&q.include_item_types, "AlbumArtist")
        || parent_is_music
    {
        repo::artist::name_prefixes(state.pool.clone()).await
    } else {
        Ok(Vec::new())
    };

    let items: Vec<Value> = letters
        .unwrap_or_default()
        .into_iter()
        .map(|n| json!({ "Name": n }))
        .collect();
    HttpResponse::Ok().json(items)
}

/// `/Artists/Prefixes` — same as `/Items/Prefixes?IncludeItemTypes=MusicArtist`
/// but the URL itself implies the type, so no query-string hints are needed.
pub async fn artists_prefixes(_user: AuthedUser, state: web::Data<JellyfinState>) -> HttpResponse {
    let letters = repo::artist::name_prefixes(state.pool.clone())
        .await
        .unwrap_or_default();
    let items: Vec<Value> = letters.into_iter().map(|n| json!({ "Name": n })).collect();
    HttpResponse::Ok().json(items)
}

pub async fn artist_by_name(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let name = path.into_inner();
    if let Ok(Some(a)) = repo::artist::find_by_name(state.pool.clone(), &name).await {
        let _ =
            super::enrichment::refresh_artist(&state.pool, state.lastfm.as_ref(), &a.id, &a.name)
                .await;
        HttpResponse::Ok().json(artist_to_dto(&state, &a).await)
    } else {
        HttpResponse::NotFound().finish()
    }
}

// ── Images ───────────────────────────────────────────────────────────────────

/// Resolve a Jellyfin item GUID to a stored `album_art` / `image` value.
/// The returned string is one of:
///   - a bare filename like `"abc123.jpg"` → lives under `~/.config/rockbox.org/covers/`
///   - an absolute filesystem path starting with `/`
///   - an `http(s)://` URL (artist images sourced from Rocksky)
async fn resolve_image_value(state: &JellyfinState, guid: &str) -> Option<String> {
    let g = mapping::normalize_guid(guid);
    let (kind, native) = mapping::lookup(&state.pool, &g).await.ok().flatten()?;
    match kind.as_str() {
        "album" => album_art_value(state, &native).await,
        "track" => {
            let t = repo::track::find(state.pool.clone(), &native)
                .await
                .ok()
                .flatten()?;
            if let Some(art) = t.album_art.clone() {
                return Some(art);
            }
            if !t.album_id.is_empty() {
                if let Some(a) = album_art_value(state, &t.album_id).await {
                    return Some(a);
                }
            }
            None
        }
        "artist" => repo::artist::find(state.pool.clone(), &native)
            .await
            .ok()
            .flatten()
            .and_then(|a| a.image),
        _ => None,
    }
}

/// Find album art either on the album row directly, or on any of its tracks.
async fn album_art_value(state: &JellyfinState, album_id: &str) -> Option<String> {
    if let Ok(Some(album)) = repo::album::find(state.pool.clone(), album_id).await {
        if let Some(art) = album.album_art {
            return Some(art);
        }
    }
    repo::album_tracks::find_by_album(state.pool.clone(), album_id)
        .await
        .ok()?
        .into_iter()
        .find_map(|t| t.album_art)
}

pub async fn item_image(
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (item_guid, _kind) = path.into_inner();
    serve_image(&state, &item_guid).await
}

pub async fn item_image_by_index(
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String, i32)>,
) -> HttpResponse {
    let (item_guid, _kind, _idx) = path.into_inner();
    serve_image(&state, &item_guid).await
}

async fn serve_image(state: &JellyfinState, item_guid: &str) -> HttpResponse {
    let Some(art) = resolve_image_value(state, item_guid).await else {
        return HttpResponse::NotFound().finish();
    };
    serve_art_value(&art).await
}

async fn serve_art_value(art: &str) -> HttpResponse {
    // Remote artist images (Rocksky) — proxy via reqwest.
    if art.starts_with("http://") || art.starts_with("https://") {
        match reqwest::get(art).await {
            Ok(resp) => {
                let mime = resp
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("image/jpeg")
                    .to_string();
                match resp.bytes().await {
                    Ok(data) => HttpResponse::Ok().content_type(mime).body(data.to_vec()),
                    Err(_) => HttpResponse::NotFound().finish(),
                }
            }
            Err(e) => {
                tracing::warn!("jellyfin image proxy {art}: {e}");
                HttpResponse::NotFound().finish()
            }
        }
    } else {
        // Bare filename → resolve under ~/.config/rockbox.org/covers/.
        // Absolute path → use as-is.
        let full = if art.starts_with('/') {
            art.to_string()
        } else {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.config/rockbox.org/covers/{}", home, art)
        };
        match std::fs::read(&full) {
            Ok(data) => {
                let mime = mime_guess::from_path(&full)
                    .first_or_octet_stream()
                    .to_string();
                HttpResponse::Ok().content_type(mime).body(data)
            }
            Err(e) => {
                tracing::warn!("jellyfin image read {full}: {e}");
                HttpResponse::NotFound().finish()
            }
        }
    }
}

// ── PlaybackInfo + stream ───────────────────────────────────────────────────

pub async fn playback_info(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
) -> HttpResponse {
    let g = mapping::normalize_guid(&path.into_inner());
    let Some((kind, native)) = resolve_native(&state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "track" {
        return HttpResponse::BadRequest().finish();
    }
    let Ok(Some(t)) = repo::track::find(state.pool.clone(), &native).await else {
        return HttpResponse::NotFound().finish();
    };
    let dto = track_to_dto(&state, &t).await;
    HttpResponse::Ok().json(PlaybackInfoResponse {
        media_sources: dto.media_sources.unwrap_or_default(),
        play_session_id: Some(auth::random_hex(8)),
    })
}

fn content_type_for(container: &str) -> &'static str {
    match container {
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "ogg" | "oga" => "audio/ogg",
        "opus" => "audio/opus",
        "m4a" | "aac" | "mp4" => "audio/mp4",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

async fn audio_by_guid(state: &JellyfinState, guid: &str, req: &HttpRequest) -> HttpResponse {
    let token = auth::extract_token(req);
    let authorized = match token {
        Some(t) => auth::token_valid(&state.pool, &t).await,
        None => false,
    };
    if !authorized {
        return HttpResponse::Unauthorized().finish();
    }
    let g = mapping::normalize_guid(guid);
    let Some((kind, native)) = resolve_native(state, &g).await else {
        return HttpResponse::NotFound().finish();
    };
    if kind != "track" {
        return HttpResponse::BadRequest().finish();
    }
    let Ok(Some(t)) = repo::track::find(state.pool.clone(), &native).await else {
        return HttpResponse::NotFound().finish();
    };
    let container = std::path::Path::new(&t.path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "mp3".to_string());
    serve_file(&t.path, content_type_for(&container), req)
}

pub async fn audio_stream(
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    audio_by_guid(&state, &path.into_inner(), &req).await
}

pub async fn audio_stream_ext(
    state: web::Data<JellyfinState>,
    path: web::Path<(String, String)>,
    req: HttpRequest,
) -> HttpResponse {
    let (id, _ext) = path.into_inner();
    audio_by_guid(&state, &id, &req).await
}

pub async fn audio_universal(
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    audio_by_guid(&state, &path.into_inner(), &req).await
}

pub async fn item_file_stream(
    state: web::Data<JellyfinState>,
    path: web::Path<String>,
    req: HttpRequest,
) -> HttpResponse {
    audio_by_guid(&state, &path.into_inner(), &req).await
}

fn serve_file(path_str: &str, content_type: &str, req: &HttpRequest) -> HttpResponse {
    let path = PathBuf::from(path_str);
    let file_size = match std::fs::metadata(&path) {
        Ok(m) => m.len(),
        Err(e) => {
            tracing::error!("jellyfin stream stat {path_str}: {e}");
            return HttpResponse::InternalServerError().finish();
        }
    };

    if let Some(range_hdr) = req.headers().get(actix_web::http::header::RANGE) {
        if let Ok(range_str) = range_hdr.to_str() {
            if let Some(range) = range_str.strip_prefix("bytes=") {
                let mut parts = range.splitn(2, '-');
                let start: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                let end: u64 = parts
                    .next()
                    .filter(|s| !s.is_empty())
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(file_size.saturating_sub(1))
                    .min(file_size.saturating_sub(1));
                if start <= end && file_size > 0 {
                    use std::io::{Read, Seek, SeekFrom};
                    return match std::fs::File::open(&path) {
                        Ok(mut file) => {
                            let _ = file.seek(SeekFrom::Start(start));
                            let length = (end - start + 1) as usize;
                            let mut buf = vec![0u8; length];
                            let n = file.read(&mut buf).unwrap_or(0);
                            buf.truncate(n);
                            let actual_end = start + n as u64 - 1;
                            HttpResponse::PartialContent()
                                .content_type(content_type.to_string())
                                .insert_header(("Accept-Ranges", "bytes"))
                                .insert_header(("Content-Length", n.to_string()))
                                .insert_header((
                                    "Content-Range",
                                    format!("bytes {start}-{actual_end}/{file_size}"),
                                ))
                                .body(buf)
                        }
                        Err(e) => {
                            tracing::error!("jellyfin stream open {path_str}: {e}");
                            HttpResponse::InternalServerError().finish()
                        }
                    };
                }
            }
        }
    }

    match std::fs::read(&path) {
        Ok(data) => HttpResponse::Ok()
            .content_type(content_type.to_string())
            .insert_header(("Accept-Ranges", "bytes"))
            .insert_header(("Content-Length", file_size.to_string()))
            .body(data),
        Err(e) => {
            tracing::error!("jellyfin stream read {path_str}: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}

// ── Sessions / scrobble ──────────────────────────────────────────────────────

pub async fn sessions_capabilities(_user: AuthedUser) -> HttpResponse {
    HttpResponse::NoContent().finish()
}

pub async fn sessions_list(_user: AuthedUser) -> HttpResponse {
    HttpResponse::Ok().json(Vec::<Value>::new())
}

pub async fn sessions_playing(_user: AuthedUser, _body: web::Json<Value>) -> HttpResponse {
    HttpResponse::NoContent().finish()
}

pub async fn sessions_playing_progress(_user: AuthedUser, _body: web::Json<Value>) -> HttpResponse {
    HttpResponse::NoContent().finish()
}

pub async fn sessions_playing_stopped(_user: AuthedUser, _body: web::Json<Value>) -> HttpResponse {
    HttpResponse::NoContent().finish()
}

pub async fn user_played_item(
    _user: AuthedUser,
    _state: web::Data<JellyfinState>,
    _path: web::Path<(String, String)>,
) -> HttpResponse {
    HttpResponse::NoContent().finish()
}

// ── Misc stubs ──────────────────────────────────────────────────────────────

pub async fn empty_array() -> HttpResponse {
    HttpResponse::Ok().json(Vec::<Value>::new())
}

pub async fn empty_items() -> HttpResponse {
    HttpResponse::Ok().json(ItemsResult {
        items: vec![],
        total_record_count: 0,
        start_index: 0,
    })
}

pub async fn no_content() -> HttpResponse {
    HttpResponse::NoContent().finish()
}

/// Routed 404 — same wire result as the default service, but bypasses the
/// `log_unrouted` warning. Use for endpoints that *should* respond 404
/// (no user avatar, unsupported WebSocket upgrade) so the log stays
/// focused on genuinely missing paths.
pub async fn not_found() -> HttpResponse {
    HttpResponse::NotFound().finish()
}

/// `GET/HEAD /System/Ping` — Jellyfin's heartbeat. Reference server returns
/// plain text "Jellyfin Server"; some clients (Moonfin, official web) use
/// this to decide if the server is reachable.
pub async fn system_ping() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; charset=utf-8")
        .body("Jellyfin Server")
}

pub async fn branding_config() -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "LoginDisclaimer": "",
        "CustomCss": "",
        "SplashscreenEnabled": false,
    }))
}

pub async fn displaypreferences(_user: AuthedUser) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "Id": "",
        "ViewType": "",
        "SortBy": "SortName",
        "SortOrder": "Ascending",
        "RememberIndexing": false,
        "PrimaryImageHeight": 250,
        "PrimaryImageWidth": 250,
        "CustomPrefs": {},
        "ScrollDirection": "Vertical",
        "ShowBackdrop": true,
        "RememberSorting": false,
        "IndexBy": "",
        "ShowSidebar": false,
        "Client": ""
    }))
}

// ── /Items/Latest ────────────────────────────────────────────────────────────

pub async fn items_latest(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    let limit = q.limit.unwrap_or(16).max(1) as usize;

    // The "Latest" rail on the Playlists library page shows the newest
    // playlists, not albums.
    if let Some(parent) = q.parent_id.as_ref() {
        let g = mapping::normalize_guid(parent);
        if g == mapping::playlists_library_guid() {
            let playlists = state.playlist_store.list().await.unwrap_or_default();
            let take = limit.min(playlists.len());
            let mut dtos = Vec::with_capacity(take);
            for p in playlists.iter().take(take) {
                dtos.push(playlist_to_dto(&state, p).await);
            }
            return HttpResponse::Ok().json(dtos);
        }
    }

    let mut albums = repo::album::all(state.pool.clone())
        .await
        .unwrap_or_default();
    albums.truncate(limit);
    let mut dtos = Vec::with_capacity(albums.len());
    for a in &albums {
        dtos.push(album_to_dto(&state, a).await);
    }
    HttpResponse::Ok().json(dtos)
}

// ── Home rails ──────────────────────────────────────────────────────────────

fn home_rail_limit(q: &ItemsQuery, default: i64) -> i64 {
    q.limit.unwrap_or(default).max(1)
}

/// Resolve every track whose `jf_user_data.playback_position_ticks > 0`
/// into a full `Track`. Ordered by `updated_at DESC` so the most
/// recently-touched item leads.
async fn resume_tracks(state: &JellyfinState, limit: i64) -> Vec<Track> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT native_id FROM jf_user_data
         WHERE kind = 'track' AND playback_position_ticks > 0
         ORDER BY updated_at DESC
         LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let mut out = Vec::with_capacity(rows.len());
    for (tid,) in rows {
        if let Ok(Some(t)) = repo::track::find(state.pool.clone(), &tid).await {
            out.push(t);
        }
    }
    out
}

/// `/Items/Resume` and `/Users/{uid}/Items/Resume` — tracks with
/// non-zero `PlaybackPositionTicks`. Response is a full
/// `BaseItemDtoQueryResult` (`ItemsResult`), not the plain array used
/// by `/UserItems/Resume`.
pub async fn items_resume(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    let limit = home_rail_limit(&q, 12);
    let tracks = resume_tracks(&state, limit).await;
    let mut dtos = Vec::with_capacity(tracks.len());
    for t in &tracks {
        dtos.push(track_to_dto(&state, t).await);
    }
    let total = dtos.len() as i32;
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: 0,
    })
}

/// `/UserItems/Resume` — Findroid's legacy path, plain array of DTOs.
pub async fn user_items_resume_array(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    let limit = home_rail_limit(&q, 12);
    let tracks = resume_tracks(&state, limit).await;
    let mut dtos = Vec::with_capacity(tracks.len());
    for t in &tracks {
        dtos.push(track_to_dto(&state, t).await);
    }
    HttpResponse::Ok().json(dtos)
}

/// `/UserItems/Latest` — Findroid asks for a plain array of the
/// newest items across all libraries. Mirrors `items_latest`'s music
/// branch: the newest albums come back so the home page has a
/// populated "Recently Added" rail without the client having to know
/// about the music library GUID.
pub async fn user_items_latest_array(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    let limit = home_rail_limit(&q, 16) as usize;
    let mut albums = repo::album::all(state.pool.clone())
        .await
        .unwrap_or_default();
    albums.truncate(limit);
    let mut dtos = Vec::with_capacity(albums.len());
    for a in &albums {
        dtos.push(album_to_dto(&state, a).await);
    }
    HttpResponse::Ok().json(dtos)
}

/// `/Items/Suggestions` and `/Users/{uid}/Items/Suggestions` — the
/// "You might like" rail. Real Jellyfin routes this through a
/// recommendation engine; we return a random slice of the library
/// filtered by `mediaType` / `type` when supplied, defaulting to
/// audio tracks.
///
/// Order comes from SQLite's `RANDOM()` which gives a deterministic
/// shuffle per request but different results across calls — matches
/// how clients expect the rail to freshen each session.
pub async fn items_suggestions(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    req: HttpRequest,
) -> HttpResponse {
    let q = parse_items_query(&req);
    let limit = home_rail_limit(&q, 12);
    let query = collect_query(&req);
    let media_type = query
        .get("mediaType")
        .or_else(|| query.get("MediaType"))
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_default();
    let want_audio = media_type.eq_ignore_ascii_case("Audio")
        || media_type.is_empty()
        || includes(&q.include_item_types, "Audio");
    let want_album = includes(&q.include_item_types, "MusicAlbum");
    let want_artist = includes(&q.include_item_types, "MusicArtist");

    let mut dtos: Vec<BaseItemDto> = Vec::new();

    if want_album {
        let rows: Vec<rockbox_library::entity::album::Album> =
            sqlx::query_as("SELECT * FROM album ORDER BY RANDOM() LIMIT ?1")
                .bind(limit)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        for a in &rows {
            dtos.push(album_to_dto(&state, a).await);
        }
    } else if want_artist {
        let rows: Vec<Artist> = sqlx::query_as("SELECT * FROM artist ORDER BY RANDOM() LIMIT ?1")
            .bind(limit)
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();
        for a in &rows {
            dtos.push(artist_to_dto(&state, a).await);
        }
    } else if want_audio {
        let rows: Vec<Track> =
            sqlx::query_as("SELECT * FROM track WHERE is_remote = 0 ORDER BY RANDOM() LIMIT ?1")
                .bind(limit)
                .fetch_all(&state.pool)
                .await
                .unwrap_or_default();
        for t in &rows {
            dtos.push(track_to_dto(&state, t).await);
        }
    }

    let total = dtos.len() as i32;
    HttpResponse::Ok().json(ItemsResult {
        items: dtos,
        total_record_count: total,
        start_index: 0,
    })
}

// ── /Search/Hints ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SearchHintsQuery {
    pub search_term: Option<String>,
    pub limit: Option<i64>,
    pub start_index: Option<i64>,
    pub include_item_types: Option<String>,
    pub user_id: Option<String>,
}

pub async fn search_hints(
    _user: AuthedUser,
    state: web::Data<JellyfinState>,
    query: web::Query<SearchHintsQuery>,
) -> HttpResponse {
    let q = query.into_inner();
    let Some(term) = q.search_term.as_deref().filter(|s| !s.is_empty()) else {
        return HttpResponse::Ok().json(json!({
            "SearchHints": [],
            "TotalRecordCount": 0,
        }));
    };
    let limit = q.limit.unwrap_or(20).max(1);
    let pattern = like_pattern(term);
    let mut hints: Vec<Value> = Vec::new();
    let want_artist = q
        .include_item_types
        .as_deref()
        .map(|s| {
            s.split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("MusicArtist"))
        })
        .unwrap_or(true);
    let want_album = q
        .include_item_types
        .as_deref()
        .map(|s| {
            s.split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("MusicAlbum"))
        })
        .unwrap_or(true);
    let want_audio = q
        .include_item_types
        .as_deref()
        .map(|s| s.split(',').any(|t| t.trim().eq_ignore_ascii_case("Audio")))
        .unwrap_or(true);

    if want_artist {
        let w = ("name LIKE ?1".to_string(), vec![pattern.clone()]);
        if let Ok(rows) = repo::artist::filter(state.pool.clone(), w).await {
            for a in rows.into_iter().take(limit as usize) {
                let id = mapping::remember_artist(&state.pool, &a)
                    .await
                    .unwrap_or_else(|_| mapping::guid(mapping::KIND_ARTIST, &a.id));
                hints.push(json!({
                    "ItemId": id.clone(),
                    "Id": id,
                    "Name": a.name,
                    "Type": "MusicArtist",
                    "MediaType": "Unknown",
                    "IsFolder": true,
                }));
            }
        }
    }
    if want_album {
        let w = ("title LIKE ?1".to_string(), vec![pattern.clone()]);
        if let Ok(rows) = repo::album::filter(state.pool.clone(), w).await {
            for al in rows.into_iter().take(limit as usize) {
                let id = mapping::remember_album(&state.pool, &al)
                    .await
                    .unwrap_or_else(|_| mapping::guid(mapping::KIND_ALBUM, &al.id));
                hints.push(json!({
                    "ItemId": id.clone(),
                    "Id": id,
                    "Name": al.title,
                    "Album": al.title,
                    "AlbumArtist": al.artist,
                    "Type": "MusicAlbum",
                    "MediaType": "Unknown",
                    "IsFolder": true,
                }));
            }
        }
    }
    if want_audio {
        let w = ("title LIKE ?1".to_string(), vec![pattern]);
        if let Ok(rows) = repo::track::filter(state.pool.clone(), w).await {
            for t in rows.into_iter().take(limit as usize) {
                let id = mapping::remember_track(&state.pool, &t)
                    .await
                    .unwrap_or_else(|_| mapping::guid(mapping::KIND_TRACK, &t.id));
                hints.push(json!({
                    "ItemId": id.clone(),
                    "Id": id,
                    "Name": t.title,
                    "Album": t.album,
                    "AlbumArtist": t.album_artist,
                    "Type": "Audio",
                    "MediaType": "Audio",
                    "IsFolder": false,
                    "RunTimeTicks": (t.length as i64) * TICKS_PER_MS,
                }));
            }
        }
    }
    let total = hints.len() as i32;
    HttpResponse::Ok().json(json!({
        "SearchHints": hints,
        "TotalRecordCount": total,
    }))
}

// keep the warning quiet — TICKS_PER_SEC is reserved for future use.
#[allow(dead_code)]
const _RESERVED: i64 = TICKS_PER_SEC;
