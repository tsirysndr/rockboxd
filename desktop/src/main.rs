//! rockbox-desktop — skinnable Slint client for rockboxd.
//!
//! Talks to a running rockboxd over gRPC (127.0.0.1:6061 by default; override
//! with ROCKBOX_HOST / ROCKBOX_GRPC_PORT / ROCKBOX_GRAPHQL_PORT). All library
//! state lives on the UI thread in a thread_local; the tokio worker in rpc.rs
//! pushes plain data across via upgrade_in_event_loop.

mod daemon;
mod rpc;
mod servers;
mod skin;

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, VecModel};

slint::include_modules!();

struct AlbumEntry {
    data: rpc::AlbumData,
    image: Option<slint::Image>,
}

struct ArtistEntry {
    data: rpc::ArtistData,
    image: Option<slint::Image>,
}

#[derive(Default)]
struct UiState {
    albums: Vec<AlbumEntry>,
    artists: Vec<ArtistEntry>,
    tracks: Vec<rpc::TrackData>,
    liked: Vec<rpc::TrackData>,
    audio: rpc::AudioSettingsData,
    servers: Vec<servers::SavedServer>,
    /// Browse navigation stack of (title, path); top = current level.
    browse_stack: Vec<(String, String)>,
    playlists: Vec<rpc::PlaylistData>,
    discovered: Vec<(String, String, u16)>,
    switcher_query: String,
    active_server: String,
    pl_detail_id: Option<String>,
    pl_detail_track_ids: Vec<String>,
    picker_query: String,
    /// Heart clicks the daemon has not answered yet, keyed by track id.
    ///
    /// A like is never instant — the daemon still has a DB write and a gRPC
    /// round-trip to do — so the UI flips the icon on click and records the
    /// intent here, the same optimistic update the web client does. The entry
    /// is retired when that click's own response lands (see `settle_pending`).
    pending_likes: std::collections::HashMap<String, PendingLike>,
    /// Hands out `PendingLike::seq`. Monotonic, so a later click always wins
    /// over an earlier one's response.
    like_seq: u64,
}

/// An optimistic heart click awaiting its response.
#[derive(Clone, Copy, Debug)]
struct PendingLike {
    seq: u64,
    liked: bool,
}

thread_local! {
    static STATE: RefCell<UiState> = RefCell::new(UiState::default());
}

fn track_item_with(t: &rpc::TrackData, liked_ids: &std::collections::HashSet<String>) -> TrackItem {
    TrackItem {
        id: t.id.clone().into(),
        title: t.title.clone().into(),
        artist: t.artist.clone().into(),
        album: t.album.clone().into(),
        duration: rpc::format_time(t.length_ms as f64 / 1000.0).into(),
        index: t.index,
        track_no: if t.track_no > 0 {
            t.track_no
        } else {
            t.index + 1
        },
        liked: liked_ids.contains(&t.id),
        album_id: t.album_id.clone().into(),
    }
}

/// Retire the optimistic entry that `seq` answers.
///
/// Cleared whether the request succeeded or failed: the caller refetches the
/// liked list either way, so dropping the entry lets a rejected like fall back
/// to whatever the daemon actually has. A newer click on the same track carries
/// a higher `seq` and is left alone, so a slow response can't undo it.
fn settle_pending(
    pending: &mut std::collections::HashMap<String, PendingLike>,
    id: &str,
    seq: u64,
) {
    if pending.get(id).is_some_and(|p| p.seq == seq) {
        pending.remove(id);
    }
}

/// The set of track ids whose heart should read as filled: what the daemon
/// last told us, with any unsettled clicks applied on top.
fn liked_ids(st: &UiState) -> std::collections::HashSet<String> {
    let mut ids: std::collections::HashSet<String> =
        st.liked.iter().map(|t| t.id.clone()).collect();
    for (id, pending) in &st.pending_likes {
        if pending.liked {
            ids.insert(id.clone());
        } else {
            ids.remove(id);
        }
    }
    ids
}

fn album_item(a: &AlbumEntry) -> AlbumItem {
    AlbumItem {
        id: a.data.id.clone().into(),
        title: a.data.title.clone().into(),
        artist: a.data.artist.clone().into(),
        year: a.data.year.clone().into(),
        art: a.image.clone().unwrap_or_default(),
        has_art: a.image.is_some(),
    }
}

fn artist_item(a: &ArtistEntry) -> ArtistItem {
    ArtistItem {
        id: a.data.id.clone().into(),
        name: a.data.name.clone().into(),
        image: a.image.clone().unwrap_or_default(),
        has_image: a.image.is_some(),
    }
}

/// Called from the rpc worker (on the UI thread) when the library loads.
pub fn ui_set_library(app: &AppWindow, data: rpc::LibraryData) {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.albums = data
            .albums
            .into_iter()
            .map(|a| AlbumEntry {
                data: a,
                image: None,
            })
            .collect();
        st.artists = data
            .artists
            .into_iter()
            .map(|a| ArtistEntry {
                data: a,
                image: None,
            })
            .collect();
        st.tracks = data.tracks;
        st.liked = data.liked;

        let albums: Vec<AlbumItem> = st.albums.iter().map(album_item).collect();
        let artists: Vec<ArtistItem> = st.artists.iter().map(artist_item).collect();
        let ids = liked_ids(&st);
        let tracks: Vec<TrackItem> = st.tracks.iter().map(|t| track_item_with(t, &ids)).collect();
        let liked: Vec<TrackItem> = st.liked.iter().map(|t| track_item_with(t, &ids)).collect();

        app.set_albums(ModelRc::new(VecModel::from(albums)));
        app.set_artists(ModelRc::new(VecModel::from(artists)));
        app.set_tracks(ModelRc::new(VecModel::from(tracks)));
        app.set_liked(ModelRc::new(VecModel::from(liked)));
    });
}

/// Called per decoded album-art thumbnail.
pub fn ui_set_album_art(app: &AppWindow, idx: usize, w: u32, h: u32, rgba: Vec<u8>) {
    let image = slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(&rgba, w, h));
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if let Some(entry) = st.albums.get_mut(idx) {
            entry.image = Some(image.clone());
        }
    });
    let model = app.get_albums();
    if let Some(mut row) = model.row_data(idx) {
        row.art = image.clone();
        row.has_art = true;
        model.set_row_data(idx, row);
    }
    // Keep an open detail view in sync with the freshly loaded art.
    let mut detail = app.get_detail_album();
    if detail.id.as_str() == STATE.with(|s| s.borrow().albums[idx].data.id.clone()) {
        detail.art = image;
        detail.has_art = true;
        app.set_detail_album(detail);
    }
}

/// Called per decoded artist picture thumbnail.
pub fn ui_set_artist_image(app: &AppWindow, idx: usize, w: u32, h: u32, rgba: Vec<u8>) {
    let image = slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(&rgba, w, h));
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if let Some(entry) = st.artists.get_mut(idx) {
            entry.image = Some(image.clone());
        }
    });
    let model = app.get_artists();
    if let Some(mut row) = model.row_data(idx) {
        row.image = image;
        row.has_image = true;
        model.set_row_data(idx, row);
    }
}

