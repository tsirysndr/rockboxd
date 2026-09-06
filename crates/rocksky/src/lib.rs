use anyhow::anyhow;
use anyhow::Error;
use api::rockbox::v1alpha1::playback_service_client::PlaybackServiceClient;
use api::rockbox::v1alpha1::playlist_service_client::PlaylistServiceClient;
use api::rockbox::v1alpha1::NextRequest;
use api::rockbox::v1alpha1::PauseRequest;
use api::rockbox::v1alpha1::PlayRequest;
use api::rockbox::v1alpha1::PreviousRequest;
use api::rockbox::v1alpha1::ResumeRequest;
use api::rockbox::v1alpha1::ResumeTrackRequest;
use api::rockbox::v1alpha1::StatusRequest;
use api::rockbox::v1alpha1::StreamCurrentTrackRequest;
use api::rockbox::v1alpha1::StreamStatusRequest;
use lofty::file::TaggedFileExt;
use reqwest::multipart;
use reqwest::Client;
use rockbox_library::entity::album::Album;
use rockbox_library::entity::track::Track;
use rocksky_sdk::{
    RemoteCommand, RemoteNowPlaying, RemotePlayer, RemotePlayerConfig, RemoteStatus,
};
use std::env;
use std::fs::File;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tonic::transport::Channel;

const AUDIO_EXTENSIONS: [&str; 18] = [
    "mp3", "ogg", "flac", "m4a", "aac", "mp4", "alac", "wav", "wv", "mpc", "aiff", "aif", "ac3",
    "opus", "spx", "sid", "ape", "wma",
];

/// Minimal view over `~/.config/rockbox.org/settings.toml` — only the fields we
/// need for the remote-control device label. Unknown keys are ignored by serde,
/// so the full settings file deserializes cleanly into this subset.
#[derive(serde::Deserialize, Default)]
struct RockskyDeviceSettings {
    device_name: Option<String>,
    player_name: Option<String>,
}

/// The display name this device advertises to the Rocksky miniplayers on
/// `register`. Read from settings.toml (`device_name`, falling back to the
/// existing `player_name`), defaulting to "Rockbox".
fn rocksky_device_name() -> String {
    let fallback = || "Rockbox".to_string();
    let Some(home) = dirs::home_dir() else {
        return fallback();
    };
    let path = home
        .join(".config")
        .join("rockbox.org")
        .join("settings.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return fallback();
    };
    let settings: RockskyDeviceSettings = toml::from_str(&content).unwrap_or_default();
    settings
        .device_name
        .or(settings.player_name)
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(fallback)
}

pub mod api {
    #[path = ""]
    pub mod rockbox {

        #[path = "rockbox.v1alpha1.rs"]
        pub mod v1alpha1;
    }
}

fn grpc_url() -> String {
    let host = env::var("ROCKBOX_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("ROCKBOX_PORT").unwrap_or_else(|_| "6061".to_string());
    format!("tcp://{}:{}", host, port)
}

/// Connect to the local rockboxd gRPC server, retrying up to 10 times — the
/// remote-player bridge starts alongside the daemon, before the server binds.
async fn connect_playback_client() -> Result<PlaybackServiceClient<Channel>, Error> {
    let url = grpc_url();
    for attempt in 1..=10 {
        match PlaybackServiceClient::connect(url.clone()).await {
            Ok(client) => return Ok(client),
            Err(e) if attempt < 10 => {
                tracing::warn!(
                    "gRPC connection attempt {}/10 failed: {}. Retrying...",
                    attempt,
                    e
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to connect to Rockbox gRPC server after 10 attempts: {}",
                    e
                ));
            }
        }
    }
    unreachable!()
}

/// Run the Rocksky remote-player bridge. Registration, heartbeat, reconnect,
/// and the device-id handshake are all owned by `rocksky_sdk::RemotePlayer`;
/// this function only maps controller commands onto the local gRPC daemon and
/// pushes daemon state (now-playing / transport status) back through the SDK.
pub async fn run_remote_player(token: String) -> Result<(), Error> {
    // Install the default crypto provider for rustls 0.23+. ring instead
    // of aws_lc_rs because aws-lc-sys's cmake cross-compile doesn't survive
    // cargo-ndk's flag injection on Android.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let ws_url =
        env::var("ROCKSKY_WS").unwrap_or_else(|_| rocksky_sdk::DEFAULT_REMOTE_WS.to_string());
    let device_name = rocksky_device_name();
    tracing::info!("Registering as device \"{}\" on {}", device_name, ws_url);

    let remote = Arc::new(RemotePlayer::connect(
        RemotePlayerConfig::new(token, device_name).url(ws_url),
    ));

    let mut client = connect_playback_client().await?;
    // Separate client for the playlist service — used to resume from saved
    // state after a daemon (re)start (see the Play command handler below).
    let mut playlist_client = PlaylistServiceClient::connect(grpc_url())
        .await
        .map_err(|e| anyhow!("Failed to connect to Rockbox playlist gRPC service: {}", e))?;

    // The status stream owns this flag; the track stream reads it, because
    // RemoteNowPlaying carries is_playing but the current-track gRPC stream
    // alone doesn't know the transport state.
    let is_playing = Arc::new(AtomicBool::new(false));

    tokio::spawn(forward_track_stream(remote.clone(), is_playing.clone()));
    tokio::spawn(forward_status_stream(remote.clone(), is_playing.clone()));

    while let Some(cmd) = remote.next_command().await {
        if let Err(e) = apply_command(&mut client, &mut playlist_client, cmd).await {
            tracing::warn!("Failed to apply remote command: {}", e);
        }
    }

    Err(anyhow!("Remote player disconnected"))
}

