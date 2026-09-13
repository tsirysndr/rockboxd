use std::{env, fs, pin::Pin};

use crate::{
    api::rockbox::v1alpha1::{playback_service_server::PlaybackService, *},
    check_and_load_player, read_files, rockbox_url, AUDIO_EXTENSIONS,
};
use rockbox_graphql::schema;
use rockbox_graphql::schema::objects::track::Track;
use rockbox_graphql::simplebroker::SimpleBroker;
use rockbox_library::repo;
use rockbox_sys::{self as rb, types::audio_status::AudioStatus};
use sqlx::Sqlite;
use tokio_stream::{Stream, StreamExt};

pub struct Playback {
    client: reqwest::Client,
    pool: sqlx::Pool<Sqlite>,
}

impl Playback {
    pub fn new(client: reqwest::Client, pool: sqlx::Pool<Sqlite>) -> Self {
        Self { client, pool }
    }
}

/// Open a gRPC PlaybackServiceClient against the broadcaster of the
/// currently-playing HLS/DASH stream. Returns `None` when no HLS session
/// is active, in which case the caller should fall through to its local
/// (Rockbox playback engine) code path.
async fn remote_playback_client(
) -> Option<playback_service_client::PlaybackServiceClient<tonic::transport::Channel>> {
    let base = rockbox_hls::player_remote_api_base()?;
    match playback_service_client::PlaybackServiceClient::connect(base.clone()).await {
        Ok(c) => Some(c),
        Err(e) => {
            tracing::warn!("hls remote control: connect {base} failed: {e}");
            None
        }
    }
}

