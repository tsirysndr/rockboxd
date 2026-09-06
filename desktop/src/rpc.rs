//! gRPC worker: talks to rockboxd, pushes state into the Slint UI.

use std::sync::{Arc, LazyLock, RwLock as StdRwLock};
use std::time::Duration;

use slint::Weak;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};

use crate::AppWindow;

pub mod api {
    tonic::include_proto!("rockbox.v1alpha1");
}

use api::browse_service_client::BrowseServiceClient;
use api::library_service_client::LibraryServiceClient;
use api::playback_service_client::PlaybackServiceClient;
use api::playlist_service_client::PlaylistServiceClient;
use api::saved_playlist_service_client::SavedPlaylistServiceClient;
use api::settings_service_client::SettingsServiceClient;
use api::sound_service_client::SoundServiceClient;
use api::system_service_client::SystemServiceClient;
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
    ConnectServer(crate::servers::SavedServer),
    Browse {
        title: String,
        path: String,
        push: bool,
    },
    PlayDir(String),
    PlayDirAt(String, i32),
    QueueClear,
    QueueRemove(i32),
    OpenPlaylist(String),
    PlaylistCreate {
        name: String,
        description: String,
    },
    PlaylistUpdate {
        id: String,
        name: String,
        description: String,
    },
    PlaylistDelete(String),
    PlaylistAddTrack {
        playlist_id: String,
        track_id: String,
    },
    PlaylistRemoveTrack {
        playlist_id: String,
        track_id: String,
    },
    PlaySavedPlaylist(String),
    InsertTracks {
        position: i32,
        tracks: Vec<String>,
    },
    InsertAlbum {
        album_id: String,
        position: i32,
    },
    LikeTrack {
        id: String,
        like: bool,
    },
    LikeAlbum(String),
    SwitchServer {
        host: String,
        grpc_port: u16,
    },
    DiscoverServers,
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
    pub disc: i32,
    pub track_no: i32,
    pub path: String,
    pub album_id: String,
}

#[derive(Clone, Debug)]
pub struct PlaylistData {
    pub id: String,
    pub name: String,
    pub description: String,
    pub track_count: i64,
}