async fn apply_command(
    client: &mut PlaybackServiceClient<Channel>,
    playlist_client: &mut PlaylistServiceClient<Channel>,
    cmd: RemoteCommand,
) -> Result<(), Error> {
    match cmd {
        RemoteCommand::Play => {
            // Decide between resume-from-pause and resume-from-saved state
            // based on the engine's current status (a bitmask: PLAY=0x01,
            // PAUSE=0x02 → 0 = stopped, 1 = playing, 3 = paused). On a fresh
            // daemon start the engine is STOPPED, not paused, so a plain
            // resume() is a no-op — we must resume_track() to restore the
            // playlist from the control file and seek to the saved position
            // (mirrors the GPUI play/pause handler).
            let status = client
                .status(tonic::Request::new(StatusRequest {}))
                .await?
                .into_inner()
                .status;
            if status == 0 {
                playlist_client
                    .resume_track(tonic::Request::new(ResumeTrackRequest {
                        start_index: 0,
                        crc: 0,
                        elapsed: 0,
                        offset: 0,
                    }))
                    .await?;
            } else {
                client.resume(tonic::Request::new(ResumeRequest {})).await?;
            }
        }
        RemoteCommand::Pause => {
            client
                .pause(tonic::Request::new(PauseRequest::default()))
                .await?;
        }
        RemoteCommand::Next => {
            client
                .next(tonic::Request::new(NextRequest::default()))
                .await?;
        }
        RemoteCommand::Previous => {
            client
                .previous(tonic::Request::new(PreviousRequest::default()))
                .await?;
        }
        RemoteCommand::Seek { position_ms } => {
            client
                .play(tonic::Request::new(PlayRequest {
                    offset: 0,
                    elapsed: position_ms as i64,
                }))
                .await?;
        }
        // Queue / shuffle / repeat / volume / audio-settings commands are
        // deliberately not implemented for this bridge — rockboxd's own
        // playlist engine is the source of truth and the now-playing pushes
        // leave shuffle/repeat/volume unset, so controllers hide those
        // controls rather than show them wrong.
        other => {
            tracing::debug!("Unsupported remote command: {:?}", other);
        }
    }
    Ok(())
}