pub fn ui_set_now_art(app: &AppWindow, w: u32, h: u32, rgba: Vec<u8>) {
    let image = slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(&rgba, w, h));
    app.set_now_art(image);
    // Blurred once here rather than per-frame in the UI: it backs the full
    // player and the player bar, and the thumbnail it comes from is small
    // enough that this is cheap.
    if let Some(buffer) = image::RgbaImage::from_raw(w, h, rgba) {
        let blurred = image::imageops::blur(&buffer, 18.0);
        app.set_now_art_blurred(slint::Image::from_rgba8(
            SharedPixelBuffer::clone_from_slice(
                blurred.as_raw(),
                blurred.width(),
                blurred.height(),
            ),
        ));
    }
    app.set_now_has_art(true);
}

pub fn ui_set_queue(
    app: &AppWindow,
    total: usize,
    upnext: Vec<rpc::TrackData>,
    history: Vec<rpc::TrackData>,
) {
    let ids = STATE.with(|s| liked_ids(&s.borrow()));
    let upnext: Vec<TrackItem> = upnext.iter().map(|t| track_item_with(t, &ids)).collect();
    let history: Vec<TrackItem> = history.iter().map(|t| track_item_with(t, &ids)).collect();
    app.set_queue_total(total as i32);
    app.set_queue_upnext(ModelRc::new(VecModel::from(upnext)));
    app.set_queue_history(ModelRc::new(VecModel::from(history)));
}

pub fn ui_show_album_detail(app: &AppWindow, detail: rpc::AlbumDetailData) {
    let (image, has_art) = STATE.with(|s| {
        s.borrow()
            .albums
            .iter()
            .find(|a| a.data.id == detail.id)
            .and_then(|a| a.image.clone())
            .map(|img| (img, true))
            .unwrap_or_default()
    });
    let liked_ids = STATE.with(|s| liked_ids(&s.borrow()));
    let total_s: u64 = detail.tracks.iter().map(|t| t.length_ms / 1000).sum();
    let duration = if total_s >= 3600 {
        format!("{} hr {} min", total_s / 3600, (total_s % 3600) / 60)
    } else {
        format!("{} min", (total_s / 60).max(1))
    };
    let mut meta = String::new();
    if !detail.year.is_empty() {
        meta.push_str(&detail.year);
        meta.push_str(" · ");
    }
    meta.push_str(&format!("{} tracks · {duration}", detail.tracks.len()));

    // Group by disc when the album spans more than one.
    let discs: std::collections::BTreeSet<i32> =
        detail.tracks.iter().map(|t| t.disc.max(1)).collect();
    let mut rows: Vec<DetailRow> = Vec::new();
    if discs.len() > 1 {
        for disc in discs {
            rows.push(DetailRow {
                is_header: true,
                header: format!("DISC {disc}").into(),
                track: TrackItem::default(),
            });
            rows.extend(
                detail
                    .tracks
                    .iter()
                    .filter(|t| t.disc.max(1) == disc)
                    .map(|t| DetailRow {
                        is_header: false,
                        header: "".into(),
                        track: track_item_with(t, &liked_ids),
                    }),
            );
        }
    } else {
        rows.extend(detail.tracks.iter().map(|t| DetailRow {
            is_header: false,
            header: "".into(),
            track: track_item_with(t, &liked_ids),
        }));
    }
    app.set_detail_album(AlbumItem {
        id: detail.id.into(),
        title: detail.title.into(),
        artist: detail.artist.into(),
        year: detail.year.into(),
        art: image,
        has_art,
    });
    app.set_detail_meta(meta.into());
    app.set_detail_label(detail.label.into());
    app.set_detail_tracks(ModelRc::new(VecModel::from(rows)));
    app.set_show_detail(true);
}

/// Opens the artist page. Built entirely from the library already in `STATE`,
/// so it needs no round-trip — accepts an artist id or a name, since the
/// palette carries ids while some callers only have the display name.
pub fn ui_show_artist_detail(app: &AppWindow, id: &str) {
    let Some((artist, albums, tracks)) = STATE.with(|s| {
        let st = s.borrow();
        let entry = st
            .artists
            .iter()
            .find(|a| a.data.id == id || a.data.name == id)?;
        let name = entry.data.name.clone();
        let albums: Vec<AlbumItem> = st
            .albums
            .iter()
            .filter(|a| a.data.artist == name)
            .map(album_item)
            .collect();
        let ids = liked_ids(&st);
        // Re-indexed from zero: a row's `index` is the position handed back to
        // `play-artist-at`, and that list is this one, not the whole library.
        let tracks: Vec<TrackItem> = st
            .tracks
            .iter()
            .filter(|t| t.artist == name)
            .enumerate()
            .map(|(i, t)| {
                let mut item = track_item_with(t, &ids);
                item.index = i as i32;
                item
            })
            .collect();
        Some((artist_item(entry), albums, tracks))
    }) else {
        return;
    };

    let meta = format!(
        "{} {} · {} {}",
        albums.len(),
        if albums.len() == 1 { "album" } else { "albums" },
        tracks.len(),
        if tracks.len() == 1 { "song" } else { "songs" },
    );
    app.set_artist_detail(artist);
    app.set_artist_detail_meta(meta.into());
    app.set_artist_detail_albums(ModelRc::new(VecModel::from(albums)));
    app.set_artist_detail_tracks(ModelRc::new(VecModel::from(tracks)));
    app.set_show_detail(false);
    app.set_show_artist(true);
}

// ── Remote servers / browsing ───────────────────────────────────────────────

fn server_item(s: &servers::SavedServer) -> ServerItem {
    ServerItem {
        kind: s.kind.clone().into(),
        name: s.name.clone().into(),
        url: s.url.clone().into(),
    }
}

fn refresh_servers_model(app: &AppWindow) {
    let items: Vec<ServerItem> =
        STATE.with(|s| s.borrow().servers.iter().map(server_item).collect());
    app.set_servers(ModelRc::new(VecModel::from(items)));
}

/// Called from the rpc worker when a browse level has been fetched.
pub fn ui_browse_opened(
    app: &AppWindow,
    title: String,
    path: String,
    entries: Vec<rpc::BrowseEntryData>,
    push: bool,
) {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        if push {
            st.browse_stack.push((title, path));
        }
        let crumbs: Vec<&str> = st.browse_stack.iter().map(|(t, _)| t.as_str()).collect();
        app.set_browse_title(crumbs.join("  ›  ").into());
    });
    let items: Vec<BrowseItem> = entries
        .iter()
        .map(|e| BrowseItem {
            name: e.name.clone().into(),
            path: e.path.clone().into(),
            is_dir: e.is_dir,
        })
        .collect();
    app.set_browse_entries(ModelRc::new(VecModel::from(items)));
    app.set_browse_loading(false);
    app.set_browse_error("".into());
    app.set_browsing(true);
}

