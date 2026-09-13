use std::pin::Pin;

use rockbox_graphql::{simplebroker::SimpleBroker, types::ScanCompleted};
use rockbox_library::{entity::favourites::Favourites, repo};
use rockbox_playlists::{resolver, rules::RuleCriteria, PlaylistStore};
use sqlx::Sqlite;
use tokio_stream::{Stream, StreamExt};

use crate::{
    api::rockbox::v1alpha1::{
        library_service_server::LibraryService, Album, AnalyticsPageRequest, Artist,
        FilterAlbumsRequest, FilterAlbumsResponse, FilterArtistsRequest, FilterArtistsResponse,
        FilterTracksRequest, FilterTracksResponse, GetAlbumRequest, GetAlbumResponse,
        GetAlbumsRequest, GetAlbumsResponse, GetArtistRequest, GetArtistResponse,
        GetArtistsRequest, GetArtistsResponse, GetLikedAlbumsRequest, GetLikedAlbumsResponse,
        GetLikedTracksRequest, GetLikedTracksResponse, GetTrackRequest, GetTrackResponse,
        GetTracksRequest, GetTracksResponse, LikeAlbumRequest, LikeAlbumResponse, LikeTrackRequest,
        LikeTrackResponse, PlayHistoryEntry, PlayHistoryResponse, ScanLibraryRequest,
        ScanLibraryResponse, SearchPlaylist, SearchRequest, SearchResponse, StreamLibraryRequest,
        StreamLibraryResponse, TrackStat, TrackStatListResponse, UnlikeAlbumRequest,
        UnlikeAlbumResponse, UnlikeTrackRequest, UnlikeTrackResponse,
    },
    rockbox_url,
};

pub struct Library {
    pool: sqlx::Pool<Sqlite>,
    client: reqwest::Client,
    store: PlaylistStore,
}

impl Library {
    pub fn new(pool: sqlx::Pool<Sqlite>, client: reqwest::Client) -> Self {
        let store = PlaylistStore::new(pool.clone());
        Self {
            pool,
            client,
            store,
        }
    }
}

fn page_limit(page: &AnalyticsPageRequest) -> i64 {
    i64::from(page.limit.unwrap_or(100)).clamp(1, 1000)
}

fn page_offset(page: &AnalyticsPageRequest) -> i64 {
    i64::from(page.offset.unwrap_or(0)).max(0)
}

impl Library {
    /// A count-bearing analytics view (most played / most skipped) as a
    /// `TrackStatListResponse`.
    async fn stat_view(
        &self,
        sql: &str,
        page: AnalyticsPageRequest,
    ) -> Result<tonic::Response<TrackStatListResponse>, tonic::Status> {
        use sqlx::Row;
        let rows = sqlx::query(sql)
            .bind(page_limit(&page))
            .bind(page_offset(&page))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(TrackStatListResponse {
            stats: rows
                .into_iter()
                .map(|r| TrackStat {
                    track_id: r.get(0),
                    title: r.get(1),
                    artist: r.get(2),
                    album: r.get(3),
                    count: r.get(4),
                    at: r.get(5),
                    created_at: None,
                })
                .collect(),
        }))
    }

    /// A created_at-bearing view (never played / recently added).
    async fn added_view(
        &self,
        sql: &str,
        page: AnalyticsPageRequest,
    ) -> Result<tonic::Response<TrackStatListResponse>, tonic::Status> {
        use sqlx::Row;
        let rows = sqlx::query(sql)
            .bind(page_limit(&page))
            .bind(page_offset(&page))
            .fetch_all(&self.pool)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(TrackStatListResponse {
            stats: rows
                .into_iter()
                .map(|r| TrackStat {
                    track_id: r.get(0),
                    title: r.get(1),
                    artist: r.get(2),
                    album: r.get(3),
                    count: 0,
                    at: None,
                    created_at: r.get(4),
                })
                .collect(),
        }))
    }
}

/// Mirror a like to the user's Rocksky account. Runs detached from the RPC —
/// the local favourite is already committed, so a failure here costs the remote
/// scrobble, not the like itself.
async fn sync_like_to_rocksky(
    pool: sqlx::Pool<Sqlite>,
    track_id: &str,
) -> Result<(), anyhow::Error> {
    let Some(track) = repo::track::find(pool.clone(), track_id).await? else {
        return Ok(());
    };
    let Some(album) = repo::album::find(pool, &track.album_id).await? else {
        return Ok(());
    };
    rockbox_rocksky::like(track, album).await
}

/// Counterpart to [`sync_like_to_rocksky`].
async fn sync_unlike_to_rocksky(
    pool: sqlx::Pool<Sqlite>,
    track_id: &str,
) -> Result<(), anyhow::Error> {
    let Some(track) = repo::track::find(pool, track_id).await? else {
        return Ok(());
    };
    rockbox_rocksky::unlike(track).await
}