/// Forward the daemon's current-track gRPC stream to the miniplayers,
/// reconnecting if the stream ends (e.g. the gRPC server restarts).
async fn forward_track_stream(remote: Arc<RemotePlayer>, is_playing: Arc<AtomicBool>) {
    loop {
        if let Err(e) = track_stream_session(&remote, &is_playing).await {
            tracing::warn!("Track stream ended: {}. Reconnecting...", e);
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn track_stream_session(remote: &RemotePlayer, is_playing: &AtomicBool) -> Result<(), Error> {
    let mut client = connect_playback_client().await?;
    let mut stream = client
        .stream_current_track(tonic::Request::new(StreamCurrentTrackRequest {}))
        .await?
        .into_inner();

    while let Some(track) = stream.message().await? {
        remote.set_now_playing(RemoteNowPlaying {
            title: track.title,
            artist: track.artist,
            album: track.album,
            album_artist: track.album_artist,
            album_art: track.album_art.unwrap_or_default(),
            duration_ms: track.length,
            elapsed_ms: track.elapsed,
            is_playing: is_playing.load(Ordering::Relaxed),
            sample_rate: (track.frequency > 0).then_some(track.frequency as u32),
            ..Default::default()
        });
    }

    Err(anyhow!("current-track stream closed"))
}

/// Forward the daemon's transport-status gRPC stream to the miniplayers,
/// reconnecting if the stream ends.
async fn forward_status_stream(remote: Arc<RemotePlayer>, is_playing: Arc<AtomicBool>) {
    loop {
        if let Err(e) = status_stream_session(&remote, &is_playing).await {
            tracing::warn!("Status stream ended: {}. Reconnecting...", e);
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn status_stream_session(
    remote: &RemotePlayer,
    is_playing: &AtomicBool,
) -> Result<(), Error> {
    let mut client = connect_playback_client().await?;
    let mut stream = client
        .stream_status(tonic::Request::new(StreamStatusRequest {}))
        .await?
        .into_inner();

    while let Some(status) = stream.message().await? {
        // Rockbox status is a bitmask: PLAY=0x01, PAUSE=0x02 → 0 = stopped,
        // 1 = playing, 3 = paused.
        let status = match status.status {
            1 => RemoteStatus::Playing,
            2 | 3 => RemoteStatus::Paused,
            _ => RemoteStatus::Stopped,
        };
        is_playing.store(status == RemoteStatus::Playing, Ordering::Relaxed);
        remote.set_status(status);
    }

    Err(anyhow!("status stream closed"))
}

pub fn register_rockbox() -> Result<(), Error> {
    let home = dirs::home_dir().unwrap();
    let token_file = home.join(".config").join("rockbox.org").join("token");

    if !token_file.exists() {
        return Ok(());
    }

    let token = std::fs::read_to_string(token_file)?;

    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let delay = 3;

            // The SDK reconnects the WebSocket itself; this outer loop only
            // covers run_remote_player bailing before the bridge is up (e.g.
            // the local gRPC server never came within the connect retries).
            loop {
                match run_remote_player(token.clone()).await {
                    Ok(_) => {
                        tracing::info!("Remote player session ended cleanly");
                    }
                    Err(e) => {
                        tracing::error!("Remote player session error: {:#?}", e);
                    }
                }

                tracing::info!("Restarting remote player in {} seconds...", delay);
                tokio::time::sleep(Duration::from_secs(delay)).await;
            }
        })
    });

    Ok(())
}

pub async fn upload_album_cover(name: &str) -> Result<(), Error> {
    let home = dirs::home_dir().unwrap();
    let cover = home
        .join(".config")
        .join("rockbox.org")
        .join("covers")
        .join(name);

    let mut file = File::open(&cover)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let part = multipart::Part::bytes(buffer).file_name(cover.display().to_string());
    let form = multipart::Form::new().part("file", part);

    let token_file = home.join(".config").join("rockbox.org").join("token");

    if !token_file.exists() {
        return Ok(());
    }

    let token = std::fs::read_to_string(token_file)?;

    let client = Client::new();

    const URL: &str = "https://uploads.rocksky.app";

    let response = client
        .post(URL)
        .header("Authorization", format!("Bearer {}", token))
        .multipart(form)
        .send()
        .await?;

    tracing::info!("Cover uploaded: {}", response.status());

    Ok(())
}

pub async fn scrobble(track: Track, album: Album) -> Result<(), Error> {
    let home = dirs::home_dir().unwrap();
    let token_file = home.join(".config").join("rockbox.org").join("token");

    if !token_file.exists() {
        return Ok(());
    }

    let token = std::fs::read_to_string(token_file)?;

    if let Some(album_art) = track.album_art.clone() {
        match upload_album_cover(&album_art).await {
            Ok(_) => {}
            Err(r) => {
                tracing::warn!("Failed to upload album art: {}", r);
            }
        }
    }

    let (lyrics, copyright_message) = match parse_lyrics_and_copyright(&track.path) {
        Ok((lyrics, copyright_message)) => (lyrics, copyright_message),
        Err(_) => (None, None),
    };

    let client = Client::new();
    const URL: &str = "https://api.rocksky.app/now-playing";
    let response = client
        .post(URL)
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "title": track.title,
            "album": track.album,
            "artist": track.artist,
            "albumArtist": track.album_artist,
            "duration": track.length,
            "trackNumber": track.track_number,
            "releaseDate": match album.year_string.contains("-") {
                true => Some(album.year_string),
                false => None,
            },
            "year": album.year,
            "discNumber": track.disc_number,
            "composer": track.composer,
            "albumArt": match track.album_art.is_some() {
                true => Some(format!("https://cdn.rocksky.app/covers/{}", track.album_art.unwrap())),
                false => None
            },
            "lyrics": lyrics,
            "copyrightMessage": copyright_message,
        }))
        .send()
        .await?;
    tracing::info!("Scrobbled: {}", response.status());

    if !response.status().is_success() {
        tracing::warn!("Failed to scrobble: {}", response.text().await?);
    }

    Ok(())
}

