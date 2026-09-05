//! gRPC worker: talks to rockboxd, pushes state into the Slint UI.

use std::sync::Arc;
use std::time::Duration;

use slint::Weak;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};

use crate::AppWindow;

pub mod api {
    tonic::include_proto!("rockbox.v1alpha1");
}

use api::library_service_client::LibraryServiceClient;
use api::playback_service_client::PlaybackServiceClient;
use api::playlist_service_client::PlaylistServiceClient;
use api::settings_service_client::SettingsServiceClient;
use api::sound_service_client::SoundServiceClient;
use api::*;

const SOUND_VOLUME: i32 = 0;

// ── Commands from UI callbacks ──────────────────────────────────────────────

#[derive(Debug)]
pub enum Cmd {
    PlayPause,
    Next,
    Previous,
    SeekMs(i32),
    SetVolume(f32),
    PlayAlbum(String),
    PlayAlbumAt(String, i32),
    PlayArtist(String),
    PlayAllAt(i32),
    PlayLikedAt(i32),
    QueueJump(i32),
    OpenAlbum(String),
    PlayAlbumShuffled(String),
    SetShuffle(bool),
    SetRepeat(i32),
}

// ── Plain data handed to the UI thread ──────────────────────────────────────

#[derive(Clone, Debug)]
pub struct AlbumData {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: String,
    pub art_file: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ArtistData {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct TrackData {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub length_ms: u64,
    pub index: i32,
}

#[derive(Clone, Debug, Default)]
pub struct LibraryData {
    pub albums: Vec<AlbumData>,
    pub artists: Vec<ArtistData>,
    pub tracks: Vec<TrackData>,
    pub liked: Vec<TrackData>,
}

#[derive(Clone, Debug)]
pub struct AlbumDetailData {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: String,
    pub label: String,
    pub tracks: Vec<TrackData>,
}

#[derive(Clone, Debug, Default)]
pub struct AudioSettingsData {
    pub bass: i32,
    pub bass_min: i32,
    pub bass_max: i32,
    pub treble: i32,
    pub treble_min: i32,
    pub treble_max: i32,
    pub balance: i32,
    pub eq_enabled: bool,
    pub eq_precut: i32,
    pub bands: Vec<(i32, i32, i32)>, // (cutoff Hz, q x10, gain dB x10)
    pub rg_type: i32,
    pub rg_preamp: i32,
    pub rg_noclip: bool,
    pub crossfade: i32,
    pub fade_in_delay: i32,
    pub fade_in_duration: i32,
    pub fade_out_delay: i32,
    pub fade_out_duration: i32,
    pub fade_out_mixmode: i32,
    pub dithering: bool,
}

#[derive(Clone, Debug)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub path: String,
    pub elapsed_ms: u64,
    pub length_ms: u64,
    pub bitrate: u32,
    pub frequency: u64,
    pub art_file: Option<String>,
}

pub struct Endpoints {
    pub grpc: String,
    pub covers: String,
    pub display: String,
}

pub fn endpoints() -> Endpoints {
    let host = std::env::var("ROCKBOX_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let grpc_port = std::env::var("ROCKBOX_GRPC_PORT").unwrap_or_else(|_| "6061".into());
    let http_port = std::env::var("ROCKBOX_GRAPHQL_PORT").unwrap_or_else(|_| "6062".into());
    Endpoints {
        grpc: format!("http://{host}:{grpc_port}"),
        covers: format!("http://{host}:{http_port}/covers/"),
        display: format!("{host}:{grpc_port}"),
    }
}

pub fn format_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// Spawns the background runtime; returns immediately.
pub fn start(
    weak: Weak<AppWindow>,
    rx: UnboundedReceiver<Cmd>,
    audio_rx: UnboundedReceiver<AudioSettingsData>,
) {
    std::thread::Builder::new()
        .name("rockbox-rpc".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(run(weak, rx, audio_rx));
        })
        .expect("spawn rpc thread");
}

async fn run(
    weak: Weak<AppWindow>,
    rx: UnboundedReceiver<Cmd>,
    audio_rx: UnboundedReceiver<AudioSettingsData>,
) {
    let ep = endpoints();
    let channel = Endpoint::from_shared(ep.grpc.clone())
        .expect("grpc url")
        .connect_timeout(Duration::from_secs(3))
        .connect_lazy();

    // Volume range discovered at connect time, shared with the command loop.
    let vol_range = Arc::new(Mutex::new((-80i32, 0i32)));

    tokio::spawn(cmd_loop(
        channel.clone(),
        rx,
        vol_range.clone(),
        weak.clone(),
        endpoints(),
    ));
    tokio::spawn(audio_save_loop(channel.clone(), audio_rx));
    tokio::spawn(status_loop(channel.clone(), weak.clone()));
    tokio::spawn(queue_loop(channel.clone(), weak.clone()));
    tokio::spawn(ticker(weak.clone()));

    // Main loop: connect → init volume → load library → follow current track.
    loop {
        match session(&channel, &ep, &weak, &vol_range).await {
            Ok(()) => {}
            Err(e) => tracing::debug!("session ended: {e}"),
        }
        let display = ep.display.clone();
        let _ = weak.upgrade_in_event_loop(move |app| {
            app.set_connected(false);
            app.set_status_text(format!("{display} (retrying…)").into());
        });
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn session(
    channel: &Channel,
    ep: &Endpoints,
    weak: &Weak<AppWindow>,
    vol_range: &Arc<Mutex<(i32, i32)>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut playback = PlaybackServiceClient::new(channel.clone());

    // Probe with a unary call so we only report connected when it works.
    playback.status(StatusRequest {}).await?;
    let display = ep.display.clone();
    let _ = weak.upgrade_in_event_loop(move |app| {
        app.set_connected(true);
        app.set_status_text(display.into());
    });

    init_volume(channel, weak, vol_range).await;
    init_settings(channel, weak).await;
    load_library(channel, ep, weak).await?;

    // Follow the current track stream until it drops.
    let mut stream = playback
        .stream_current_track(StreamCurrentTrackRequest {})
        .await?
        .into_inner();
    let mut last_art: Option<String> = None;
    while let Some(msg) = stream.message().await? {
        let np = NowPlaying {
            title: if msg.title.is_empty() {
                filename_stem(&msg.path)
            } else {
                msg.title.clone()
            },
            artist: msg.artist.clone(),
            path: msg.path.clone(),
            elapsed_ms: msg.elapsed,
            length_ms: msg.length,
            bitrate: msg.bitrate,
            frequency: msg.frequency,
            art_file: msg.album_art.clone(),
        };
        if np.art_file != last_art {
            last_art = np.art_file.clone();
            match &np.art_file {
                Some(file) => {
                    tokio::spawn(fetch_now_art(ep.covers.clone(), file.clone(), weak.clone()));
                }
                None => {
                    let _ = weak.upgrade_in_event_loop(|app| app.set_now_has_art(false));
                }
            }
        }
        push_now_playing(weak, np);
    }
    Ok(())
}

fn filename_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn push_now_playing(weak: &Weak<AppWindow>, np: NowPlaying) {
    let _ = weak.upgrade_in_event_loop(move |app| {
        let elapsed = np.elapsed_ms as f32 / 1000.0;
        let length = np.length_ms as f32 / 1000.0;
        app.set_now_title(np.title.into());
        app.set_now_artist(np.artist.into());
        app.set_now_path(np.path.into());
        app.set_elapsed_s(elapsed);
        app.set_length_s(length);
        app.set_progress(if length > 0.0 { elapsed / length } else { 0.0 });
        app.set_elapsed_text(format_time(elapsed as f64).into());
        app.set_duration_text(format_time(length as f64).into());
        let khz = np.frequency as f64 / 1000.0;
        app.set_vfd_info(format!("{:>4} kbps  {khz:.1} kHz", np.bitrate).into());
    });
}

async fn status_loop(channel: Channel, weak: Weak<AppWindow>) {
    loop {
        let mut playback = PlaybackServiceClient::new(channel.clone());
        if let Ok(resp) = playback.stream_status(StreamStatusRequest {}).await {
            let mut stream = resp.into_inner();
            loop {
                match stream.message().await {
                    Ok(Some(msg)) => {
                        // 0 stopped, 1 playing, 2 paused.
                        let playing = msg.status == 1;
                        let stopped = msg.status == 0;
                        let _ = weak.upgrade_in_event_loop(move |app| {
                            app.set_playing(playing);
                            app.set_stopped(stopped);
                        });
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Follows the playlist stream and splits it into Up Next / History around
/// the current index. TrackData.index keeps the ABSOLUTE queue position so
/// row clicks can jump via PlaylistService.Start.
async fn queue_loop(channel: Channel, weak: Weak<AppWindow>) {
    loop {
        let mut playback = PlaybackServiceClient::new(channel.clone());
        if let Ok(resp) = playback.stream_playlist(StreamPlaylistRequest {}).await {
            let mut stream = resp.into_inner();
            loop {
                match stream.message().await {
                    Ok(Some(msg)) => {
                        let current = msg.index as usize;
                        let all: Vec<TrackData> = msg
                            .tracks
                            .iter()
                            .enumerate()
                            .map(|(i, t)| TrackData {
                                id: t.id.clone(),
                                title: if t.title.is_empty() {
                                    filename_stem(&t.path)
                                } else {
                                    t.title.clone()
                                },
                                artist: t.artist.clone(),
                                album: t.album.clone(),
                                length_ms: t.length,
                                index: i as i32,
                            })
                            .collect();
                        let total = all.len();
                        let upnext: Vec<TrackData> = all
                            .iter()
                            .skip(current.saturating_add(1))
                            .cloned()
                            .collect();
                        let mut history: Vec<TrackData> =
                            all.iter().take(current.min(total)).cloned().collect();
                        history.reverse(); // most recent first
                        let _ = weak.upgrade_in_event_loop(move |app| {
                            crate::ui_set_queue(&app, total, upnext, history);
                        });
                    }
                    Ok(None) | Err(_) => break,
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Local clock: advances elapsed between stream updates and animates the VU
/// meters (decorative — the daemon does not export PCM levels over gRPC).
async fn ticker(weak: Weak<AppWindow>) {
    let mut phase: f64 = 0.0;
    loop {
        tokio::time::sleep(Duration::from_millis(250)).await;
        phase += 0.25;
        let t = phase;
        let _ = weak.upgrade_in_event_loop(move |app| {
            if app.get_playing() {
                let length = app.get_length_s();
                let elapsed = (app.get_elapsed_s() + 0.25).min(length.max(0.0));
                app.set_elapsed_s(elapsed);
                app.set_progress(if length > 0.0 { elapsed / length } else { 0.0 });
                app.set_elapsed_text(format_time(elapsed as f64).into());
                let l = 0.62 + 0.22 * (t * 5.9).sin() + 0.12 * (t * 13.7).sin();
                let r = 0.60 + 0.24 * (t * 5.1 + 1.3).sin() + 0.12 * (t * 11.3).sin();
                app.set_vu_left(l.clamp(0.05, 1.0) as f32);
                app.set_vu_right(r.clamp(0.05, 1.0) as f32);
            } else {
                app.set_vu_left((app.get_vu_left() * 0.6).max(0.0));
                app.set_vu_right((app.get_vu_right() * 0.6).max(0.0));
            }
        });
    }
}

async fn init_volume(
    channel: &Channel,
    weak: &Weak<AppWindow>,
    vol_range: &Arc<Mutex<(i32, i32)>>,
) {
    let mut sound = SoundServiceClient::new(channel.clone());
    let min = sound
        .sound_min(SoundMinRequest {
            setting: SOUND_VOLUME,
        })
        .await
        .map(|r| r.into_inner().value)
        .unwrap_or(-80);
    let max = sound
        .sound_max(SoundMaxRequest {
            setting: SOUND_VOLUME,
        })
        .await
        .map(|r| r.into_inner().value)
        .unwrap_or(0);
    let cur = sound
        .sound_current(SoundCurrentRequest {
            setting: SOUND_VOLUME,
        })
        .await
        .map(|r| r.into_inner().value)
        .unwrap_or(max);
    *vol_range.lock().await = (min, max);
    if max > min {
        let pct = (cur - min) as f32 / (max - min) as f32;
        let _ = weak.upgrade_in_event_loop(move |app| app.set_volume(pct.clamp(0.0, 1.0)));
    }
}

const SOUND_BASS: i32 = 1;
const SOUND_TREBLE: i32 = 2;

async fn sound_range(channel: &Channel, setting: i32, fallback: (i32, i32)) -> (i32, i32) {
    let mut sound = SoundServiceClient::new(channel.clone());
    let min = sound
        .sound_min(SoundMinRequest { setting })
        .await
        .map(|r| r.into_inner().value)
        .unwrap_or(fallback.0);
    let max = sound
        .sound_max(SoundMaxRequest { setting })
        .await
        .map(|r| r.into_inner().value)
        .unwrap_or(fallback.1);
    if min == max {
        fallback
    } else {
        (min, max)
    }
}

async fn init_settings(channel: &Channel, weak: &Weak<AppWindow>) {
    let mut settings = SettingsServiceClient::new(channel.clone());
    if let Ok(resp) = settings
        .get_global_settings(GetGlobalSettingsRequest {})
        .await
    {
        let s = resp.into_inner();
        let shuffle = s.playlist_shuffle;
        // Rockbox: 0 off, 1 all, 2 one (higher modes unused here)
        let repeat = s.repeat_mode.clamp(0, 2);
        let _ = weak.upgrade_in_event_loop(move |app| {
            app.set_shuffle(shuffle);
            app.set_repeat_mode(repeat);
        });

        let (bass_min, bass_max) = sound_range(channel, SOUND_BASS, (-24, 24)).await;
        let (treble_min, treble_max) = sound_range(channel, SOUND_TREBLE, (-24, 24)).await;
        let rg = s.replaygain_settings.clone().unwrap_or_default();
        let audio = AudioSettingsData {
            bass: s.bass,
            bass_min,
            bass_max,
            treble: s.treble,
            treble_min,
            treble_max,
            balance: s.balance,
            eq_enabled: s.eq_enabled,
            eq_precut: s.eq_precut as i32,
            bands: s
                .eq_band_settings
                .iter()
                .map(|b| (b.cutoff, b.q, b.gain))
                .collect(),
            rg_type: rg.r#type,
            rg_preamp: rg.preamp,
            rg_noclip: rg.noclip,
            crossfade: s.crossfade,
            fade_in_delay: s.crossfade_fade_in_delay,
            fade_in_duration: s.crossfade_fade_in_duration,
            fade_out_delay: s.crossfade_fade_out_delay,
            fade_out_duration: s.crossfade_fade_out_duration,
            fade_out_mixmode: s.crossfade_fade_out_mixmode,
            dithering: s.dithering_enabled,
        };
        let _ = weak.upgrade_in_event_loop(move |app| {
            crate::ui_set_audio_settings(&app, audio);
        });
    }
}

/// Applies audio-settings snapshots, coalescing bursts (knob/slider drags)
/// into one SaveSettings call per ~150 ms.
async fn audio_save_loop(channel: Channel, mut rx: UnboundedReceiver<AudioSettingsData>) {
    while let Some(mut latest) = rx.recv().await {
        while let Ok(newer) = rx.try_recv() {
            latest = newer;
        }
        let mut settings = SettingsServiceClient::new(channel.clone());
        let req = SaveSettingsRequest {
            bass: Some(latest.bass),
            treble: Some(latest.treble),
            balance: Some(latest.balance),
            eq_enabled: Some(latest.eq_enabled),
            eq_precut: Some(latest.eq_precut),
            eq_band_settings: latest
                .bands
                .iter()
                .map(|&(cutoff, q, gain)| EqBandSetting { cutoff, q, gain })
                .collect(),
            replaygain_settings: Some(ReplaygainSettings {
                noclip: latest.rg_noclip,
                r#type: latest.rg_type,
                preamp: latest.rg_preamp,
            }),
            crossfade: Some(latest.crossfade),
            fade_in_delay: Some(latest.fade_in_delay),
            fade_in_duration: Some(latest.fade_in_duration),
            fade_out_delay: Some(latest.fade_out_delay),
            fade_out_duration: Some(latest.fade_out_duration),
            fade_out_mixmode: Some(latest.fade_out_mixmode),
            dithering_enabled: Some(latest.dithering),
            ..Default::default()
        };
        if let Err(e) = settings.save_settings(req).await {
            tracing::warn!("save audio settings failed: {e}");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

async fn load_library(
    channel: &Channel,
    ep: &Endpoints,
    weak: &Weak<AppWindow>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut lib = LibraryServiceClient::new(channel.clone());

    let albums = lib
        .get_albums(GetAlbumsRequest {})
        .await?
        .into_inner()
        .albums;
    let artists = lib
        .get_artists(GetArtistsRequest {})
        .await?
        .into_inner()
        .artists;
    let tracks = lib
        .get_tracks(GetTracksRequest {})
        .await?
        .into_inner()
        .tracks;
    let liked = lib
        .get_liked_tracks(GetLikedTracksRequest {})
        .await
        .map(|r| r.into_inner().tracks)
        .unwrap_or_default();

    let data = LibraryData {
        albums: albums
            .iter()
            .map(|a| AlbumData {
                id: a.id.clone(),
                title: a.title.clone(),
                artist: a.artist.clone(),
                year: a.year_string.clone(),
                art_file: a.album_art.clone(),
            })
            .collect(),
        artists: artists
            .iter()
            .map(|a| ArtistData {
                id: a.id.clone(),
                name: a.name.clone(),
            })
            .collect(),
        tracks: to_track_data(&tracks),
        liked: to_track_data(&liked),
    };
    let art_files: Vec<(usize, String)> = data
        .albums
        .iter()
        .enumerate()
        .filter_map(|(i, a)| a.art_file.clone().map(|f| (i, f)))
        .collect();

    let _ = weak.upgrade_in_event_loop(move |app| crate::ui_set_library(&app, data));

    // Album art thumbnails, fetched sequentially so we don't hammer the server.
    let covers = ep.covers.clone();
    let weak2 = weak.clone();
    tokio::spawn(async move {
        for (idx, file) in art_files {
            if let Some((w, h, rgba)) = fetch_thumb(&covers, &file, 320).await {
                let _ = weak2.upgrade_in_event_loop(move |app| {
                    crate::ui_set_album_art(&app, idx, w, h, rgba);
                });
            }
        }
    });
    Ok(())
}

fn to_track_data(tracks: &[Track]) -> Vec<TrackData> {
    tracks
        .iter()
        .enumerate()
        .map(|(i, t)| TrackData {
            id: t.id.clone(),
            title: t.title.clone(),
            artist: t.artist.clone(),
            album: t.album.clone(),
            length_ms: t.length as u64,
            index: i as i32,
        })
        .collect()
}

async fn fetch_thumb(covers_base: &str, file: &str, max: u32) -> Option<(u32, u32, Vec<u8>)> {
    let url = format!("{covers_base}{file}");
    let bytes = reqwest::get(&url).await.ok()?.bytes().await.ok()?;
    tokio::task::spawn_blocking(move || {
        let img = image::load_from_memory(&bytes).ok()?;
        let thumb = img.thumbnail(max, max).to_rgba8();
        Some((thumb.width(), thumb.height(), thumb.into_raw()))
    })
    .await
    .ok()?
}

async fn fetch_now_art(covers_base: String, file: String, weak: Weak<AppWindow>) {
    if let Some((w, h, rgba)) = fetch_thumb(&covers_base, &file, 256).await {
        let _ = weak.upgrade_in_event_loop(move |app| {
            crate::ui_set_now_art(&app, w, h, rgba);
        });
    }
}

async fn open_album(
    channel: &Channel,
    weak: &Weak<AppWindow>,
    id: String,
) -> Result<(), tonic::Status> {
    let mut lib = LibraryServiceClient::new(channel.clone());
    let resp = lib.get_album(GetAlbumRequest { id }).await?.into_inner();
    let Some(album) = resp.album else {
        return Ok(());
    };
    let mut tracks = album.tracks.clone();
    tracks.sort_by_key(|t| (t.disc_number, t.track_number));
    let detail = AlbumDetailData {
        id: album.id.clone(),
        title: album.title.clone(),
        artist: album.artist.clone(),
        year: album.year_string.clone(),
        label: album.label.clone().unwrap_or_default(),
        tracks: to_track_data(&tracks),
    };
    let _ = weak.upgrade_in_event_loop(move |app| crate::ui_show_album_detail(&app, detail));
    Ok(())
}

async fn cmd_loop(
    channel: Channel,
    mut rx: UnboundedReceiver<Cmd>,
    vol_range: Arc<Mutex<(i32, i32)>>,
    weak: Weak<AppWindow>,
    _ep: Endpoints,
) {
    while let Some(cmd) = rx.recv().await {
        let mut playback = PlaybackServiceClient::new(channel.clone());
        let mut sound = SoundServiceClient::new(channel.clone());
        let mut playlist = PlaylistServiceClient::new(channel.clone());
        let res: Result<(), tonic::Status> = async {
            match cmd {
                Cmd::PlayPause => {
                    playback.play_or_pause(PlayOrPauseRequest {}).await?;
                }
                Cmd::Next => {
                    playback.next(NextRequest {}).await?;
                }
                Cmd::Previous => {
                    playback.previous(PreviousRequest {}).await?;
                }
                Cmd::SeekMs(ms) => {
                    playback
                        .fast_forward_rewind(FastForwardRewindRequest { new_time: ms })
                        .await?;
                }
                Cmd::SetVolume(pct) => {
                    let (min, max) = *vol_range.lock().await;
                    if max > min {
                        let value = min + ((max - min) as f32 * pct.clamp(0.0, 1.0)).round() as i32;
                        sound
                            .sound_set(SoundSetRequest {
                                setting: SOUND_VOLUME,
                                value,
                            })
                            .await?;
                    }
                }
                Cmd::PlayAlbum(id) => {
                    playback
                        .play_album(PlayAlbumRequest {
                            album_id: id,
                            shuffle: None,
                            position: Some(0),
                        })
                        .await?;
                }
                Cmd::PlayAlbumAt(id, pos) => {
                    playback
                        .play_album(PlayAlbumRequest {
                            album_id: id,
                            shuffle: None,
                            position: Some(pos),
                        })
                        .await?;
                }
                Cmd::PlayArtist(id) => {
                    playback
                        .play_artist_tracks(PlayArtistTracksRequest {
                            artist_id: id,
                            shuffle: None,
                            position: Some(0),
                        })
                        .await?;
                }
                Cmd::PlayAllAt(pos) => {
                    playback
                        .play_all_tracks(PlayAllTracksRequest {
                            shuffle: None,
                            position: Some(pos),
                        })
                        .await?;
                }
                Cmd::PlayLikedAt(pos) => {
                    playback
                        .play_liked_tracks(PlayLikedTracksRequest {
                            shuffle: None,
                            position: Some(pos),
                        })
                        .await?;
                }
                Cmd::QueueJump(idx) => {
                    playlist
                        .start(StartRequest {
                            start_index: Some(idx),
                            elapsed: None,
                            offset: None,
                        })
                        .await?;
                }
                Cmd::OpenAlbum(id) => {
                    open_album(&channel, &weak, id).await?;
                }
                Cmd::PlayAlbumShuffled(id) => {
                    playback
                        .play_album(PlayAlbumRequest {
                            album_id: id,
                            shuffle: Some(true),
                            position: None,
                        })
                        .await?;
                }
                Cmd::SetShuffle(enabled) => {
                    let mut settings = SettingsServiceClient::new(channel.clone());
                    settings
                        .save_settings(SaveSettingsRequest {
                            playlist_shuffle: Some(enabled),
                            ..Default::default()
                        })
                        .await?;
                }
                Cmd::SetRepeat(mode) => {
                    let mut settings = SettingsServiceClient::new(channel.clone());
                    settings
                        .save_settings(SaveSettingsRequest {
                            repeat_mode: Some(mode),
                            ..Default::default()
                        })
                        .await?;
                }
            }
            Ok(())
        }
        .await;
        if let Err(e) = res {
            tracing::warn!("command failed: {e}");
        }
    }
}
