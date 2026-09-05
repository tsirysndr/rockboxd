//! rockbox-desktop — skinnable Slint client for rockboxd.
//!
//! Talks to a running rockboxd over gRPC (127.0.0.1:6061 by default; override
//! with ROCKBOX_HOST / ROCKBOX_GRPC_PORT / ROCKBOX_GRAPHQL_PORT). All library
//! state lives on the UI thread in a thread_local; the tokio worker in rpc.rs
//! pushes plain data across via upgrade_in_event_loop.

mod daemon;
mod rpc;
mod skin;

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedPixelBuffer, VecModel};

slint::include_modules!();

struct AlbumEntry {
    data: rpc::AlbumData,
    image: Option<slint::Image>,
}

#[derive(Default)]
struct UiState {
    albums: Vec<AlbumEntry>,
    artists: Vec<rpc::ArtistData>,
    tracks: Vec<rpc::TrackData>,
    liked: Vec<rpc::TrackData>,
    audio: rpc::AudioSettingsData,
}

thread_local! {
    static STATE: RefCell<UiState> = RefCell::new(UiState::default());
}

fn track_item(t: &rpc::TrackData) -> TrackItem {
    TrackItem {
        id: t.id.clone().into(),
        title: t.title.clone().into(),
        artist: t.artist.clone().into(),
        album: t.album.clone().into(),
        duration: rpc::format_time(t.length_ms as f64 / 1000.0).into(),
        index: t.index,
    }
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
        st.artists = data.artists;
        st.tracks = data.tracks;
        st.liked = data.liked;

        let albums: Vec<AlbumItem> = st.albums.iter().map(album_item).collect();
        let artists: Vec<ArtistItem> = st
            .artists
            .iter()
            .map(|a| ArtistItem {
                id: a.id.clone().into(),
                name: a.name.clone().into(),
            })
            .collect();
        let tracks: Vec<TrackItem> = st.tracks.iter().map(track_item).collect();
        let liked: Vec<TrackItem> = st.liked.iter().map(track_item).collect();

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

pub fn ui_set_now_art(app: &AppWindow, w: u32, h: u32, rgba: Vec<u8>) {
    let image = slint::Image::from_rgba8(SharedPixelBuffer::clone_from_slice(&rgba, w, h));
    app.set_now_art(image);
    app.set_now_has_art(true);
}

pub fn ui_set_queue(
    app: &AppWindow,
    total: usize,
    upnext: Vec<rpc::TrackData>,
    history: Vec<rpc::TrackData>,
) {
    let upnext: Vec<TrackItem> = upnext.iter().map(track_item).collect();
    let history: Vec<TrackItem> = history.iter().map(track_item).collect();
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

    let tracks: Vec<TrackItem> = detail.tracks.iter().map(track_item).collect();
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
    app.set_detail_tracks(ModelRc::new(VecModel::from(tracks)));
    app.set_show_detail(true);
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
                .map(|t| PaletteItem {
                    kind: "track".into(),
                    id: t.id.clone().into(),
                    title: t.title.clone().into(),
                    subtitle: format!("{} · {}", t.artist, t.album).into(),
                    index: t.index,
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
                }),
        );
        out.extend(
            st.artists
                .iter()
                .filter(|a| hit(&[&a.name]))
                .take(4)
                .map(|a| PaletteItem {
                    kind: "artist".into(),
                    id: a.id.clone().into(),
                    title: a.name.clone().into(),
                    subtitle: "".into(),
                    index: -1,
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

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    setup_backend();
    let app = AppWindow::new()?;
    #[cfg(target_os = "macos")]
    app.set_titlebar_inset(24.0);

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
    {
        let app_weak = app.as_weak();
        app.on_palette_query(move |text| {
            let app = app_weak.unwrap();
            app.set_palette_results(ModelRc::new(VecModel::from(palette_results(&text))));
        });
    }
    {
        let tx = tx.clone();
        app.on_palette_activate(move |item| {
            let cmd = match item.kind.as_str() {
                "track" => rpc::Cmd::PlayAllAt(item.index),
                "album" => rpc::Cmd::PlayAlbum(item.id.into()),
                _ => rpc::Cmd::PlayArtist(item.id.into()),
            };
            let _ = tx.send(cmd);
        });
    }
    {
        let app_weak = app.as_weak();
        app.on_open_artist(move |name| {
            let app = app_weak.unwrap();
            app.invoke_open_palette_with(name);
        });
    }

    app.run()
}