#[tonic::async_trait]
impl PlaybackService for Playback {
    async fn play(
        &self,
        request: tonic::Request<PlayRequest>,
    ) -> Result<tonic::Response<PlayResponse>, tonic::Status> {
        let params = request.into_inner();
        let elapsed = params.elapsed;
        let offset = params.offset;
        tokio::task::spawn_blocking(move || {
            rb::with_kernel_lock(|| rb::playback::play(elapsed, offset));
        })
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(PlayResponse::default()))
    }

    async fn pause(
        &self,
        _request: tonic::Request<PauseRequest>,
    ) -> Result<tonic::Response<PauseResponse>, tonic::Status> {
        // HLS/DASH pause is a local sink-side action — it just stops pushing
        // PCM. The broadcaster keeps streaming and the consumer's segment
        // buffer fills up; resume picks up at the live edge again.
        if rockbox_hls::player_is_active() {
            rockbox_hls::player_pause();
            return Ok(tonic::Response::new(PauseResponse::default()));
        }
        self.client
            .put(&format!("{}/player/pause", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(PauseResponse::default()))
    }

    async fn play_or_pause(
        &self,
        _request: tonic::Request<PlayOrPauseRequest>,
    ) -> Result<tonic::Response<PlayOrPauseResponse>, tonic::Status> {
        // HLS/DASH session: toggle local sink-side pause based on the
        // standalone player's own state.
        if rockbox_hls::player_is_active() {
            let s = rockbox_hls::player_status_json();
            if s.contains("\"state\":\"paused\"") {
                rockbox_hls::player_resume();
            } else {
                rockbox_hls::player_pause();
            }
            return Ok(tonic::Response::new(PlayOrPauseResponse::default()));
        }
        let client = reqwest::Client::new();
        let response = client
            .get(&format!("{}/player/status", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .json::<AudioStatus>()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let client = reqwest::Client::new();
        match response.status {
            1 => {
                client
                    .put(&format!("{}/player/pause", rockbox_url()))
                    .send()
                    .await
                    .map_err(|e| tonic::Status::internal(e.to_string()))?;
            }
            3 => {
                client
                    .put(&format!("{}/player/resume", rockbox_url()))
                    .send()
                    .await
                    .map_err(|e| tonic::Status::internal(e.to_string()))?;
            }
            _ => {
                let status = rb::system::get_global_status();
                if status.resume_index > -1 {
                    tokio::task::spawn_blocking(move || {
                        rb::with_kernel_lock(|| {
                            if rb::playlist::amount() == 0 {
                                let ret = rb::playlist::resume();
                                if ret == -1 {
                                    return;
                                }
                            }
                            rb::playlist::resume_track(
                                status.resume_index,
                                status.resume_crc32,
                                status.resume_elapsed.into(),
                                status.resume_offset.into(),
                            );
                        });
                    })
                    .await
                    .map_err(|e| tonic::Status::internal(e.to_string()))?;
                }
            }
        };
        Ok(tonic::Response::new(PlayOrPauseResponse::default()))
    }

    async fn resume(
        &self,
        _request: tonic::Request<ResumeRequest>,
    ) -> Result<tonic::Response<ResumeResponse>, tonic::Status> {
        if rockbox_hls::player_is_active() {
            rockbox_hls::player_resume();
            return Ok(tonic::Response::new(ResumeResponse::default()));
        }
        self.client
            .put(&format!("{}/player/resume", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(ResumeResponse::default()))
    }

    async fn next(
        &self,
        _request: tonic::Request<NextRequest>,
    ) -> Result<tonic::Response<NextResponse>, tonic::Status> {
        // HLS/DASH: forward to the *broadcaster's* PlaybackService over gRPC.
        // The broadcaster advances its own playlist, its CMAF output reflects
        // the new track, and the consumer's segment refresher picks it up.
        if let Some(mut client) = remote_playback_client().await {
            return client.next(NextRequest::default()).await;
        }
        self.client
            .put(&format!("{}/player/next", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(NextResponse::default()))
    }

    async fn previous(
        &self,
        _request: tonic::Request<PreviousRequest>,
    ) -> Result<tonic::Response<PreviousResponse>, tonic::Status> {
        if let Some(mut client) = remote_playback_client().await {
            return client.previous(PreviousRequest::default()).await;
        }
        self.client
            .put(&format!("{}/player/previous", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(PreviousResponse::default()))
    }

    async fn fast_forward_rewind(
        &self,
        request: tonic::Request<FastForwardRewindRequest>,
    ) -> Result<tonic::Response<FastForwardRewindResponse>, tonic::Status> {
        let params = request.into_inner();
        let newtime = params.new_time;
        // The consumer can't seek a sliding HLS window — only the
        // broadcaster can move its playhead. Forward over gRPC.
        if let Some(mut client) = remote_playback_client().await {
            return client
                .fast_forward_rewind(FastForwardRewindRequest { new_time: newtime })
                .await;
        }
        tokio::task::spawn_blocking(move || {
            rb::with_kernel_lock(|| rb::playback::ff_rewind(newtime));
        })
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(FastForwardRewindResponse::default()))
    }

    async fn status(
        &self,
        _request: tonic::Request<StatusRequest>,
    ) -> Result<tonic::Response<StatusResponse>, tonic::Status> {
        if let Some(mut client) = remote_playback_client().await {
            return client.status(StatusRequest::default()).await;
        }
        let response = self
            .client
            .get(&format!("{}/player/status", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .json::<AudioStatus>()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(StatusResponse {
            status: response.status,
        }))
    }

    async fn current_track(
        &self,
        _request: tonic::Request<CurrentTrackRequest>,
    ) -> Result<tonic::Response<CurrentTrackResponse>, tonic::Status> {
        // HLS/DASH: ask the broadcaster directly. The local library DB has
        // no record of the broadcaster's track, so we skip the local
        // metadata enrichment and pass the response straight through.
        if let Some(mut client) = remote_playback_client().await {
            return client.current_track(CurrentTrackRequest::default()).await;
        }
        let track = self
            .client
            .get(&format!("{}/player/current-track", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .json::<Option<rb::types::mp3_entry::Mp3Entry>>()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(track) = track.as_ref() {
            let hash = format!("{:x}", md5::compute(track.path.as_bytes()));
            let metadata = repo::track::find_by_md5(self.pool.clone(), &hash)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
            if let Some(metadata) = metadata {
                let mut track = track.clone();
                // Only override album_art if the DB has a non-None value; the HTTP
                // endpoint already resolved album_art via find_internal_track_by_url,
                // so we must not overwrite it with None from the remote-saved record.
                if metadata.album_art.is_some() {
                    track.album_art = metadata.album_art;
                }
                track.album_id = Some(metadata.album_id);
                track.artist_id = Some(metadata.artist_id);
                return Ok(tonic::Response::new(track.into()));
            }
        }

        Ok(tonic::Response::new(track.into()))
    }

    async fn next_track(
        &self,
        _request: tonic::Request<NextTrackRequest>,
    ) -> Result<tonic::Response<NextTrackResponse>, tonic::Status> {
        if let Some(mut client) = remote_playback_client().await {
            return client.next_track(NextTrackRequest::default()).await;
        }
        let track = self
            .client
            .get(&format!("{}/player/next-track", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .json::<Option<rb::types::mp3_entry::Mp3Entry>>()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(track.into()))
    }

    async fn flush_and_reload_tracks(
        &self,
        _request: tonic::Request<FlushAndReloadTracksRequest>,
    ) -> Result<tonic::Response<FlushAndReloadTracksResponse>, tonic::Status> {
        tokio::task::spawn_blocking(move || {
            rb::with_kernel_lock(|| rb::playback::flush_and_reload_tracks());
        })
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(FlushAndReloadTracksResponse::default()))
    }

    async fn get_file_position(
        &self,
        _request: tonic::Request<GetFilePositionRequest>,
    ) -> Result<tonic::Response<GetFilePositionResponse>, tonic::Status> {
        let position = self
            .client
            .get(&format!("{}/player/file-position", rockbox_url()))
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .json::<rb::types::file_position::FilePosition>()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?
            .position;
        Ok(tonic::Response::new(GetFilePositionResponse { position }))
    }

    async fn hard_stop(
        &self,
        _request: tonic::Request<HardStopRequest>,
    ) -> Result<tonic::Response<HardStopResponse>, tonic::Status> {
        // Local: tear down the HLS/DASH player if one is active, so the
        // consumer stops decoding and pushing PCM. Don't propagate to the
        // broadcaster — other consumers may still be listening.
        let was_hls = rockbox_hls::player_stop();
        if was_hls {
            return Ok(tonic::Response::new(HardStopResponse::default()));
        }
        tokio::task::spawn_blocking(move || {
            rb::with_kernel_lock(|| rb::playback::hard_stop());
        })
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(HardStopResponse::default()))
    }

    async fn play_album(
        &self,
        request: tonic::Request<PlayAlbumRequest>,
    ) -> Result<tonic::Response<PlayAlbumResponse>, tonic::Status> {
        let request = request.into_inner();
        let album_id = request.album_id;
        let shuffle = request.shuffle;
        let position = request.position;
        let tracks = repo::album_tracks::find_by_album(self.pool.clone(), &album_id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let tracks = tracks.into_iter().map(|t| t.path).collect::<Vec<String>>();
        let body = serde_json::json!({
            "tracks": tracks,
        });

        let response = PlayAlbumResponse::default();
        check_and_load_player!(response, tracks, shuffle.unwrap_or_default());

        let url = format!("{}/playlists", rockbox_url());
        let client = reqwest::Client::new();
        client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(true) = shuffle {
            let url = format!("{}/playlists/shuffle", rockbox_url());
            let client = reqwest::Client::new();
            client
                .put(&url)
                .send()
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let url = match position {
            Some(position) => format!("{}/playlists/start?start_index={}", rockbox_url(), position),
            None => format!("{}/playlists/start", rockbox_url()),
        };

        let client = reqwest::Client::new();
        client
            .put(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(PlayAlbumResponse::default()))
    }

    async fn play_artist_tracks(
        &self,
        request: tonic::Request<PlayArtistTracksRequest>,
    ) -> Result<tonic::Response<PlayArtistTracksResponse>, tonic::Status> {
        let request = request.into_inner();
        let artist_id = request.artist_id;
        let shuffle = request.shuffle;
        let position = request.position;
        let tracks = repo::artist_tracks::find_by_artist(self.pool.clone(), &artist_id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let tracks = tracks.into_iter().map(|t| t.path).collect::<Vec<String>>();
        let body = serde_json::json!({
            "tracks": tracks,
        });

        let response = PlayArtistTracksResponse::default();
        check_and_load_player!(response, tracks, shuffle.unwrap_or_default());

        let url = format!("{}/playlists", rockbox_url());
        let client = reqwest::Client::new();
        client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(true) = shuffle {
            let url = format!("{}/playlists/shuffle", rockbox_url());
            let client = reqwest::Client::new();
            client
                .put(&url)
                .send()
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let url = match position {
            Some(position) => format!("{}/playlists/start?start_index={}", rockbox_url(), position),
            None => format!("{}/playlists/start", rockbox_url()),
        };

        let client = reqwest::Client::new();
        client
            .put(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(PlayArtistTracksResponse::default()))
    }

    async fn play_playlist(
        &self,
        _request: tonic::Request<PlayPlaylistRequest>,
    ) -> Result<tonic::Response<PlayPlaylistResponse>, tonic::Status> {
        todo!()
    }

    async fn play_directory(
        &self,
        request: tonic::Request<PlayDirectoryRequest>,
    ) -> Result<tonic::Response<PlayDirectoryResponse>, tonic::Status> {
        let request = request.into_inner();
        let path = request.path.trim().to_string();
        let recurse = request.recurse;
        let shuffle = request.shuffle;
        let position = request.position;
        let mut tracks: Vec<String> = vec![];

        let recurse = match position {
            Some(_) => Some(false),
            None => recurse,
        };

        if path.starts_with("upnp://") {
            tracks = crate::read_upnp_files(path)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        } else {
            if !std::path::Path::new(&path).is_dir() {
                return Err(tonic::Status::invalid_argument("Path is not a directory"));
            }
            match recurse {
                Some(true) => {
                    tracks = read_files(path)
                        .await
                        .map_err(|e| tonic::Status::internal(e.to_string()))?
                }
                _ => {
                    for file in
                        fs::read_dir(&path).map_err(|e| tonic::Status::internal(e.to_string()))?
                    {
                        let file = file.map_err(|e| tonic::Status::internal(e.to_string()))?;

                        if file
                            .metadata()
                            .map_err(|e| tonic::Status::internal(e.to_string()))?
                            .is_file()
                            && !AUDIO_EXTENSIONS.iter().any(|ext| {
                                file.path()
                                    .to_string_lossy()
                                    .ends_with(&format!(".{}", ext))
                            })
                        {
                            continue;
                        }

                        tracks.push(file.path().to_string_lossy().to_string());
                    }
                }
            }
            tracks.sort();
        }

        let body = serde_json::json!({
            "tracks": tracks
        });

        let response = PlayDirectoryResponse::default();
        check_and_load_player!(response, tracks, shuffle.unwrap_or_default());

        let url = format!("{}/playlists", rockbox_url());
        let client = reqwest::Client::new();
        client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(true) = shuffle {
            let url = format!("{}/playlists/shuffle", rockbox_url());
            let client = reqwest::Client::new();
            client
                .put(&url)
                .send()
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let url = match position {
            Some(position) => format!("{}/playlists/start?start_index={}", rockbox_url(), position),
            None => format!("{}/playlists/start", rockbox_url()),
        };

        let client = reqwest::Client::new();
        client
            .put(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(PlayDirectoryResponse::default()))
    }

    async fn play_music_directory(
        &self,
        request: tonic::Request<PlayMusicDirectoryRequest>,
    ) -> Result<tonic::Response<PlayMusicDirectoryResponse>, tonic::Status> {
        let request = request.into_inner();
        let path = format!("{}/Music", env::var("HOME").unwrap());
        let recurse = request.recurse;
        let shuffle = request.shuffle;
        let position = request.position;
        let mut tracks: Vec<String> = vec![];

        let recurse = match position {
            Some(_) => Some(false),
            None => recurse,
        };

        if !std::path::Path::new(&path).is_dir() {
            return Err(tonic::Status::invalid_argument("Path is not a directory"));
        }

        match recurse {
            Some(true) => {
                tracks = read_files(path)
                    .await
                    .map_err(|e| tonic::Status::internal(e.to_string()))?
            }
            _ => {
                for file in
                    fs::read_dir(&path).map_err(|e| tonic::Status::internal(e.to_string()))?
                {
                    let file = file.map_err(|e| tonic::Status::internal(e.to_string()))?;

                    if file
                        .metadata()
                        .map_err(|e| tonic::Status::internal(e.to_string()))?
                        .is_file()
                        && !AUDIO_EXTENSIONS.iter().any(|ext| {
                            file.path()
                                .to_string_lossy()
                                .ends_with(&format!(".{}", ext))
                        })
                    {
                        continue;
                    }

                    tracks.push(file.path().to_string_lossy().to_string());
                }
            }
        }

        tracks.sort();

        let body = serde_json::json!({
            "tracks": tracks
        });

        let response = PlayMusicDirectoryResponse::default();
        check_and_load_player!(response, tracks, shuffle.unwrap_or_default());

        let url = format!("{}/playlists", rockbox_url());
        let client = reqwest::Client::new();
        client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(true) = shuffle {
            let url = format!("{}/playlists/shuffle", rockbox_url());
            let client = reqwest::Client::new();
            client
                .put(&url)
                .send()
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let url = match position {
            Some(position) => format!("{}/playlists/start?start_index={}", rockbox_url(), position),
            None => format!("{}/playlists/start", rockbox_url()),
        };

        let client = reqwest::Client::new();
        client
            .put(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(PlayMusicDirectoryResponse::default()))
    }

    async fn play_track(
        &self,
        request: tonic::Request<PlayTrackRequest>,
    ) -> Result<tonic::Response<PlayTrackResponse>, tonic::Status> {
        let request = request.into_inner();
        let raw = request.path.replace("file://", "");
        let raw = raw.trim();
        let path = raw.split('#').next().unwrap_or(raw).to_string();

        // HLS / DASH URLs bypass Rockbox's playback engine entirely and run
        // through the standalone player in `crates/hls/`.  The decoded PCM
        // goes straight into whatever PCM sink the user has configured
        // (cpal / AirPlay / Snapcast / CMAF / …), so this composes cleanly
        // with the rest of the audio-output graph.
        if rockbox_hls::is_hls_or_dash_url(&path).is_some() {
            if let Err(e) = rockbox_hls::player_play(&path) {
                return Err(tonic::Status::invalid_argument(e));
            }
            return Ok(tonic::Response::new(PlayTrackResponse::default()));
        }

        let tracks = vec![path.clone()];

        let body = serde_json::json!({
            "tracks": tracks,
        });

        let response = PlayTrackResponse::default();
        check_and_load_player!(response, tracks, false);

        let url = format!("{}/playlists", rockbox_url());
        self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        let client = reqwest::Client::new();
        let url = format!("{}/playlists/start", rockbox_url());
        client
            .put(&url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(PlayTrackResponse::default()))
    }

    async fn play_liked_tracks(
        &self,
        request: tonic::Request<PlayLikedTracksRequest>,
    ) -> Result<tonic::Response<PlayLikedTracksResponse>, tonic::Status> {
        let request = request.into_inner();
        let shuffle = request.shuffle;
        let position = request.position;
        let tracks = repo::favourites::all_tracks(self.pool.clone())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let tracks = tracks.into_iter().map(|t| t.path).collect::<Vec<String>>();
        let body = serde_json::json!({
            "tracks": tracks,
        });

        let response = PlayLikedTracksResponse::default();
        check_and_load_player!(response, tracks, shuffle.unwrap_or_default());

        let url = format!("{}/playlists", rockbox_url());
        let client = reqwest::Client::new();
        client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(true) = shuffle {
            let url = format!("{}/playlists/shuffle", rockbox_url());
            let client = reqwest::Client::new();
            client
                .put(&url)
                .send()
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let url = match position {
            Some(position) => format!("{}/playlists/start?start_index={}", rockbox_url(), position),
            None => format!("{}/playlists/start", rockbox_url()),
        };

        let client = reqwest::Client::new();
        client
            .put(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(PlayLikedTracksResponse::default()))
    }

    async fn play_all_tracks(
        &self,
        request: tonic::Request<PlayAllTracksRequest>,
    ) -> Result<tonic::Response<PlayAllTracksResponse>, tonic::Status> {
        let request = request.into_inner();
        let shuffle = request.shuffle;
        let position = request.position;
        let tracks = repo::track::all(self.pool.clone())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let tracks = tracks.into_iter().map(|t| t.path).collect::<Vec<String>>();
        let body = serde_json::json!({
            "tracks": tracks,
        });

        let response = PlayAllTracksResponse::default();
        check_and_load_player!(response, tracks, shuffle.unwrap_or_default());

        let url = format!("{}/playlists", rockbox_url());
        let client = reqwest::Client::new();
        client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(true) = shuffle {
            let url = format!("{}/playlists/shuffle", rockbox_url());
            let client = reqwest::Client::new();
            client
                .put(&url)
                .send()
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?;
        }

        let url = match position {
            Some(position) => format!("{}/playlists/start?start_index={}", rockbox_url(), position),
            None => format!("{}/playlists/start", rockbox_url()),
        };

        let client = reqwest::Client::new();
        client
            .put(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        Ok(tonic::Response::new(PlayAllTracksResponse::default()))
    }

    type StreamCurrentTrackStream = Pin<
        Box<dyn Stream<Item = Result<CurrentTrackResponse, tonic::Status>> + Send + Sync + 'static>,
    >;

    async fn stream_current_track(
        &self,
        _request: tonic::Request<StreamCurrentTrackRequest>,
    ) -> Result<tonic::Response<Self::StreamCurrentTrackStream>, tonic::Status> {
        let mut stream = SimpleBroker::<Track>::subscribe();
        let output = async_stream::try_stream! {
            while let Some(track) = stream.next().await {
                yield track.into();
            }
        };

        Ok(tonic::Response::new(
            Box::pin(output) as Self::StreamCurrentTrackStream
        ))
    }

    type StreamStatusStream =
        Pin<Box<dyn Stream<Item = Result<StatusResponse, tonic::Status>> + Send + Sync + 'static>>;

    async fn stream_status(
        &self,
        _request: tonic::Request<StreamStatusRequest>,
    ) -> Result<tonic::Response<Self::StreamStatusStream>, tonic::Status> {
        let mut stream = SimpleBroker::<schema::objects::audio_status::AudioStatus>::subscribe();
        let output = async_stream::try_stream! {
            while let Some(status) = stream.next().await {
                yield status.into();
            }
        };

        Ok(tonic::Response::new(
            Box::pin(output) as Self::StreamStatusStream
        ))
    }

    type StreamPlaylistStream = Pin<
        Box<dyn Stream<Item = Result<PlaylistResponse, tonic::Status>> + Send + Sync + 'static>,
    >;

    async fn stream_playlist(
        &self,
        _request: tonic::Request<StreamPlaylistRequest>,
    ) -> Result<tonic::Response<Self::StreamPlaylistStream>, tonic::Status> {
        let mut stream = SimpleBroker::<schema::objects::playlist::Playlist>::subscribe();
        let output = async_stream::try_stream! {
            while let Some(playlist) = stream.next().await {
                yield PlaylistResponse {
                    index: playlist.index,
                    amount: playlist.amount,
                    tracks: playlist.tracks.into_iter().map(|t| t.into()).collect(),
                };
            }
        };
        Ok(tonic::Response::new(
            Box::pin(output) as Self::StreamPlaylistStream
        ))
    }

    type StreamLevelsStream =
        Pin<Box<dyn Stream<Item = Result<Levels, tonic::Status>> + Send + Sync + 'static>>;

    /// Output levels for a meter, polled rather than brokered: levels are a
    /// sampled quantity, not an event, and a message per output buffer would
    /// be several hundred a second to draw a bar that redraws at frame rate.
    async fn stream_levels(
        &self,
        _request: tonic::Request<StreamLevelsRequest>,
    ) -> Result<tonic::Response<Self::StreamLevelsStream>, tonic::Status> {
        // 20 Hz — fast enough that a kick still reads as a kick once the
        // client applies attack/release, cheap enough to leave running.
        const PERIOD: std::time::Duration = std::time::Duration::from_millis(50);

        let output = async_stream::try_stream! {
            let mut ticker = tokio::time::interval(PERIOD);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                // A plain read of the four words the audio path publishes. It
                // touches no firmware state, so unlike the mutating handlers
                // it needs no `fw_bus::run_on_broker` hop.
                let levels = rb::sound::pcm::levels();
                yield Levels {
                    left: levels.left,
                    right: levels.right,
                    low_left: levels.low_left,
                    low_right: levels.low_right,
                    bands: levels.bands.to_vec(),
                };
            }
        };

        Ok(tonic::Response::new(
            Box::pin(output) as Self::StreamLevelsStream
        ))
    }
}
