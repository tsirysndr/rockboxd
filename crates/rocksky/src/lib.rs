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
use futures_util::SinkExt;
use futures_util::StreamExt;
use lofty::file::TaggedFileExt;
use reqwest::multipart;
use reqwest::Client;
use rockbox_library::entity::album::Album;
use rockbox_library::entity::track::Track;
use serde_json::json;
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::thread;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::{connect_async_tls_with_config, Connector};

const AUDIO_EXTENSIONS: [&str; 18] = [
    "mp3", "ogg", "flac", "m4a", "aac", "mp4", "alac", "wav", "wv", "mpc", "aiff", "aif", "ac3",
    "opus", "spx", "sid", "ape", "wma",
];

// const ROCKSKY_WS: &str = "ws://localhost:8000/ws";

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

pub async fn run_ws_session(token: String) -> Result<(), Error> {
    // Install the default crypto provider for rustls 0.23+. ring instead
    // of aws_lc_rs because aws-lc-sys's cmake cross-compile doesn't survive
    // cargo-ndk's flag injection on Android.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let rocksky_ws =
        env::var("ROCKSKY_WS").unwrap_or_else(|_| "wss://api.rocksky.app/ws".to_string());

    let mut root_store = rustls::RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let connector = Connector::Rustls(Arc::new(tls_config));

    let mut request = rocksky_ws.as_str().into_client_request()?;
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {}", token).parse()?);

    let (ws_stream, _) =
        match connect_async_tls_with_config(request, None, false, Some(connector)).await {
            Ok(stream) => stream,
            Err(e) => {
                if let WsError::Http(ref response) = e {
                    let status = response.status();
                    let body = response
                        .body()
                        .as_deref()
                        .and_then(|b| std::str::from_utf8(b).ok())
                        .unwrap_or("<empty body>");
                    tracing::error!("WebSocket connection failed: HTTP {} — {}", status, body);
                } else {
                    tracing::error!("WebSocket connection failed: {:?}", e);
                }
                return Err(e.into());
            }
        };
    tracing::info!("Connected to {}", rocksky_ws);

    let (mut write, mut read) = ws_stream.split();
    let device_id = Arc::new(Mutex::new(String::new()));

    let device_name = rocksky_device_name();
    tracing::info!("Registering as device \"{}\"", device_name);

    write
        .send(
            json!({
                "type": "register",
                "clientName": device_name,
                "token": token
            })
            .to_string()
            .into(),
        )
        .await?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(32);

    // Spawn track stream
    let tx_clone = tx.clone();
    tokio::spawn(start_track_stream(tx_clone));

    // Spawn status stream
    tokio::spawn(start_status_stream(tx.clone()));

    // Spawn sender
    {
        let device_id = device_id.clone();
        let token = token.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let id = device_id.lock().await.clone();
                if let Err(e) = write
                    .send(
                        json!({
                            "type": "message",
                            "data": serde_json::from_str::<Value>(&msg).unwrap(),
                            "device_id": id,
                            "token": token
                        })
                        .to_string()
                        .into(),
                    )
                    .await
                {
                    tracing::warn!("WebSocket send error: {}", e);
                    break;
                }
            }
        });
    }

    let host = env::var("ROCKBOX_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("ROCKBOX_PORT").unwrap_or_else(|_| "6061".to_string());
    let url = format!("tcp://{}:{}", host, port);

    // Retry gRPC connection up to 10 times with 1 second delay
    let mut client = None;
    for attempt in 1..=10 {
        match PlaybackServiceClient::connect(url.clone()).await {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(e) if attempt < 10 => {
                tracing::warn!(
                    "gRPC connection attempt {}/10 failed: {}. Retrying...",
                    attempt,
                    e
                );
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to connect to Rockbox gRPC server after 10 attempts: {}",
                    e
                ));
            }
        }
    }
    let mut client = client.unwrap();

    // Separate client for the playlist service — used to resume from saved state
    // after a daemon (re)start (see the "play" command handler below).
    let mut playlist_client = PlaylistServiceClient::connect(url.clone())
        .await
        .map_err(|e| anyhow!("Failed to connect to Rockbox playlist gRPC service: {}", e))?;

    while let Some(msg) = read.next().await {
        let msg = match msg {
            Ok(m) => m.to_string(),
            Err(e) => {
                tracing::error!("WebSocket read error: {:?}", e);
                return Err(anyhow!("WebSocket read error: {:?}", e));
            }
        };

        let msg: Value = serde_json::from_str(&msg)?;
        // Our device id comes ONLY from the registration reply, which the server
        // marks with `status: "registered"`. Do NOT read `deviceId` from any
        // message: the server also broadcasts `device_registered` events carrying
        // ANOTHER device's id whenever a new device (e.g. the web/mobile
        // miniplayer) joins. Capturing that would clobber our own id, so our
        // now-playing pushes would be tagged with the other device's id and every
        // miniplayer would mislabel the source.
        if msg["status"].as_str() == Some("registered") {
            if let Some(id) = msg["deviceId"].as_str() {
                *device_id.lock().await = id.to_string();
            }
        }

        // Ignore presence / primary-selection announcements about other
        // devices — they carry ANOTHER device's id and are informational for
        // the miniplayers, not actionable for a headless player. (A lone
        // player is auto-adopted as primary server-side, so we never need to
        // react to `primary_changed`.)
        match msg["type"].as_str() {
            Some("device_registered") | Some("device_unregistered") | Some("primary_changed") => {
                continue
            }
            _ => {}
        }

        if let Some("command") = msg["type"].as_str() {
            if let Some(cmd) = msg["action"].as_str() {
                match cmd {
                    "play" => {
                        // Decide between resume-from-pause and resume-from-saved
                        // state based on the engine's current status (a bitmask:
                        // PLAY=0x01, PAUSE=0x02 → 0 = stopped, 1 = playing,
                        // 3 = paused). On a fresh daemon start the engine is
                        // STOPPED, not paused, so a plain resume() is a no-op —
                        // we must resume_track() to restore the playlist from the
                        // control file and seek to the saved position (mirrors the
                        // GPUI play/pause handler).
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
                    "pause" => {
                        client
                            .pause(tonic::Request::new(PauseRequest::default()))
                            .await?;
                    }
                    "next" => {
                        client
                            .next(tonic::Request::new(NextRequest::default()))
                            .await?;
                    }
                    "previous" => {
                        client
                            .previous(tonic::Request::new(PreviousRequest::default()))
                            .await?;
                    }
                    "seek" => {
                        let pos = msg["args"]["position"].as_i64().unwrap_or(0);
                        client
                            .play(tonic::Request::new(PlayRequest {
                                offset: 0,
                                elapsed: pos,
                            }))
                            .await?;
                    }

                    _ => {
                        tracing::debug!("Unknown command: {}", cmd);
                    }
                };
            }
        }
    }

    Err(anyhow!("Connection closed"))
}