fn push_audio_to_ui(app: &AppWindow, a: &rpc::AudioSettingsData) {
    app.set_bass(a.bass);
    app.set_bass_min(a.bass_min);
    app.set_bass_max(a.bass_max);
    app.set_treble(a.treble);
    app.set_treble_min(a.treble_min);
    app.set_treble_max(a.treble_max);
    app.set_balance(a.balance);
    app.set_eq_enabled(a.eq_enabled);
    app.set_eq_precut(a.eq_precut);
    app.set_rg_type(a.rg_type);
    app.set_rg_preamp(a.rg_preamp);
    app.set_rg_noclip(a.rg_noclip);
    app.set_crossfade(a.crossfade);
    app.set_fade_in_delay(a.fade_in_delay);
    app.set_fade_in_duration(a.fade_in_duration);
    app.set_fade_out_delay(a.fade_out_delay);
    app.set_fade_out_duration(a.fade_out_duration);
    app.set_fade_out_mixmode(a.fade_out_mixmode);
    app.set_dithering(a.dithering);
    let bands: Vec<EqBand> = a
        .bands
        .iter()
        .map(|&(cutoff, q, gain)| EqBand { cutoff, q, gain })
        .collect();
    app.set_eq_bands(ModelRc::new(VecModel::from(bands)));
}

pub fn ui_set_audio_settings(app: &AppWindow, data: rpc::AudioSettingsData) {
    push_audio_to_ui(app, &data);
    STATE.with(|s| s.borrow_mut().audio = data);
}

// ── Playlists ───────────────────────────────────────────────────────────────

fn playlist_item(p: &rpc::PlaylistData) -> PlaylistItem {
    PlaylistItem {
        id: p.id.clone().into(),
        name: p.name.clone().into(),
        description: p.description.clone().into(),
        count: p.track_count as i32,
    }
}

pub fn ui_set_playlists(app: &AppWindow, data: Vec<rpc::PlaylistData>) {
    let items: Vec<PlaylistItem> = data.iter().map(playlist_item).collect();
    STATE.with(|s| s.borrow_mut().playlists = data);
    app.set_playlists(ModelRc::new(VecModel::from(items)));
    // Refresh an open detail header (name/description/count may have changed).
    let id = app.get_pl_detail().id.to_string();
    if !id.is_empty() {
        if let Some(p) = STATE.with(|s| s.borrow().playlists.iter().find(|p| p.id == id).cloned()) {
            app.set_pl_detail(playlist_item(&p));
        }
    }
}

/// Called from the rpc worker with a playlist's track ids.
pub fn ui_show_playlist(app: &AppWindow, id: String, track_ids: Vec<String>, open_picker: bool) {
    let tracks: Vec<TrackItem> = STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.pl_detail_id = Some(id.clone());
        st.pl_detail_track_ids = track_ids.clone();
        let ids = liked_ids(&st);
        track_ids
            .iter()
            .filter_map(|tid| st.tracks.iter().find(|t| &t.id == tid))
            .map(|t| track_item_with(t, &ids))
            .collect()
    });
    if let Some(p) = STATE.with(|s| s.borrow().playlists.iter().find(|p| p.id == id).cloned()) {
        app.set_pl_detail(playlist_item(&p));
    }
    app.set_pl_detail_tracks(ModelRc::new(VecModel::from(tracks)));
    app.set_pl_detail_open(true);
    app.set_current_tab(5);
    if open_picker {
        app.invoke_open_track_picker();
    } else if app.get_show_track_picker() {
        // Keep the picker results in sync after an add.
        let query = STATE.with(|s| s.borrow().picker_query.clone());
        app.set_picker_results(ModelRc::new(VecModel::from(picker_results(&query))));
    }
}

/// Track-only palette results for the add-to-playlist picker; tracks already
/// in the playlist are hidden.
fn picker_results(query: &str) -> Vec<PaletteItem> {
    let q = query.trim().to_lowercase();
    STATE.with(|s| {
        let st = s.borrow();
        st.tracks
            .iter()
            .filter(|t| !st.pl_detail_track_ids.contains(&t.id))
            .filter(|t| {
                q.is_empty()
                    || [&t.title, &t.artist, &t.album]
                        .iter()
                        .any(|f| f.to_lowercase().contains(&q))
            })
            .take(12)
            .map(|t| {
                let art = st
                    .albums
                    .iter()
                    .find(|a| a.data.title == t.album && a.data.artist == t.artist)
                    .and_then(|a| a.image.clone());
                PaletteItem {
                    kind: "track".into(),
                    id: t.id.clone().into(),
                    title: t.title.clone().into(),
                    subtitle: format!("{} · {}", t.artist, t.album).into(),
                    index: t.index,
                    has_art: art.is_some(),
                    art: art.unwrap_or_default(),
                }
            })
            .collect()
    })
}

/// Called after a like/unlike round-trip: refresh liked list + hearts.
///
/// `settled` names the click this response answers, if any, so its optimistic
/// entry can be retired.
pub fn ui_set_liked(app: &AppWindow, liked: Vec<rpc::TrackData>, settled: Option<(String, u64)>) {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.liked = liked;
        if let Some((id, seq)) = settled {
            settle_pending(&mut st.pending_likes, &id, seq);
        }
    });
    STATE.with(|s| {
        let st = s.borrow();
        let ids = liked_ids(&st);
        // The Liked tab lists the liked tracks themselves, so its *membership*
        // changed — rebuild it. Every other view keeps its rows and only needs
        // the flag re-stamped.
        let liked_items: Vec<TrackItem> =
            st.liked.iter().map(|t| track_item_with(t, &ids)).collect();
        app.set_liked(ModelRc::new(VecModel::from(liked_items)));
        refresh_liked_flags(app, &ids);
    });
}

/// Re-stamps `TrackItem::liked` on every track row currently on screen.
///
/// The library tabs could be rebuilt from `STATE`, but the album-detail,
/// playlist-detail and queue lists are built from data that only ever existed
/// in the message that produced them — `STATE` never keeps it. Rebuilding them
/// is therefore not an option, and the previous version simply skipped them, so
/// clicking a heart anywhere but the Tracks/Liked tabs sent the request and
/// left the icon unchanged: the like looked broken even though the daemon had
/// recorded it.
///
/// Patching the live models in place fixes every view at once, and — unlike
/// swapping in a fresh model — leaves the list's scroll position alone.
fn refresh_liked_flags(app: &AppWindow, ids: &std::collections::HashSet<String>) {
    patch_track_model(&app.get_tracks(), ids);
    patch_track_model(&app.get_liked(), ids);
    patch_track_model(&app.get_pl_detail_tracks(), ids);
    patch_track_model(&app.get_artist_detail_tracks(), ids);
    patch_track_model(&app.get_queue_upnext(), ids);
    patch_track_model(&app.get_queue_history(), ids);

    // Album detail wraps each track in a DetailRow so it can interleave
    // "DISC n" headers.
    if let Some(vm) = app
        .get_detail_tracks()
        .as_any()
        .downcast_ref::<VecModel<DetailRow>>()
    {
        for i in 0..vm.row_count() {
            let Some(mut row) = vm.row_data(i) else {
                continue;
            };
            if row.is_header {
                continue;
            }
            let liked = ids.contains(row.track.id.as_str());
            if row.track.liked != liked {
                row.track.liked = liked;
                vm.set_row_data(i, row);
            }
        }
    }

    app.set_now_liked(ids.contains(app.get_now_track_id().as_str()));
}