#[derive(Clone, Debug)]
pub struct BrowseEntryData {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
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
    pub id: String,
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

/// The daemon the app is currently pointed at. Starts from the env overrides
/// and can be switched at runtime via `switch_server` (server switcher UI).
static TARGET: LazyLock<StdRwLock<(String, u16)>> = LazyLock::new(|| {
    let host = std::env::var("ROCKBOX_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let grpc = std::env::var("ROCKBOX_GRPC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(6061);
    StdRwLock::new((host, grpc))
});

static CHAN: LazyLock<StdRwLock<Channel>> = LazyLock::new(|| StdRwLock::new(make_channel()));

static SWITCH: LazyLock<tokio::sync::broadcast::Sender<()>> =
    LazyLock::new(|| tokio::sync::broadcast::channel(8).0);

fn make_channel() -> Channel {
    let (host, grpc) = TARGET.read().unwrap().clone();
    Endpoint::from_shared(format!("http://{host}:{grpc}"))
        .expect("grpc url")
        .connect_timeout(Duration::from_secs(3))
        .connect_lazy()
}

/// Current channel — cheap to clone; loops fetch a fresh one per iteration so
/// a server switch takes effect everywhere.
fn chan() -> Channel {
    CHAN.read().unwrap().clone()
}

fn switch_rx() -> tokio::sync::broadcast::Receiver<()> {
    SWITCH.subscribe()
}

/// Repoints the app at another rockboxd and interrupts all live streams.
pub fn switch_server(host: &str, grpc_port: u16) {
    *TARGET.write().unwrap() = (host.to_string(), grpc_port);
    *CHAN.write().unwrap() = make_channel();
    let _ = SWITCH.send(());
}

pub fn endpoints() -> Endpoints {
    let (host, grpc_port) = TARGET.read().unwrap().clone();
    let http_port = std::env::var("ROCKBOX_GRAPHQL_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(grpc_port + 1);
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
    // Volume range discovered at connect time, shared with the command loop.
    let vol_range = Arc::new(Mutex::new((-80i32, 0i32)));

    tokio::spawn(cmd_loop(rx, vol_range.clone(), weak.clone()));
    tokio::spawn(audio_save_loop(audio_rx));
    tokio::spawn(status_loop(weak.clone()));
    tokio::spawn(queue_loop(weak.clone()));
    tokio::spawn(ticker(weak.clone()));

    // Main loop: connect → init volume → load library → follow current track.
    // Re-reads chan()/endpoints() every attempt so server switches apply.
    loop {
        let channel = chan();
        let ep = endpoints();
        match session(&channel, &ep, &weak, &vol_range).await {
            Ok(()) => {}
            Err(e) => tracing::debug!("session ended: {e}"),
        }
        let display = endpoints().display;
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
    restore_playback_state(channel, ep, weak).await;
    load_library(channel, ep, weak).await?;
    load_playlists(channel, weak).await;

    // Follow the current track stream until it drops.
    let mut stream = playback
        .stream_current_track(StreamCurrentTrackRequest {})
        .await?
        .into_inner();
    let mut last_art: Option<String> = None;
    let mut sw = switch_rx();
    loop {
        let msg = tokio::select! {
            m = stream.message() => m?,
            _ = sw.recv() => break,
        };
        let Some(msg) = msg else { break };
        let np = NowPlaying {
            id: msg.id.clone(),
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
        let codec = std::path::Path::new(&np.path)
            .extension()
            .map(|e| e.to_string_lossy().to_uppercase())
            .unwrap_or_default();
        // A stopped daemon streams an initial all-zero timing snapshot; don't
        // let it clobber the state restored from the saved queue + resume info.
        app.set_now_track_id(np.id.clone().into());
        app.set_now_liked(crate::is_liked(&np.id));
        if np.length_ms == 0 && np.elapsed_ms == 0 && app.get_stopped() && app.get_length_s() > 0.0
        {
            app.set_now_title(np.title.into());
            app.set_now_artist(np.artist.into());
            app.set_now_path(np.path.into());
            return;
        }
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
        app.set_vfd_info(format!("{codec}  {} kbps  {khz:.1} kHz", np.bitrate).into());
    });
}

/// Splits a queue snapshot around `current` into Up Next / History and pushes
/// it to the UI. TrackData.index keeps the ABSOLUTE queue position.
fn push_queue(weak: &Weak<AppWindow>, current: i32, tracks: &[CurrentTrackResponse]) {
    let current = current.max(0) as usize;
    let all: Vec<TrackData> = tracks
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
            disc: t.discnum,
            track_no: t.tracknum,
            path: t.path.clone(),
            album_id: t.album_id.clone(),
        })
        .collect();
    let total = all.len();
    let upnext: Vec<TrackData> = all
        .iter()
        .skip(current.saturating_add(1))
        .cloned()
        .collect();
    let mut history: Vec<TrackData> = all.iter().take(current.min(total)).cloned().collect();
    history.reverse(); // most recent first
    let _ = weak.upgrade_in_event_loop(move |app| {
        crate::ui_set_queue(&app, total, upnext, history);
    });
}

fn apply_status(weak: &Weak<AppWindow>, status: i32) {
    // Rockbox audio_status() bitmask: bit 0 = play, bit 1 = pause.
    let playing = status & 0x01 != 0 && status & 0x02 == 0;
    let stopped = status & 0x01 == 0;
    let _ = weak.upgrade_in_event_loop(move |app| {
        app.set_playing(playing);
        app.set_stopped(stopped);
    });
}

/// After a daemon restart: reload the saved queue and resume position so the
/// mini-player and queue drawer match the pre-restart state (same recipe as
/// the GPUI client). StreamPlaylist only delivers FUTURE publishes, so the
/// snapshot fetch here is what fills the queue on connect; the resume calls
/// are no-ops when playback is already going.
async fn restore_playback_state(channel: &Channel, ep: &Endpoints, weak: &Weak<AppWindow>) {
    let mut playlist = PlaylistServiceClient::new(channel.clone());
    let mut playback = PlaybackServiceClient::new(channel.clone());
    let mut system = SystemServiceClient::new(channel.clone());

    // The status stream only fires on CHANGES — fetch the initial value once.
    let status = playback
        .status(StatusRequest {})
        .await
        .map(|r| r.into_inner().status)
        .unwrap_or(0);
    apply_status(weak, status);

    // The gRPC port binds before the firmware finishes loading saved playlist
    // state from disk — retry with back-off for up to ~5 s.
    let mut snapshot: Option<(i32, Vec<CurrentTrackResponse>)> = None;
    for attempt in 0u32..10 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        let _ = playlist.playlist_resume(PlaylistResumeRequest {}).await;
        if let Ok(resp) = playlist.get_current(GetCurrentRequest {}).await {
            let q = resp.into_inner();
            if !q.tracks.is_empty() {
                push_queue(weak, q.index, &q.tracks);
                snapshot = Some((q.index, q.tracks));
                break;
            }
        }
    }

    // When the daemon boots stopped, StreamCurrentTrack's initial message
    // carries zeros for length/elapsed — rebuild the mini-player state from
    // the queue snapshot + the saved resume position instead.
    if status & 0x01 == 0 {
        if let Some((index, tracks)) = snapshot {
            if let Some(cur) = tracks.get(index.max(0) as usize) {
                let elapsed_ms = system
                    .get_global_status(GetGlobalStatusRequest {})
                    .await
                    .map(|r| r.into_inner().resume_elapsed as u64)
                    .unwrap_or(0);
                let np = NowPlaying {
                    id: cur.id.clone(),
                    title: if cur.title.is_empty() {
                        filename_stem(&cur.path)
                    } else {
                        cur.title.clone()
                    },
                    artist: cur.artist.clone(),
                    path: cur.path.clone(),
                    elapsed_ms: elapsed_ms.min(cur.length),
                    length_ms: cur.length,
                    bitrate: cur.bitrate,
                    frequency: cur.frequency,
                    art_file: cur.album_art.clone().filter(|a| !a.is_empty()),
                };
                if let Some(file) = np.art_file.clone() {
                    tokio::spawn(fetch_now_art(ep.covers.clone(), file, weak.clone()));
                }
                push_now_playing(weak, np);
            }
        }
    }
}

async fn status_loop(weak: Weak<AppWindow>) {
    loop {
        let mut playback = PlaybackServiceClient::new(chan());
        let mut sw = switch_rx();
        if let Ok(resp) = playback.stream_status(StreamStatusRequest {}).await {
            let mut stream = resp.into_inner();
            loop {
                tokio::select! {
                    msg = stream.message() => match msg {
                        Ok(Some(msg)) => apply_status(&weak, msg.status),
                        Ok(None) | Err(_) => break,
                    },
                    _ = sw.recv() => break,
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Follows the playlist stream and splits it into Up Next / History around
/// the current index. TrackData.index keeps the ABSOLUTE queue position so
/// row clicks can jump via PlaylistService.Start.
async fn queue_loop(weak: Weak<AppWindow>) {
    loop {
        let mut playback = PlaybackServiceClient::new(chan());
        let mut sw = switch_rx();
        if let Ok(resp) = playback.stream_playlist(StreamPlaylistRequest {}).await {
            let mut stream = resp.into_inner();
            loop {
                tokio::select! {
                    msg = stream.message() => match msg {
                        Ok(Some(msg)) => push_queue(&weak, msg.index, &msg.tracks),
                        Ok(None) | Err(_) => break,
                    },
                    _ = sw.recv() => break,
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

/// Local clock: advances elapsed between stream updates and animates the VU
/// meters (decorative — the daemon does not export PCM levels over gRPC).
async fn ticker(weak: Weak<AppWindow>) {
    const TICK_S: f64 = 0.07; // ~14 fps — snappy VU without burning CPU
    let mut phase: f64 = 0.0;
    loop {
        tokio::time::sleep(Duration::from_millis((TICK_S * 1000.0) as u64)).await;
        phase += TICK_S;
        let t = phase;
        let _ = weak.upgrade_in_event_loop(move |app| {
            if app.get_playing() {
                let length = app.get_length_s();
                let elapsed = (app.get_elapsed_s() + TICK_S as f32).min(length.max(0.0));
                app.set_elapsed_s(elapsed);
                app.set_progress(if length > 0.0 { elapsed / length } else { 0.0 });
                app.set_elapsed_text(format_time(elapsed as f64).into());
                // Lively pseudo-VU: a few incommensurate sines per channel so
                // the pattern never visibly repeats.
                let l = 0.60
                    + 0.24 * (t * 14.3).sin()
                    + 0.14 * (t * 33.7).sin()
                    + 0.08 * (t * 7.1).sin();
                let r = 0.58
                    + 0.26 * (t * 12.9 + 1.3).sin()
                    + 0.14 * (t * 29.1).sin()
                    + 0.08 * (t * 8.3 + 0.7).sin();
                app.set_vu_left(l.clamp(0.05, 1.0) as f32);
                app.set_vu_right(r.clamp(0.05, 1.0) as f32);
            } else {
                app.set_vu_left((app.get_vu_left() * 0.8).max(0.0));
                app.set_vu_right((app.get_vu_right() * 0.8).max(0.0));
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
async fn audio_save_loop(mut rx: UnboundedReceiver<AudioSettingsData>) {
    while let Some(mut latest) = rx.recv().await {
        while let Ok(newer) = rx.try_recv() {
            latest = newer;
        }
        let mut settings = SettingsServiceClient::new(chan());
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
            disc: t.disc_number as i32,
            track_no: t.track_number as i32,
            path: t.path.clone(),
            album_id: t.album_id.clone().unwrap_or_default(),
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
    // The library can hold the same song twice (rescans, duplicate files);
    // don't show it twice within one album.
    tracks.dedup_by(|a, b| {
        a.disc_number == b.disc_number && a.track_number == b.track_number && a.title == b.title
    });
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

async fn load_playlists(channel: &Channel, weak: &Weak<AppWindow>) {
    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
    if let Ok(resp) = sp
        .get_saved_playlists(GetSavedPlaylistsRequest { folder_id: None })
        .await
    {
        let data: Vec<PlaylistData> = resp
            .into_inner()
            .playlists
            .into_iter()
            .map(|p| PlaylistData {
                id: p.id,
                name: p.name,
                description: p.description.unwrap_or_default(),
                track_count: p.track_count,
            })
            .collect();
        let _ = weak.upgrade_in_event_loop(move |app| crate::ui_set_playlists(&app, data));
    }
}

async fn open_playlist(
    channel: &Channel,
    weak: &Weak<AppWindow>,
    id: String,
    open_picker: bool,
) -> Result<(), tonic::Status> {
    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
    let ids = sp
        .get_saved_playlist_tracks(GetSavedPlaylistTracksRequest {
            playlist_id: id.clone(),
        })
        .await?
        .into_inner()
        .track_ids;
    let _ = weak.upgrade_in_event_loop(move |app| {
        crate::ui_show_playlist(&app, id, ids, open_picker);
    });
    Ok(())
}

/// Re-fetches the queue snapshot after a mutation (the playlist stream does
/// not always publish for removals).
async fn refresh_queue(channel: &Channel, weak: &Weak<AppWindow>) {
    let mut playlist = PlaylistServiceClient::new(channel.clone());
    if let Ok(resp) = playlist.get_current(GetCurrentRequest {}).await {
        let q = resp.into_inner();
        push_queue(weak, q.index, &q.tracks);
    }
}

/// Fetches a browse level and pushes it to the UI. Entries are sorted
/// dirs-first + case-insensitive alpha — the SAME order the display uses,
/// because PlayDirectory's `position` refers to this ordering.
async fn browse_into(
    channel: &Channel,
    weak: &Weak<AppWindow>,
    title: String,
    path: String,
    push: bool,
) -> Result<(), tonic::Status> {
    let mut browse = BrowseServiceClient::new(channel.clone());
    let resp = browse
        .tree_get_entries(TreeGetEntriesRequest {
            path: Some(path.clone()),
        })
        .await?
        .into_inner();
    let mut entries: Vec<BrowseEntryData> = resp
        .entries
        .into_iter()
        .map(|e| {
            let is_dir = e.attr == 0x10;
            let name = e
                .display_name
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| filename_stem(&e.name));
            BrowseEntryData {
                name,
                path: e.name,
                is_dir,
            }
        })
        .collect();
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    let _ = weak.upgrade_in_event_loop(move |app| {
        crate::ui_browse_opened(&app, title, path, entries, push);
    });
    Ok(())
}

/// Browses `_rockbox._tcp.local.` for ~2.5 s and returns (name, host, port)
/// per resolved peer (deduped). Peers that resolve to one of THIS machine's
/// own addresses are dropped — they are just the local daemon's adverts
/// echoed on every interface, already covered by the "This Mac" entry.
async fn discover_rockbox_servers() -> Vec<(String, String, u16)> {
    let own_addrs: std::collections::HashSet<String> = if_addrs::get_if_addrs()
        .map(|ifs| ifs.into_iter().map(|i| i.addr.ip().to_string()).collect())
        .unwrap_or_default();
    let found = Arc::new(Mutex::new(Vec::<(String, String, u16)>::new()));
    let found2 = found.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let Ok(daemon) = mdns_sd::ServiceDaemon::new() else {
            return;
        };
        let Ok(receiver) = daemon.browse("_rockbox._tcp.local.") else {
            return;
        };
        let deadline = std::time::Instant::now() + Duration::from_millis(2500);
        while let Ok(event) =
            receiver.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
        {
            if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
                let name = info
                    .get_fullname()
                    .trim_end_matches("._rockbox._tcp.local.")
                    .to_string();
                // rockboxd advertises mpd-/grpc-/graphql- instances per host;
                // only the gRPC one is a valid switch target.
                let Some(name) = name.strip_prefix("grpc-").map(|n| n.to_string()) else {
                    continue;
                };
                let port = info.get_port();
                for addr in info.get_addresses() {
                    if addr.is_ipv4() {
                        let host = addr.to_string();
                        if own_addrs.contains(&host) {
                            continue;
                        }
                        let mut list = found2.blocking_lock();
                        if !list.iter().any(|(_, h, p)| h == &host && *p == port) {
                            list.push((name.clone(), host, port));
                        }
                    }
                }
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
        }
        let _ = daemon.shutdown();
    });
    let _ = handle.await;
    let list = found.lock().await.clone();
    list
}

async fn cmd_loop(
    mut rx: UnboundedReceiver<Cmd>,
    vol_range: Arc<Mutex<(i32, i32)>>,
    weak: Weak<AppWindow>,
) {
    while let Some(cmd) = rx.recv().await {
        let channel = chan();
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
                Cmd::QueueClear => {
                    playlist
                        .remove_all_tracks(RemoveAllTracksRequest {})
                        .await?;
                    refresh_queue(&channel, &weak).await;
                }
                Cmd::QueueRemove(pos) => {
                    playlist
                        .remove_tracks(RemoveTracksRequest {
                            positions: vec![pos],
                        })
                        .await?;
                    refresh_queue(&channel, &weak).await;
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
                Cmd::OpenPlaylist(id) => {
                    open_playlist(&channel, &weak, id, false).await?;
                }
                Cmd::PlaylistCreate { name, description } => {
                    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
                    let resp = sp
                        .create_saved_playlist(CreateSavedPlaylistRequest {
                            name,
                            description: Some(description).filter(|d| !d.is_empty()),
                            image: None,
                            folder_id: None,
                            track_ids: vec![],
                        })
                        .await?
                        .into_inner();
                    load_playlists(&channel, &weak).await;
                    if let Some(p) = resp.playlist {
                        open_playlist(&channel, &weak, p.id, true).await?;
                    }
                }
                Cmd::PlaylistUpdate {
                    id,
                    name,
                    description,
                } => {
                    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
                    sp.update_saved_playlist(UpdateSavedPlaylistRequest {
                        id: id.clone(),
                        name,
                        description: Some(description).filter(|d| !d.is_empty()),
                        image: None,
                        folder_id: None,
                    })
                    .await?;
                    load_playlists(&channel, &weak).await;
                    open_playlist(&channel, &weak, id, false).await.ok();
                }
                Cmd::PlaylistDelete(id) => {
                    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
                    sp.delete_saved_playlist(DeleteSavedPlaylistRequest { id })
                        .await?;
                    load_playlists(&channel, &weak).await;
                }
                Cmd::PlaylistAddTrack {
                    playlist_id,
                    track_id,
                } => {
                    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
                    sp.add_tracks_to_saved_playlist(AddTracksToSavedPlaylistRequest {
                        playlist_id: playlist_id.clone(),
                        track_ids: vec![track_id],
                    })
                    .await?;
                    load_playlists(&channel, &weak).await;
                    open_playlist(&channel, &weak, playlist_id, false)
                        .await
                        .ok();
                }
                Cmd::PlaylistRemoveTrack {
                    playlist_id,
                    track_id,
                } => {
                    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
                    sp.remove_track_from_saved_playlist(RemoveTrackFromSavedPlaylistRequest {
                        playlist_id: playlist_id.clone(),
                        track_id,
                    })
                    .await?;
                    load_playlists(&channel, &weak).await;
                    open_playlist(&channel, &weak, playlist_id, false)
                        .await
                        .ok();
                }
                Cmd::PlaySavedPlaylist(id) => {
                    let mut sp = SavedPlaylistServiceClient::new(channel.clone());
                    sp.play_saved_playlist(PlaySavedPlaylistRequest { playlist_id: id })
                        .await?;
                }
                Cmd::InsertTracks { position, tracks } => {
                    playlist
                        .insert_tracks(InsertTracksRequest {
                            playlist_id: None,
                            position,
                            tracks,
                            shuffle: None,
                        })
                        .await?;
                    refresh_queue(&channel, &weak).await;
                }
                Cmd::InsertAlbum { album_id, position } => {
                    playlist
                        .insert_album(InsertAlbumRequest {
                            position,
                            album_id,
                            shuffle: None,
                        })
                        .await?;
                    refresh_queue(&channel, &weak).await;
                }
                Cmd::LikeTrack { id, like } => {
                    let mut lib = LibraryServiceClient::new(channel.clone());
                    if like {
                        lib.like_track(LikeTrackRequest { id }).await?;
                    } else {
                        lib.unlike_track(UnlikeTrackRequest { id }).await?;
                    }
                    if let Ok(resp) = lib.get_liked_tracks(GetLikedTracksRequest {}).await {
                        let liked = to_track_data(&resp.into_inner().tracks);
                        let _ = weak.upgrade_in_event_loop(move |app| {
                            crate::ui_set_liked(&app, liked);
                        });
                    }
                }
                Cmd::SwitchServer { host, grpc_port } => {
                    switch_server(&host, grpc_port);
                    let display = format!("{host}:{grpc_port}");
                    let _ = weak.upgrade_in_event_loop(move |app| {
                        app.set_connected(false);
                        app.set_status_text(display.into());
                    });
                }
                Cmd::DiscoverServers => {
                    let weak2 = weak.clone();
                    tokio::spawn(async move {
                        let found = discover_rockbox_servers().await;
                        let _ = weak2.upgrade_in_event_loop(move |app| {
                            crate::ui_set_discovered(&app, found);
                        });
                    });
                }
                Cmd::LikeAlbum(id) => {
                    let mut lib = LibraryServiceClient::new(channel.clone());
                    lib.like_album(LikeAlbumRequest { id }).await?;
                }
                Cmd::ConnectServer(srv) => {
                    let name = srv.name.clone();
                    let result = match crate::servers::root_url(&srv).await {
                        Ok(root) => browse_into(&channel, &weak, name, root, true)
                            .await
                            .map_err(|e| e.message().to_string()),
                        Err(e) => Err(e),
                    };
                    if let Err(e) = result {
                        let _ = weak.upgrade_in_event_loop(move |app| {
                            app.set_browse_loading(false);
                            app.set_browse_error(e.into());
                        });
                    }
                }
                Cmd::Browse { title, path, push } => {
                    if let Err(e) = browse_into(&channel, &weak, title, path, push).await {
                        let msg = e.message().to_string();
                        let _ = weak.upgrade_in_event_loop(move |app| {
                            app.set_browse_loading(false);
                            app.set_browse_error(msg.into());
                        });
                    }
                }
                Cmd::PlayDir(path) => {
                    playback
                        .play_directory(PlayDirectoryRequest {
                            path,
                            shuffle: Some(false),
                            recurse: Some(true),
                            position: None,
                        })
                        .await?;
                }
                Cmd::PlayDirAt(path, position) => {
                    playback
                        .play_directory(PlayDirectoryRequest {
                            path,
                            shuffle: Some(false),
                            recurse: Some(true),
                            position: Some(position),
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