#[tonic::async_trait]
impl LibraryService for Library {
    async fn get_albums(
        &self,
        _request: tonic::Request<GetAlbumsRequest>,
    ) -> Result<tonic::Response<GetAlbumsResponse>, tonic::Status> {
        let albums = repo::album::all(self.pool.clone())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(GetAlbumsResponse {
            albums: albums.into_iter().map(|a| a.into()).collect(),
        }))
    }

    async fn get_artists(
        &self,
        _request: tonic::Request<GetArtistsRequest>,
    ) -> Result<tonic::Response<GetArtistsResponse>, tonic::Status> {
        let artists = repo::artist::all(self.pool.clone())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(GetArtistsResponse {
            artists: artists.into_iter().map(|a| a.into()).collect(),
        }))
    }

    async fn get_tracks(
        &self,
        _request: tonic::Request<GetTracksRequest>,
    ) -> Result<tonic::Response<GetTracksResponse>, tonic::Status> {
        let tracks = repo::track::all(self.pool.clone())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(GetTracksResponse {
            tracks: tracks.into_iter().map(|t| t.into()).collect(),
        }))
    }

    async fn get_album(
        &self,
        request: tonic::Request<GetAlbumRequest>,
    ) -> Result<tonic::Response<GetAlbumResponse>, tonic::Status> {
        let params = request.into_inner();
        let album = repo::album::find(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let mut album: Option<Album> = album.map(|a| a.into());
        let tracks = repo::album_tracks::find_by_album(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(album) = album.as_mut() {
            album.tracks = tracks.into_iter().map(|t| t.into()).collect();
        }

        let response = GetAlbumResponse { album };
        Ok(tonic::Response::new(response))
    }

    async fn get_artist(
        &self,
        request: tonic::Request<GetArtistRequest>,
    ) -> Result<tonic::Response<GetArtistResponse>, tonic::Status> {
        let params = request.into_inner();
        let artist = repo::artist::find(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let mut artist: Option<Artist> = artist.map(|a| a.into());
        let albums = repo::album::find_by_artist(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        let tracks = repo::artist_tracks::find_by_artist(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        if let Some(artist) = artist.as_mut() {
            artist.albums = albums.into_iter().map(|a| a.into()).collect();
            artist.tracks = tracks.into_iter().map(|t| t.into()).collect();
        }

        Ok(tonic::Response::new(GetArtistResponse { artist }))
    }

    async fn get_track(
        &self,
        request: tonic::Request<GetTrackRequest>,
    ) -> Result<tonic::Response<GetTrackResponse>, tonic::Status> {
        let params = request.into_inner();
        let track = repo::track::find(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(GetTrackResponse {
            track: track.map(|t| t.into()),
        }))
    }

    async fn like_track(
        &self,
        request: tonic::Request<LikeTrackRequest>,
    ) -> Result<tonic::Response<LikeTrackResponse>, tonic::Status> {
        let params = request.into_inner();
        repo::favourites::save(
            self.pool.clone(),
            Favourites {
                id: cuid::cuid1().map_err(|e| tonic::Status::internal(e.to_string()))?,
                track_id: Some(params.id.clone()),
                created_at: chrono::Utc::now(),
                album_id: None,
            },
        )
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;

        // The local favourite is already committed above, which is all
        // `GetLikedTracks` reads — so answer now and mirror to Rocksky in the
        // background. That call uploads the album cover and POSTs to
        // api.rocksky.app with no timeout; awaiting it here made every client's
        // heart icon hang on the network round-trip.
        let pool = self.pool.clone();
        let id = params.id.clone();
        tokio::spawn(async move {
            if let Err(e) = sync_like_to_rocksky(pool, &id).await {
                tracing::warn!("rocksky like sync failed for {id}: {e}");
            }
        });

        Ok(tonic::Response::new(LikeTrackResponse {}))
    }

    async fn like_album(
        &self,
        request: tonic::Request<LikeAlbumRequest>,
    ) -> Result<tonic::Response<LikeAlbumResponse>, tonic::Status> {
        let params = request.into_inner();
        repo::favourites::save(
            self.pool.clone(),
            Favourites {
                id: cuid::cuid1().map_err(|e| tonic::Status::internal(e.to_string()))?,
                track_id: None,
                created_at: chrono::Utc::now(),
                album_id: Some(params.id),
            },
        )
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(LikeAlbumResponse {}))
    }

    async fn unlike_track(
        &self,
        request: tonic::Request<UnlikeTrackRequest>,
    ) -> Result<tonic::Response<UnlikeTrackResponse>, tonic::Status> {
        let params = request.into_inner();
        repo::favourites::delete(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;

        // Answer as soon as the local favourite is gone; mirror to Rocksky in
        // the background. See `like_track` for why.
        let pool = self.pool.clone();
        let id = params.id.clone();
        tokio::spawn(async move {
            if let Err(e) = sync_unlike_to_rocksky(pool, &id).await {
                tracing::warn!("rocksky unlike sync failed for {id}: {e}");
            }
        });

        Ok(tonic::Response::new(UnlikeTrackResponse {}))
    }

    async fn unlike_album(
        &self,
        request: tonic::Request<UnlikeAlbumRequest>,
    ) -> Result<tonic::Response<UnlikeAlbumResponse>, tonic::Status> {
        let params = request.into_inner();
        repo::favourites::delete(self.pool.clone(), &params.id)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(UnlikeAlbumResponse {}))
    }

    async fn get_liked_tracks(
        &self,
        _request: tonic::Request<GetLikedTracksRequest>,
    ) -> Result<tonic::Response<GetLikedTracksResponse>, tonic::Status> {
        let tracks = repo::favourites::all_tracks(self.pool.clone())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(GetLikedTracksResponse {
            tracks: tracks.into_iter().map(|t| t.into()).collect(),
        }))
    }

    async fn get_liked_albums(
        &self,
        _request: tonic::Request<GetLikedAlbumsRequest>,
    ) -> Result<tonic::Response<GetLikedAlbumsResponse>, tonic::Status> {
        let albums = repo::favourites::all_albums(self.pool.clone())
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(GetLikedAlbumsResponse {
            albums: albums.into_iter().map(|a| a.into()).collect(),
        }))
    }

    async fn scan_library(
        &self,
        request: tonic::Request<ScanLibraryRequest>,
    ) -> Result<tonic::Response<ScanLibraryResponse>, tonic::Status> {
        let request = request.into_inner();
        let params = match (request.path, request.rebuild_index) {
            (Some(path), Some(true)) => format!("?path={}&rebuild_index=1", path),
            (Some(path), Some(false)) => format!("?path={}", path),
            (None, Some(true)) => format!("?rebuild_index=1"),
            (None, Some(false)) => "".to_string(),
            (Some(path), None) => format!("?path={}", path),
            (None, None) => "".to_string(),
        };
        let url = format!("{}/scan-library{}", rockbox_url(), params);
        self.client
            .put(&url)
            .send()
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(ScanLibraryResponse {}))
    }

    async fn search(
        &self,
        request: tonic::Request<SearchRequest>,
    ) -> Result<tonic::Response<SearchResponse>, tonic::Status> {
        let request = request.into_inner();
        let term = request.term;

        #[cfg(not(feature = "fts5"))]
        let (tracks, albums, artists, playlists) = {
            use rockbox_typesense::client::{
                search_albums, search_artists, search_playlists, search_tracks,
            };
            let tracks = search_tracks(&term)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let albums = search_albums(&term)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let artists = search_artists(&term)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let playlists = search_playlists(&term)
                .await
                .unwrap_or_default()
                .map(|r| {
                    r.hits
                        .into_iter()
                        .map(|h| SearchPlaylist {
                            id: h.document.id,
                            name: h.document.name,
                            description: h.document.description,
                            image: h.document.image,
                            is_smart: h.document.is_smart,
                            track_count: h.document.track_count,
                        })
                        .collect()
                })
                .unwrap_or_default();
            (tracks, albums, artists, playlists)
        };

        #[cfg(feature = "fts5")]
        let (tracks, albums, artists, playlists) = {
            use rockbox_fts5::{search_albums, search_artists, search_playlists, search_tracks};
            let tracks = search_tracks(self.pool.clone(), &term)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let albums = search_albums(self.pool.clone(), &term)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let artists = search_artists(self.pool.clone(), &term)
                .await
                .map_err(|e| tonic::Status::internal(e.to_string()))?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let playlists = search_playlists(self.pool.clone(), &term)
                .await
                .unwrap_or_default()
                .map(|r| {
                    r.hits
                        .into_iter()
                        .map(|h| SearchPlaylist {
                            id: h.document.id,
                            name: h.document.name,
                            description: h.document.description,
                            image: h.document.image,
                            is_smart: h.document.is_smart,
                            track_count: h.document.track_count,
                        })
                        .collect()
                })
                .unwrap_or_default();
            (tracks, albums, artists, playlists)
        };

        Ok(tonic::Response::new(SearchResponse {
            tracks,
            albums,
            artists,
            playlists,
        }))
    }

    async fn filter_albums(
        &self,
        request: tonic::Request<FilterAlbumsRequest>,
    ) -> Result<tonic::Response<FilterAlbumsResponse>, tonic::Status> {
        let rules_json = request.into_inner().rules_json;
        let criteria: RuleCriteria = serde_json::from_str(&rules_json)
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        let albums = resolver::filter_albums(&self.store, &self.pool, &criteria)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(FilterAlbumsResponse {
            albums: albums.into_iter().map(Into::into).collect(),
        }))
    }

    async fn filter_artists(
        &self,
        request: tonic::Request<FilterArtistsRequest>,
    ) -> Result<tonic::Response<FilterArtistsResponse>, tonic::Status> {
        let rules_json = request.into_inner().rules_json;
        let criteria: RuleCriteria = serde_json::from_str(&rules_json)
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        let artists = resolver::filter_artists(&self.store, &self.pool, &criteria)
            .await
            .map_err(|e| tonic::Status::internal(e.to_string()))?;
        Ok(tonic::Response::new(FilterArtistsResponse {
            artists: artists.into_iter().map(Into::into).collect(),
        }))
    }

    async fn filter_tracks(
        &self,
        request: tonic::Request<FilterTracksRequest>,
    ) -> Result<tonic::Response<FilterTracksResponse>, tonic::Status> {
        let rules_json = request.into_inner().rules_json;
        let criteria: RuleCriteria = serde_json::from_str(&rules_json)
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        let tracks = resolver::resolve_tracks(&self.store, &self.pool, &criteria)
            .await
            .map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        Ok(tonic::Response::new(FilterTracksResponse {
            tracks: tracks.into_iter().map(Into::into).collect(),
        }))
    }

    async fn most_played(
        &self,
        request: tonic::Request<AnalyticsPageRequest>,
    ) -> Result<tonic::Response<TrackStatListResponse>, tonic::Status> {
        self.stat_view(
            "SELECT track_id, title, artist, album, play_count, last_played \
             FROM v_most_played LIMIT ? OFFSET ?",
            request.into_inner(),
        )
        .await
    }

    async fn most_skipped(
        &self,
        request: tonic::Request<AnalyticsPageRequest>,
    ) -> Result<tonic::Response<TrackStatListResponse>, tonic::Status> {
        self.stat_view(
            "SELECT track_id, title, artist, album, skip_count, last_skipped \
             FROM v_most_skipped LIMIT ? OFFSET ?",
            request.into_inner(),
        )
        .await
    }

    async fn never_played(
        &self,
        request: tonic::Request<AnalyticsPageRequest>,
    ) -> Result<tonic::Response<TrackStatListResponse>, tonic::Status> {
        self.added_view(
            "SELECT track_id, title, artist, album, created_at \
             FROM v_never_played LIMIT ? OFFSET ?",
            request.into_inner(),
        )
        .await
    }

    async fn recently_added(
        &self,
        request: tonic::Request<AnalyticsPageRequest>,
    ) -> Result<tonic::Response<TrackStatListResponse>, tonic::Status> {
        self.added_view(
            "SELECT track_id, title, artist, album, created_at \
             FROM v_recently_added LIMIT ? OFFSET ?",
            request.into_inner(),
        )
        .await
    }

    async fn recently_played(
        &self,
        request: tonic::Request<AnalyticsPageRequest>,
    ) -> Result<tonic::Response<PlayHistoryResponse>, tonic::Status> {
        let page = request.into_inner();
        let rows = sqlx::query(
            "SELECT track_id, title, artist, album, played_at, ms_played, length_ms, skipped \
             FROM v_recently_played LIMIT ? OFFSET ?",
        )
        .bind(page_limit(&page))
        .bind(page_offset(&page))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| tonic::Status::internal(e.to_string()))?;
        use sqlx::Row;
        Ok(tonic::Response::new(PlayHistoryResponse {
            entries: rows
                .into_iter()
                .map(|r| PlayHistoryEntry {
                    track_id: r.get(0),
                    title: r.get(1),
                    artist: r.get(2),
                    album: r.get(3),
                    played_at: r.get(4),
                    ms_played: r.get(5),
                    length_ms: r.get(6),
                    skipped: r.get::<i64, _>(7) != 0,
                })
                .collect(),
        }))
    }

    type StreamLibraryStream = Pin<
        Box<
            dyn Stream<Item = Result<StreamLibraryResponse, tonic::Status>> + Send + Sync + 'static,
        >,
    >;

    async fn stream_library(
        &self,
        _request: tonic::Request<StreamLibraryRequest>,
    ) -> Result<tonic::Response<Self::StreamLibraryStream>, tonic::Status> {
        let mut stream = SimpleBroker::<ScanCompleted>::subscribe();
        let output = async_stream::try_stream! {
            while let Some(_) = stream.next().await {
                yield StreamLibraryResponse {};
            }
        };
        Ok(tonic::Response::new(
            Box::pin(output) as Self::StreamLibraryStream
        ))
    }
}