/// Set `liked` on each row of a plain track model. A no-op for the empty
/// literal `[]` the properties start out as — that is not a `VecModel`.
fn patch_track_model(model: &ModelRc<TrackItem>, ids: &std::collections::HashSet<String>) {
    let Some(vm) = model.as_any().downcast_ref::<VecModel<TrackItem>>() else {
        return;
    };
    for i in 0..vm.row_count() {
        let Some(mut item) = vm.row_data(i) else {
            continue;
        };
        let liked = ids.contains(item.id.as_str());
        if item.liked != liked {
            item.liked = liked;
            vm.set_row_data(i, item);
        }
    }
}

/// Resolves a track id to its file path (for queue inserts).
fn track_path(id: &str) -> Option<String> {
    STATE.with(|s| {
        let st = s.borrow();
        st.tracks
            .iter()
            .chain(st.liked.iter())
            .find(|t| t.id == id)
            .map(|t| t.path.clone())
    })
}

pub fn is_liked(id: &str) -> bool {
    STATE.with(|s| {
        let st = s.borrow();
        // An unconfirmed click wins, so clicking twice in a row toggles back
        // instead of re-sending the same request.
        match st.pending_likes.get(id) {
            Some(pending) => pending.liked,
            None => st.liked.iter().any(|t| t.id == id),
        }
    })
}

/// Flip a heart before the daemon answers, then let `ui_set_liked` reconcile.
fn set_like_optimistically(app: &AppWindow, id: &str, like: bool) -> u64 {
    let (seq, ids) = STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.like_seq += 1;
        let seq = st.like_seq;
        st.pending_likes
            .insert(id.to_string(), PendingLike { seq, liked: like });
        (seq, liked_ids(&st))
    });
    refresh_liked_flags(app, &ids);
    seq
}

// ── Analytics ───────────────────────────────────────────────────────────────

/// "3d ago" / "2h ago" for the recently-played list.
fn ago(unix: i64) -> String {
    let secs = (chrono_now() - unix).max(0);
    match secs {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn stat_item(r: &rpc::StatRowData, value: String) -> StatItem {
    StatItem {
        track_id: r.track_id.clone().into(),
        title: r.title.clone().into(),
        artist: r.artist.clone().into(),
        album: r.album.clone().into(),
        value: value.into(),
    }
}

pub fn ui_set_most_played(app: &AppWindow, rows: Vec<rpc::StatRowData>) {
    let items: Vec<StatItem> = rows
        .iter()
        .map(|r| stat_item(r, format!("{} plays", r.count)))
        .collect();
    app.set_most_played(ModelRc::new(VecModel::from(items)));
}

pub fn ui_set_stats(app: &AppWindow, data: rpc::StatsData) {
    app.set_stats_total_tracks(data.total_tracks.to_string().into());
    app.set_stats_total_plays(data.total_plays.to_string().into());
    app.set_stats_total_skips(data.total_skips.to_string().into());
    app.set_stats_never_count(data.never_played_count.max(0).to_string().into());

    let skipped: Vec<StatItem> = data
        .most_skipped
        .iter()
        .map(|r| stat_item(r, format!("{} skips", r.count)))
        .collect();
    let recent: Vec<StatItem> = data
        .recently_played
        .iter()
        .map(|r| stat_item(r, r.at.map(ago).unwrap_or_default()))
        .collect();
    let never: Vec<StatItem> = data
        .never_played
        .iter()
        .map(|r| stat_item(r, String::new()))
        .collect();
    app.set_stats_most_skipped(ModelRc::new(VecModel::from(skipped)));
    app.set_stats_recent(ModelRc::new(VecModel::from(recent)));
    app.set_stats_never(ModelRc::new(VecModel::from(never)));
}

// ── Server switcher ─────────────────────────────────────────────────────────

pub fn ui_set_discovered(app: &AppWindow, found: Vec<(String, String, u16)>) {
    STATE.with(|s| s.borrow_mut().discovered = found);
    let q = STATE.with(|s| s.borrow().switcher_query.clone());
    app.set_switcher_results(ModelRc::new(VecModel::from(switcher_results(&q))));
}

fn server_row(
    kind: &str,
    title: String,
    mut subtitle: String,
    id: String,
    active: &str,
) -> PaletteItem {
    if subtitle == active || id == active {
        subtitle = format!("{subtitle} · connected");
    }
    PaletteItem {
        kind: kind.into(),
        id: id.into(),
        title: title.into(),
        subtitle: subtitle.into(),
        index: -1,
        has_art: false,
        art: slint::Image::default(),
    }
}

fn switcher_results(query: &str) -> Vec<PaletteItem> {
    let q = query.trim().to_lowercase();
    let hit =
        |fields: &[&str]| q.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&q));
    STATE.with(|s| {
        let st = s.borrow();
        let active = st.active_server.clone();
        let mut out: Vec<PaletteItem> = Vec::new();

        if hit(&["this mac", "localhost", "127.0.0.1:6061", "embedded"]) {
            out.push(server_row(
                "server",
                "This Mac (embedded)".into(),
                "127.0.0.1:6061".into(),
                "127.0.0.1:6061".into(),
                &active,
            ));
        }
        for (name, host, port) in &st.discovered {
            if host == "127.0.0.1" {
                continue;
            }
            let addr = format!("{host}:{port}");
            if hit(&[name, &addr]) {
                out.push(server_row(
                    "server",
                    name.clone(),
                    addr.clone(),
                    addr,
                    &active,
                ));
            }
        }
        for (i, srv) in st.servers.iter().enumerate() {
            if hit(&[&srv.name, &srv.url]) {
                out.push(server_row(
                    if srv.kind == "jellyfin" {
                        "jellyfin"
                    } else {
                        "subsonic"
                    },
                    srv.name.clone(),
                    srv.url.clone(),
                    format!("srv:{i}"),
                    &active,
                ));
            }
        }
        // Free-form "connect to host[:port]" when the query looks like one.
        let raw = query.trim();
        if (raw.contains('.') || raw.contains(':')) && !out.iter().any(|r| r.id.as_str() == raw) {
            out.push(server_row(
                "server",
                format!("Connect to {raw}"),
                "remote rockboxd".into(),
                raw.to_string(),
                &active,
            ));
        }
        out
    })
}