pub async fn start_track_stream(tx: tokio::sync::mpsc::Sender<String>) -> Result<(), Error> {
    let host = env::var("ROCKBOX_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("ROCKBOX_PORT").unwrap_or_else(|_| "6061".to_string());
    let url = format!("tcp://{}:{}", host, port);

    // Retry gRPC connection up to 10 times with 1 second delay
    let mut client = None;
    for attempt in 1..=10 {
        match PlaybackServiceClient::connect(url.clone()).await {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) if attempt < 10 => {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to connect to Rockbox gRPC server for track stream: {}",
                    e
                ));
            }
        }
    }
    let mut client = client.unwrap();
    let mut stream = client
        .stream_current_track(tonic::Request::new(StreamCurrentTrackRequest {}))
        .await?
        .into_inner();

    while let Some(Ok(track)) = stream.next().await {
        tx.send(
            json!({
                "type": "track",
                "title": track.title,
                "artist": track.artist,
                "album_artist": track.album_artist,
                "album": track.album,
                "length": track.length,
                "elapsed": track.elapsed,
                "track_number": track.tracknum,
                "disc_number": track.discnum,
                "composer": track.composer,
                "album_art": track.album_art
            })
            .to_string(),
        )
        .await?;
    }

    Ok(())
}

async fn start_status_stream(tx: tokio::sync::mpsc::Sender<String>) -> Result<(), Error> {
    let host = env::var("ROCKBOX_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("ROCKBOX_PORT").unwrap_or_else(|_| "6061".to_string());
    let url = format!("tcp://{}:{}", host, port);

    // Retry gRPC connection up to 10 times with 1 second delay
    let mut client = None;
    for attempt in 1..=10 {
        match PlaybackServiceClient::connect(url.clone()).await {
            Ok(c) => {
                client = Some(c);
                break;
            }
            Err(_) if attempt < 10 => {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                return Err(anyhow!(
                    "Failed to connect to Rockbox gRPC server for status stream: {}",
                    e
                ));
            }
        }
    }
    let mut client = client.unwrap();
    let mut stream = client
        .stream_status(tonic::Request::new(StreamStatusRequest {}))
        .await?
        .into_inner();

    while let Some(Ok(status)) = stream.next().await {
        tx.send(
            json!({
                "type": "status",
                "status": status.status
            })
            .to_string(),
        )
        .await?;
    }

    Ok(())
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

            loop {
                match run_ws_session(token.clone()).await {
                    Ok(_) => {
                        tracing::info!("WebSocket session ended cleanly");
                    }
                    Err(e) => {
                        tracing::error!("WebSocket session error: {:#?}", e);
                    }
                }

                tracing::info!("Reconnecting in {} seconds...", delay);
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
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