pub async fn save_track(track: Track, album: Album) -> Result<(), Error> {
    let home = dirs::home_dir().unwrap();
    let token_file = home.join(".config").join("rockbox.org").join("token");

    if !token_file.exists() {
        return Ok(());
    }

    let token = std::fs::read_to_string(token_file)?;

    if let Some(album_art) = track.album_art.clone() {
        match upload_album_cover(&album_art).await {
            Ok(_) => {}
            Err(r) => {
                tracing::warn!("Failed to upload album art: {}", r);
            }
        }
    }

    let (lyrics, copyright_message) = match parse_lyrics_and_copyright(&track.path) {
        Ok((lyrics, copyright_message)) => (lyrics, copyright_message),
        Err(_) => (None, None),
    };

    let client = Client::new();
    const URL: &str = "https://api.rocksky.app/tracks";
    let response = client
        .post(URL)
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "title": track.title,
            "album": track.album,
            "artist": track.artist,
            "albumArtist": match track.album_artist.is_empty() {
                true => track.artist,
                false => track.album_artist,
            },
            "duration": track.length,
            "trackNumber": track.track_number,
            "releaseDate": match album.year_string.contains("-") {
                true => Some(album.year_string),
                false => None,
            },
            "year": album.year,
            "discNumber": track.disc_number,
            "composer": track.composer,
            "albumArt": match track.album_art.is_some() {
                true => Some(format!("https://cdn.rocksky.app/covers/{}", track.album_art.unwrap())),
                false => None
            },
            "lyrics": lyrics,
            "copyrightMessage": copyright_message,
        }))
        .send()
        .await?;
    tracing::info!("Track Saved: {} {}", track.path, response.status());

    if !response.status().is_success() {
        tracing::warn!(
            "Failed to save Track: {} {}",
            track.path,
            response.text().await?
        );
    }

    Ok(())
}

pub async fn like(track: Track, album: Album) -> Result<(), Error> {
    let home = dirs::home_dir().unwrap();
    let token_file = home.join(".config").join("rockbox.org").join("token");

    if !token_file.exists() {
        return Ok(());
    }

    let token = std::fs::read_to_string(token_file)?;

    if let Some(album_art) = track.album_art.clone() {
        match upload_album_cover(&album_art).await {
            Ok(_) => {}
            Err(r) => {
                tracing::warn!("Failed to upload album art: {}", r);
            }
        }
    }

    let client = Client::new();
    const URL: &str = "https://api.rocksky.app/likes";
    let response = client
        .post(URL)
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({
            "title": track.title,
            "album": track.album,
            "artist": track.artist,
            "albumArtist": track.album_artist,
            "duration": track.length,
            "trackNumber": track.track_number,
            "releaseDate": match album.year_string.contains("-") {
                true => Some(album.year_string),
                false => None,
            },
            "year": album.year,
            "discNumber": track.disc_number,
            "composer": track.composer,
            "albumArt": match track.album_art.is_some() {
                true => Some(format!("https://cdn.rocksky.app/covers/{}", track.album_art.unwrap())),
                false => None
            }
        }))
        .send()
        .await?;
    tracing::info!("Liked: {}", response.status());
    Ok(())
}

pub async fn unlike(track: Track) -> Result<(), Error> {
    let home = dirs::home_dir().unwrap();
    let token_file = home.join(".config").join("rockbox.org").join("token");

    if !token_file.exists() {
        return Ok(());
    }

    let token = std::fs::read_to_string(token_file)?;

    let hash = sha256::digest(
        format!("{} - {} - {}", track.title, track.artist, track.album).to_lowercase(),
    );

    let client = Client::new();
    let url: &str = &format!("https://api.rocksky.app/likes/{}", hash);
    let response = client
        .delete(url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?;

    tracing::info!("Unliked: {} {}", response.status(), hash);

    Ok(())
}

fn parse_lyrics_and_copyright(path: &str) -> Result<(Option<String>, Option<String>), Error> {
    if !AUDIO_EXTENSIONS
        .into_iter()
        .any(|ext| path.ends_with(&format!(".{}", ext)))
    {
        return Ok((None, None));
    }

    let tagged_file = lofty::read_from_path(path)?;

    let tag = match tagged_file.primary_tag() {
        Some(primary_tag) => primary_tag,
        None => tagged_file.first_tag().expect("No tags found"),
    };

    let lyrics = tag
        .get_string(&lofty::tag::ItemKey::Lyrics)
        .map(|x| x.to_string());
    let copyright_message = tag
        .get_string(&lofty::tag::ItemKey::CopyrightMessage)
        .map(|x| x.to_string());

    Ok((lyrics, copyright_message))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_parse_metadata() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("fixtures");
        path.push("08 - Internet Money - Speak(Explicit).m4a");

        let result = parse_lyrics_and_copyright(path.to_str().unwrap());
        assert!(result.is_ok());

        let result = result.unwrap();

        assert!(result.0.is_some());
        assert!(result.1.is_some());
    }
}