fn palette_results(query: &str) -> Vec<PaletteItem> {
    let q = query.trim().to_lowercase();
    let hit =
        |fields: &[&str]| q.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&q));
    STATE.with(|s| {
        let st = s.borrow();
        let mut out: Vec<PaletteItem> = Vec::new();
        out.extend(
            st.tracks
                .iter()
                .filter(|t| hit(&[&t.title, &t.artist, &t.album]))
                .take(8)
                .map(|t| {
                    // Reuse the album grid's thumbnail for the track's album.
                    let art = st
                        .albums
                        .iter()
                        .find(|a| a.data.title == t.album && a.data.artist == t.artist)
                        .and_then(|a| a.image.clone());
                    PaletteItem {
                        kind: "track".into(),
                        id: t.id.clone().into(),
                        title: t.title.clone().into(),
                        subtitle: format!("{} · {}", t.artist, t.album).into(),
                        index: t.index,
                        has_art: art.is_some(),
                        art: art.unwrap_or_default(),
                    }
                }),
        );
        out.extend(
            st.albums
                .iter()
                .filter(|a| hit(&[&a.data.title, &a.data.artist]))
                .take(5)
                .map(|a| PaletteItem {
                    kind: "album".into(),
                    id: a.data.id.clone().into(),
                    title: a.data.title.clone().into(),
                    subtitle: a.data.artist.clone().into(),
                    index: -1,
                    has_art: a.image.is_some(),
                    art: a.image.clone().unwrap_or_default(),
                }),
        );
        out.extend(
            st.playlists
                .iter()
                .filter(|p| hit(&[&p.name, &p.description]))
                .take(4)
                .map(|p| PaletteItem {
                    kind: "playlist".into(),
                    id: p.id.clone().into(),
                    title: p.name.clone().into(),
                    subtitle: if p.track_count == 1 {
                        "1 track".into()
                    } else {
                        format!("{} tracks", p.track_count).into()
                    },
                    index: -1,
                    has_art: false,
                    art: slint::Image::default(),
                }),
        );
        out.extend(
            st.artists
                .iter()
                .filter(|a| hit(&[&a.data.name]))
                .take(4)
                .map(|a| PaletteItem {
                    kind: "artist".into(),
                    id: a.data.id.clone().into(),
                    title: a.data.name.clone().into(),
                    subtitle: "".into(),
                    index: -1,
                    has_art: a.image.is_some(),
                    art: a.image.clone().unwrap_or_default(),
                }),
        );
        out
    })
}

/// On macOS: hide the titlebar strip but keep the native traffic-light
/// buttons floating over the sidebar (transparent titlebar + full-size
/// content view). Must run before the first window is created.
fn setup_backend() {
    #[cfg(target_os = "macos")]
    {
        let backend = i_slint_backend_winit::Backend::builder()
            .with_window_attributes_hook(|attrs| {
                use winit::platform::macos::WindowAttributesExtMacOS;
                attrs
                    .with_titlebar_transparent(true)
                    .with_fullsize_content_view(true)
                    .with_title_hidden(true)
            })
            .build()
            .expect("winit backend");
        slint::platform::set_platform(Box::new(backend)).expect("set slint platform");
    }
}

/// Sets the Dock icon at runtime from the bundled `assets/AppIcon.icns`
/// (built from `macos/Rockbox/Assets.xcassets`, same artwork as the GPUI
/// app). Only meaningful on macOS; other platforms use the window icon.
#[cfg(target_os = "macos")]
fn set_dock_icon() {
    use objc2::ClassType;
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::{MainThreadMarker, NSData};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let bytes: &[u8] = include_bytes!("../assets/AppIcon.icns");
    let data = NSData::with_bytes(bytes);
    if let Some(img) = NSImage::initWithData(NSImage::alloc(), &data) {
        let app = NSApplication::sharedApplication(mtm);
        unsafe { app.setApplicationIconImage(Some(&img)) };
    }
}

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    setup_backend();
    let app = AppWindow::new()?;
    #[cfg(target_os = "macos")]
    {
        app.set_titlebar_inset(24.0);
        set_dock_icon();
    }

    // ── Skins ───────────────────────────────────────────────────────────────
    let skins = Rc::new(skin::load_all());
    let current = Rc::new(RefCell::new(skin::load_selection(&skins)));
    if let Some(s) = skins.get(*current.borrow()) {
        skin::apply(s, &app);
    }
    {
        let app_weak = app.as_weak();
        let skins = skins.clone();
        let current = current.clone();
        app.on_cycle_skin(move || {
            let app = app_weak.unwrap();
            let mut idx = current.borrow_mut();
            *idx = (*idx + 1) % skins.len().max(1);
            if let Some(s) = skins.get(*idx) {
                skin::apply(s, &app);
                skin::save_selection(&s.name);
            }
        });
    }

    // ── Embedded daemon (if no rockboxd is listening on 6061) ───────────────
    if !daemon::is_running() {
        app.set_status_text("starting rockboxd…".into());
    }
    std::thread::spawn(|| {
        daemon::ensure_running();
    });

    // ── gRPC worker + commands ──────────────────────────────────────────────
    // The worker retries until the (embedded or external) daemon answers.
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<rpc::Cmd>();
    let (audio_tx, audio_rx) = tokio::sync::mpsc::unbounded_channel::<rpc::AudioSettingsData>();
    rpc::start(app.as_weak(), rx, audio_rx);

    {
        let tx = tx.clone();
        app.on_play_pause(move || {
            let _ = tx.send(rpc::Cmd::PlayPause);
        });
    }
    {
        let tx = tx.clone();
        app.on_next(move || {
            let _ = tx.send(rpc::Cmd::Next);
        });
    }
    {
        let tx = tx.clone();
        app.on_previous(move || {
            let _ = tx.send(rpc::Cmd::Previous);
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_seek(move |pct| {
            let app = app_weak.unwrap();
            let length_ms = (app.get_length_s() as f64 * 1000.0) as i32;
            if length_ms > 0 {
                let ms = (length_ms as f32 * pct.clamp(0.0, 1.0)) as i32;
                // Optimistic local update so the bar doesn't snap back while
                // the daemon catches up.
                app.set_elapsed_s(ms as f32 / 1000.0);
                app.set_progress(pct);
                let _ = tx.send(rpc::Cmd::SeekMs(ms));
            }
        });
    }
    {
        let tx = tx.clone();
        app.on_set_volume(move |pct| {
            let _ = tx.send(rpc::Cmd::SetVolume(pct));
        });
    }
    {
        let tx = tx.clone();
        app.on_play_album(move |id| {
            let _ = tx.send(rpc::Cmd::PlayAlbum(id.into()));
        });
    }
    {
        let tx = tx.clone();
        app.on_open_album(move |id| {
            let _ = tx.send(rpc::Cmd::OpenAlbum(id.into()));
        });
    }
    {
        let tx = tx.clone();
        app.on_play_album_track(move |id, pos| {
            let _ = tx.send(rpc::Cmd::PlayAlbumAt(id.into(), pos));
        });
    }
    {
        let tx = tx.clone();
        app.on_play_artist(move |id| {
            let _ = tx.send(rpc::Cmd::PlayArtist(id.into()));
        });
    }
    {
        let tx = tx.clone();
        app.on_play_track_at(move |i| {
            let _ = tx.send(rpc::Cmd::PlayAllAt(i));
        });
    }
    {
        let tx = tx.clone();
        app.on_play_liked_at(move |i| {
            let _ = tx.send(rpc::Cmd::PlayLikedAt(i));
        });
    }
    {
        let tx = tx.clone();
        app.on_play_queue_at(move |i| {
            let _ = tx.send(rpc::Cmd::QueueJump(i));
        });
    }
    {
        let tx = tx.clone();
        app.on_queue_clear(move || {
            let _ = tx.send(rpc::Cmd::QueueClear);
        });
    }
    {
        let tx = tx.clone();
        app.on_queue_remove(move |i| {
            let _ = tx.send(rpc::Cmd::QueueRemove(i));
        });
    }
    {
        let audio_tx = audio_tx.clone();
        let app_weak = app.as_weak();
        app.on_audio_set(move |key, value| {
            let app = app_weak.unwrap();
            let snapshot = STATE.with(|s| {
                let mut st = s.borrow_mut();
                let a = &mut st.audio;
                match key.as_str() {
                    "bass" => a.bass = value,
                    "treble" => a.treble = value,
                    "balance" => a.balance = value,
                    "eq_enabled" => a.eq_enabled = value != 0,
                    "eq_precut" => a.eq_precut = value,
                    "rg_type" => a.rg_type = value,
                    "rg_preamp" => a.rg_preamp = value,
                    "rg_noclip" => a.rg_noclip = value != 0,
                    "crossfade" => a.crossfade = value,
                    "fade_in_delay" => a.fade_in_delay = value,
                    "fade_in_duration" => a.fade_in_duration = value,
                    "fade_out_delay" => a.fade_out_delay = value,
                    "fade_out_duration" => a.fade_out_duration = value,
                    "fade_out_mixmode" => a.fade_out_mixmode = value,
                    "dithering" => a.dithering = value != 0,
                    _ => tracing::warn!("unknown audio setting key: {key}"),
                }
                a.clone()
            });
            push_audio_to_ui(&app, &snapshot);
            let _ = audio_tx.send(snapshot);
        });
    }
    {
        let audio_tx = audio_tx.clone();
        let app_weak = app.as_weak();
        app.on_eq_band_set(move |index, gain| {
            let app = app_weak.unwrap();
            let gain = gain.clamp(-240, 240);
            let snapshot = STATE.with(|s| {
                let mut st = s.borrow_mut();
                if let Some(band) = st.audio.bands.get_mut(index as usize) {
                    band.2 = gain;
                }
                st.audio.clone()
            });
            let model = app.get_eq_bands();
            if let Some(mut row) = model.row_data(index as usize) {
                row.gain = gain;
                model.set_row_data(index as usize, row);
            }
            let _ = audio_tx.send(snapshot);
        });
    }
    {
        let tx = tx.clone();
        app.on_play_album_shuffled(move |id| {
            let _ = tx.send(rpc::Cmd::PlayAlbumShuffled(id.into()));
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_toggle_shuffle(move || {
            let app = app_weak.unwrap();
            let enabled = !app.get_shuffle();
            app.set_shuffle(enabled); // optimistic
            let _ = tx.send(rpc::Cmd::SetShuffle(enabled));
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_cycle_repeat(move || {
            let app = app_weak.unwrap();
            let mode = (app.get_repeat_mode() + 1) % 3;
            app.set_repeat_mode(mode); // optimistic
            let _ = tx.send(rpc::Cmd::SetRepeat(mode));
        });
    }
    // ── Server switcher ─────────────────────────────────────────────────────
    STATE.with(|s| s.borrow_mut().active_server = rpc::endpoints().display);
    {
        let tx = tx.clone();
        app.on_switcher_opened(move || {
            let _ = tx.send(rpc::Cmd::DiscoverServers);
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_switcher_query(move |q| {
            let app = app_weak.unwrap();
            STATE.with(|s| s.borrow_mut().switcher_query = q.to_string());
            app.set_switcher_results(ModelRc::new(VecModel::from(switcher_results(&q))));
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_switcher_activate(move |item| {
            let app = app_weak.unwrap();
            let kind = item.kind.to_string();
            let id = item.id.to_string();
            if kind == "server" {
                let (host, port) = match id.rsplit_once(':') {
                    Some((h, p)) => (h.to_string(), p.parse().unwrap_or(6061)),
                    None => (id.clone(), 6061),
                };
                let display = format!("{host}:{port}");
                STATE.with(|s| s.borrow_mut().active_server = display.clone());
                app.set_status_text(display.into());
                app.set_connected(false);
                let _ = tx.send(rpc::Cmd::SwitchServer {
                    host,
                    grpc_port: port,
                });
            } else if let Some(idx) = id
                .strip_prefix("srv:")
                .and_then(|i| i.parse::<usize>().ok())
            {
                // Saved Subsonic / Jellyfin server → open the browse flow.
                let srv = STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.browse_stack.clear();
                    st.servers.get(idx).cloned()
                });
                if let Some(srv) = srv {
                    app.set_current_tab(4);
                    app.set_browse_error("".into());
                    app.set_browse_loading(true);
                    let _ = tx.send(rpc::Cmd::ConnectServer(srv));
                }
            }
        });
    }

    // ── Palette context menu ────────────────────────────────────────────────
    {
        let tx = tx.clone();
        app.on_palette_menu_play(move |item| {
            let cmd = match item.kind.as_str() {
                "track" => rpc::Cmd::PlayAllAt(item.index),
                "album" => rpc::Cmd::PlayAlbum(item.id.into()),
                _ => rpc::Cmd::PlaySavedPlaylist(item.id.into()),
            };
            let _ = tx.send(cmd);
        });
    }
    {
        let tx = tx.clone();
        app.on_palette_menu_play_next(move |item| {
            let cmd = match item.kind.as_str() {
                "track" => track_path(item.id.as_str()).map(|path| rpc::Cmd::InsertTracks {
                    position: -2,
                    tracks: vec![path],
                }),
                "album" => Some(rpc::Cmd::InsertAlbum {
                    album_id: item.id.into(),
                    position: -2,
                }),
                _ => None,
            };
            if let Some(cmd) = cmd {
                let _ = tx.send(cmd);
            }
        });
    }
    {
        let tx = tx.clone();
        app.on_palette_menu_add_queue(move |item| {
            let cmd = match item.kind.as_str() {
                "track" => track_path(item.id.as_str()).map(|path| rpc::Cmd::InsertTracks {
                    position: -3,
                    tracks: vec![path],
                }),
                "album" => Some(rpc::Cmd::InsertAlbum {
                    album_id: item.id.into(),
                    position: -3,
                }),
                _ => None,
            };
            if let Some(cmd) = cmd {
                let _ = tx.send(cmd);
            }
        });
    }
    {
        let tx = tx.clone();
        app.on_album_add_to_playlist(move |playlist_id, album_id| {
            // The library is already in STATE; an album's tracks are the ones
            // carrying its id.
            let track_ids: Vec<String> = STATE.with(|s| {
                s.borrow()
                    .tracks
                    .iter()
                    .filter(|t| t.album_id == album_id.as_str())
                    .map(|t| t.id.clone())
                    .collect()
            });
            for track_id in track_ids {
                let _ = tx.send(rpc::Cmd::PlaylistAddTrack {
                    playlist_id: playlist_id.to_string(),
                    track_id,
                });
            }
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_plpicker_create_and_add(move |typed| {
            let app = app_weak.unwrap();
            let name = typed.trim().to_string();
            let name = if name.is_empty() {
                "New Playlist".to_string()
            } else {
                name
            };
            let pending_id: String = app.get_plpicker_track_id().into();
            let track_ids: Vec<String> = if app.get_plpicker_kind() == "album" {
                STATE.with(|s| {
                    s.borrow()
                        .tracks
                        .iter()
                        .filter(|t| t.album_id == pending_id)
                        .map(|t| t.id.clone())
                        .collect()
                })
            } else {
                vec![pending_id]
            };
            let _ = tx.send(rpc::Cmd::PlaylistCreateAndAdd { name, track_ids });
        });
    }

    // ── Analytics tabs ──────────────────────────────────────────────────────
    {
        let tx = tx.clone();
        app.on_load_most_played(move || {
            let _ = tx.send(rpc::Cmd::LoadMostPlayed);
        });
    }
    {
        let tx = tx.clone();
        app.on_load_stats(move || {
            let _ = tx.send(rpc::Cmd::LoadStats);
        });
    }
    {
        let tx = tx.clone();
        app.on_play_stat_track(move |id| {
            // The stat lists carry library track ids; play by the track's
            // position in the full library, which STATE already holds.
            let index = STATE.with(|s| {
                s.borrow()
                    .tracks
                    .iter()
                    .find(|t| t.id == id.as_str())
                    .map(|t| t.index)
            });
            if let Some(index) = index {
                let _ = tx.send(rpc::Cmd::PlayAllAt(index));
            }
        });
    }

    // ── Track / album context actions ───────────────────────────────────────
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_track_like(move |id| {
            let id: String = id.into();
            let like = !is_liked(&id);
            // Flip the icon now; the request goes out in the background and
            // `ui_set_liked` settles it (or rolls it back) when it answers.
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let seq = set_like_optimistically(&app, &id, like);
            let _ = tx.send(rpc::Cmd::LikeTrack { id, like, seq });
        });
    }
    {
        let tx = tx.clone();
        app.on_track_insert(move |id, position| {
            if let Some(path) = track_path(&id) {
                let _ = tx.send(rpc::Cmd::InsertTracks {
                    position,
                    tracks: vec![path],
                });
            }
        });
    }
    {
        let tx = tx.clone();
        app.on_album_insert(move |id, position| {
            let _ = tx.send(rpc::Cmd::InsertAlbum {
                album_id: id.into(),
                position,
            });
        });
    }
    {
        let tx = tx.clone();
        app.on_album_like(move |id| {
            let _ = tx.send(rpc::Cmd::LikeAlbum(id.into()));
        });
    }
    {
        let tx = tx.clone();
        app.on_track_add_to_playlist(move |playlist_id, track_id| {
            let _ = tx.send(rpc::Cmd::PlaylistAddTrack {
                playlist_id: playlist_id.into(),
                track_id: track_id.into(),
            });
        });
    }

    // ── Playlists ───────────────────────────────────────────────────────────
    {
        let tx = tx.clone();
        app.on_playlists_open(move |id| {
            let _ = tx.send(rpc::Cmd::OpenPlaylist(id.into()));
        });
    }
    {
        let tx = tx.clone();
        app.on_playlist_play(move |id| {
            let _ = tx.send(rpc::Cmd::PlaySavedPlaylist(id.into()));
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_playlist_delete(move |id| {
            let app = app_weak.unwrap();
            let id: String = id.into();
            if app.get_pl_detail().id.as_str() == id {
                app.set_pl_detail_open(false);
            }
            let _ = tx.send(rpc::Cmd::PlaylistDelete(id));
        });
    }
    {
        let tx = tx.clone();
        app.on_playlist_form_submit(move |id, name, desc| {
            let cmd = if id.is_empty() {
                rpc::Cmd::PlaylistCreate {
                    name: name.into(),
                    description: desc.into(),
                }
            } else {
                rpc::Cmd::PlaylistUpdate {
                    id: id.into(),
                    name: name.into(),
                    description: desc.into(),
                }
            };
            let _ = tx.send(cmd);
        });
    }
    {
        let tx = tx.clone();
        app.on_playlist_remove_track(move |track_id| {
            let playlist_id = STATE.with(|s| s.borrow().pl_detail_id.clone());
            if let Some(playlist_id) = playlist_id {
                let _ = tx.send(rpc::Cmd::PlaylistRemoveTrack {
                    playlist_id,
                    track_id: track_id.into(),
                });
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_plpicker_query(move |q| {
            let app = app_weak.unwrap();
            let q = q.to_lowercase();
            // "Create new playlist" rides on top of every result set, so the
            // escape hatch is always one arrow-up away — and typing a name
            // that matches nothing still leaves something to activate.
            let create_row = PaletteItem {
                kind: "create".into(),
                id: "create".into(),
                title: if q.trim().is_empty() {
                    "Create new playlist".into()
                } else {
                    format!("Create \"{}\"", q.trim()).into()
                },
                subtitle: "New playlist with this track".into(),
                index: -1,
                has_art: false,
                art: slint::Image::default(),
            };
            let items: Vec<PaletteItem> = STATE.with(|s| {
                std::iter::once(create_row)
                    .chain(s.borrow()
                    .playlists
                    .iter()
                    .filter(|p| {
                        q.is_empty()
                            || p.name.to_lowercase().contains(&q)
                            || p.description.to_lowercase().contains(&q)
                    })
                    .map(|p| PaletteItem {
                        kind: "playlist".into(),
                        id: p.id.clone().into(),
                        title: p.name.clone().into(),
                        subtitle: if p.track_count == 1 {
                            "1 track".into()
                        } else {
                            format!("{} tracks", p.track_count).into()
                        },
                        index: -1,
                        has_art: false,
                        art: slint::Image::default(),
                    }))
                    .collect()
            });
            app.set_plpicker_results(ModelRc::new(VecModel::from(items)));
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_picker_query(move |q| {
            let app = app_weak.unwrap();
            STATE.with(|s| s.borrow_mut().picker_query = q.to_string());
            app.set_picker_results(ModelRc::new(VecModel::from(picker_results(&q))));
        });
    }
    {
        let tx = tx.clone();
        app.on_picker_add(move |track_id| {
            let track_id: String = track_id.into();
            let playlist_id = STATE.with(|s| {
                let mut st = s.borrow_mut();
                // Optimistic: hide it from the picker immediately.
                st.pl_detail_track_ids.push(track_id.clone());
                st.pl_detail_id.clone()
            });
            if let Some(playlist_id) = playlist_id {
                let _ = tx.send(rpc::Cmd::PlaylistAddTrack {
                    playlist_id,
                    track_id,
                });
            }
        });
    }

    // ── Remote servers ──────────────────────────────────────────────────────
    STATE.with(|s| s.borrow_mut().servers = servers::load());
    refresh_servers_model(&app);
    {
        let app_weak = app.as_weak();
        app.on_server_add(move |kind, name, url, user, pass| {
            let app = app_weak.unwrap();
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.servers.push(servers::SavedServer {
                    kind: kind.into(),
                    name: name.into(),
                    url: url.into(),
                    username: user.into(),
                    password: pass.into(),
                });
                servers::save(&st.servers);
            });
            refresh_servers_model(&app);
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_server_delete(move |idx| {
            let app = app_weak.unwrap();
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                if (idx as usize) < st.servers.len() {
                    st.servers.remove(idx as usize);
                    servers::save(&st.servers);
                }
            });
            refresh_servers_model(&app);
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_server_connect(move |idx| {
            let app = app_weak.unwrap();
            let srv = STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.browse_stack.clear();
                st.servers.get(idx as usize).cloned()
            });
            if let Some(srv) = srv {
                app.set_browse_loading(true);
                let _ = tx.send(rpc::Cmd::ConnectServer(srv));
            }
        });
    }
    {
        let tx = tx.clone();
        app.on_browse_open(move |path, title| {
            let _ = tx.send(rpc::Cmd::Browse {
                title: title.into(),
                path: path.into(),
                push: true,
            });
        });
    }
    {
        let tx = tx.clone();
        app.on_browse_play_dir(move |path| {
            let _ = tx.send(rpc::Cmd::PlayDir(path.into()));
        });
    }
    {
        let tx = tx.clone();
        app.on_browse_play_at(move |idx| {
            let dir = STATE.with(|s| s.borrow().browse_stack.last().map(|(_, p)| p.clone()));
            if let Some(dir) = dir {
                let _ = tx.send(rpc::Cmd::PlayDirAt(dir, idx));
            }
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_browse_back(move || {
            let app = app_weak.unwrap();
            let target = STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.browse_stack.pop();
                st.browse_stack.last().cloned()
            });
            match target {
                Some((title, path)) => {
                    app.set_browse_loading(true);
                    let _ = tx.send(rpc::Cmd::Browse {
                        title,
                        path,
                        push: false,
                    });
                }
                None => {
                    app.set_browsing(false);
                    app.set_browse_entries(ModelRc::new(VecModel::from(Vec::<BrowseItem>::new())));
                }
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_palette_query(move |text| {
            let app = app_weak.unwrap();
            app.set_palette_results(ModelRc::new(VecModel::from(palette_results(&text))));
        });
    }
    {
        let tx = tx.clone();
        let app_weak = app.as_weak();
        app.on_palette_activate(move |item| {
            // Albums and artists open their page rather than playing: opening
            // one is what a search result is for, and the play button is right
            // there once you land. Tracks and playlists still play — there is
            // no page for them to open.
            match item.kind.as_str() {
                "album" => {
                    let _ = tx.send(rpc::Cmd::OpenAlbum(item.id.into()));
                }
                "artist" => {
                    if let Some(app) = app_weak.upgrade() {
                        ui_show_artist_detail(&app, item.id.as_str());
                    }
                }
                "playlist" => {
                    let _ = tx.send(rpc::Cmd::PlaySavedPlaylist(item.id.into()));
                }
                _ => {
                    let _ = tx.send(rpc::Cmd::PlayAllAt(item.index));
                }
            }
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_open_artist(move |name| {
            let app = app_weak.unwrap();
            ui_show_artist_detail(&app, name.as_str());
        });
    }
    {
        let tx = tx.clone();
        app.on_play_artist_at(move |id, index| {
            let _ = tx.send(rpc::Cmd::PlayArtistAt(id.into(), index));
        });
    }
    {
        let tx = tx.clone();
        app.on_play_artist_shuffled(move |id| {
            let _ = tx.send(rpc::Cmd::PlayArtistShuffled(id.into()));
        });
    }

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn track(id: &str) -> rpc::TrackData {
        rpc::TrackData {
            id: id.into(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            length_ms: 0,
            index: 0,
            disc: 0,
            track_no: 0,
            path: String::new(),
            album_id: String::new(),
        }
    }

    fn state(liked: &[&str], pending: &[(&str, u64, bool)]) -> UiState {
        UiState {
            liked: liked.iter().map(|id| track(id)).collect(),
            pending_likes: pending
                .iter()
                .map(|(id, seq, liked)| {
                    (
                        id.to_string(),
                        PendingLike {
                            seq: *seq,
                            liked: *liked,
                        },
                    )
                })
                .collect(),
            ..UiState::default()
        }
    }

    #[test]
    fn unsettled_click_wins_over_the_server_list() {
        // Liking fills the heart before the daemon answers…
        assert!(liked_ids(&state(&[], &[("a", 1, true)])).contains("a"));
        // …and unliking empties it even though the server still lists it.
        assert!(!liked_ids(&state(&["b"], &[("b", 1, false)])).contains("b"));
    }

    #[test]
    fn server_list_stands_where_nothing_is_pending() {
        let ids = liked_ids(&state(&["a", "b"], &[("c", 1, true)]));
        assert_eq!(ids.len(), 3);
        for id in ["a", "b", "c"] {
            assert!(ids.contains(id), "{id} missing");
        }
    }

    #[test]
    fn a_settled_like_leaves_the_heart_filled() {
        let mut st = state(&[], &[("a", 1, true)]);
        // Response arrives and the daemon agrees `a` is liked.
        st.liked = vec![track("a")];
        settle_pending(&mut st.pending_likes, "a", 1);

        assert!(st.pending_likes.is_empty());
        assert!(liked_ids(&st).contains("a"));
    }

    #[test]
    fn a_failed_like_rolls_back() {
        let mut st = state(&[], &[("a", 1, true)]);
        // The request failed, so the refetched list still lacks `a`. Settling
        // on failure too is what lets the heart fall back to the truth —
        // leaving the entry would strand it filled forever.
        settle_pending(&mut st.pending_likes, "a", 1);

        assert!(st.pending_likes.is_empty());
        assert!(!liked_ids(&st).contains("a"));
    }

    #[test]
    fn a_stale_response_cannot_undo_a_newer_click() {
        // Like (seq 1), then unlike (seq 2) before the first answers.
        let mut st = state(&[], &[("a", 2, false)]);
        // Now seq 1's response lands, carrying a server list from before the
        // unlike. It must not retire seq 2's entry.
        settle_pending(&mut st.pending_likes, "a", 1);
        st.liked = vec![track("a")];

        assert_eq!(st.pending_likes.len(), 1);
        assert!(
            !liked_ids(&st).contains("a"),
            "stale response undid the unlike"
        );

        // seq 2's own response then settles it.
        settle_pending(&mut st.pending_likes, "a", 2);
        assert!(st.pending_likes.is_empty());
    }

    #[test]
    fn settling_an_unknown_id_is_a_no_op() {
        let mut pending: HashMap<String, PendingLike> = HashMap::new();
        settle_pending(&mut pending, "ghost", 7);
        assert!(pending.is_empty());
    }
}
