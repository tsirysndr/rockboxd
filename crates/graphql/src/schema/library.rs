use async_graphql::*;
use rockbox_library::{entity::favourites::Favourites, repo};
use rockbox_playlists::{resolver, rules::RuleCriteria, PlaylistStore};
use sqlx::{Pool, Sqlite};

use crate::{rockbox_url, schema::objects::track::Track};

use super::objects::{album::Album, artist::Artist, genre::Genre, search::SearchResults};

#[derive(Default)]
pub struct LibraryQuery;

#[Object]
impl LibraryQuery {
    async fn albums(&self, ctx: &Context<'_>) -> Result<Vec<Album>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::album::all(pool.clone()).await?;
        Ok(results.into_iter().map(Into::into).collect())
    }

    async fn artists(&self, ctx: &Context<'_>) -> Result<Vec<Artist>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::artist::all(pool.clone()).await?;
        Ok(results.into_iter().map(Into::into).collect())
    }

    async fn tracks(&self, ctx: &Context<'_>) -> Result<Vec<Track>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::track::all(pool.clone()).await?;
        Ok(results.into_iter().map(Into::into).collect())
    }

    async fn album(&self, ctx: &Context<'_>, id: String) -> Result<Option<Album>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::album::find(pool.clone(), &id).await?;
        let tracks = repo::album_tracks::find_by_album(pool.clone(), &id).await?;
        let mut album: Option<Album> = results.map(Into::into);
        if let Some(album) = album.as_mut() {
            album.tracks = tracks.into_iter().map(Into::into).collect();
        }
        Ok(album)
    }

    async fn artist(&self, ctx: &Context<'_>, id: String) -> Result<Option<Artist>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::artist::find(pool.clone(), &id).await?;
        let mut artist: Option<Artist> = results.map(Into::into);
        let albums = repo::album::find_by_artist(pool.clone(), &id).await?;
        let tracks = repo::artist_tracks::find_by_artist(pool.clone(), &id).await?;

        if let Some(artist) = artist.as_mut() {
            artist.albums = albums.into_iter().map(Into::into).collect();
            artist.tracks = tracks.into_iter().map(Into::into).collect();
        }

        Ok(artist)
    }

    async fn track(&self, ctx: &Context<'_>, id: String) -> Result<Option<Track>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::track::find(pool.clone(), &id).await?;
        Ok(results.map(Into::into))
    }

    async fn genres(&self, ctx: &Context<'_>) -> Result<Vec<Genre>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let genres = repo::genre::all(pool.clone()).await?;

        let mut out: Vec<Genre> = Vec::with_capacity(genres.len());
        for g in genres {
            let track_count = repo::genre::find_tracks(pool.clone(), &g.id)
                .await
                .map(|t| t.len() as i64)
                .unwrap_or(0);
            let mut genre: Genre = g.into();
            genre.track_count = track_count;
            out.push(genre);
        }
        Ok(out)
    }

    async fn genre(&self, ctx: &Context<'_>, id: String) -> Result<Option<Genre>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let result = repo::genre::find(pool.clone(), &id).await?;
        let mut genre: Option<Genre> = result.map(Into::into);
        if let Some(g) = genre.as_mut() {
            let tracks = repo::genre::find_tracks(pool.clone(), &id).await?;
            let albums = repo::genre::find_albums(pool.clone(), &id).await?;
            let artists = repo::genre::find_artists(pool.clone(), &id).await?;
            g.track_count = tracks.len() as i64;
            g.tracks = tracks.into_iter().map(Into::into).collect();
            g.albums = albums.into_iter().map(Into::into).collect();
            g.artists = artists.into_iter().map(Into::into).collect();
        }
        Ok(genre)
    }

    async fn genre_tracks(&self, ctx: &Context<'_>, id: String) -> Result<Vec<Track>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let tracks = repo::genre::find_tracks(pool.clone(), &id).await?;
        Ok(tracks.into_iter().map(Into::into).collect())
    }

    async fn genre_albums(&self, ctx: &Context<'_>, id: String) -> Result<Vec<Album>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let albums = repo::genre::find_albums(pool.clone(), &id).await?;
        Ok(albums.into_iter().map(Into::into).collect())
    }

    async fn genre_artists(&self, ctx: &Context<'_>, id: String) -> Result<Vec<Artist>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let artists = repo::genre::find_artists(pool.clone(), &id).await?;
        Ok(artists.into_iter().map(Into::into).collect())
    }

    /// Apply smart-playlist-style rules to the track library and return the
    /// matching albums (unique album of each matching track, in resolver order).
    /// `rules` is a JSON-encoded RuleCriteria (same shape used by smart playlists).
    async fn filter_albums(&self, ctx: &Context<'_>, rules: String) -> Result<Vec<Album>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let store = ctx.data::<PlaylistStore>()?;
        let criteria: RuleCriteria = serde_json::from_str(&rules)?;
        let albums = resolver::filter_albums(store, pool, &criteria).await?;
        Ok(albums.into_iter().map(Into::into).collect())
    }

    /// Same as `filter_albums` but returns matching artists.
    async fn filter_artists(&self, ctx: &Context<'_>, rules: String) -> Result<Vec<Artist>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let store = ctx.data::<PlaylistStore>()?;
        let criteria: RuleCriteria = serde_json::from_str(&rules)?;
        let artists = resolver::filter_artists(store, pool, &criteria).await?;
        Ok(artists.into_iter().map(Into::into).collect())
    }

    async fn liked_tracks(&self, ctx: &Context<'_>) -> Result<Vec<Track>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::favourites::all_tracks(pool.clone()).await?;
        Ok(results.into_iter().map(Into::into).collect())
    }

    async fn liked_albums(&self, ctx: &Context<'_>) -> Result<Vec<Album>, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        let results = repo::favourites::all_albums(pool.clone()).await?;
        Ok(results.into_iter().map(Into::into).collect())
    }

    async fn search(&self, ctx: &Context<'_>, term: String) -> Result<SearchResults, Error> {
        #[cfg(not(feature = "fts5"))]
        let (tracks, albums, artists) = {
            use rockbox_typesense::client::{search_albums, search_artists, search_tracks};
            let _ = ctx;
            let tracks = search_tracks(&term)
                .await?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let albums = search_albums(&term)
                .await?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let artists = search_artists(&term)
                .await?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            (tracks, albums, artists)
        };

        #[cfg(feature = "fts5")]
        let (tracks, albums, artists) = {
            use rockbox_fts5::{search_albums, search_artists, search_tracks};
            let pool = ctx.data::<Pool<Sqlite>>()?;
            let tracks = search_tracks(pool.clone(), &term)
                .await?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let albums = search_albums(pool.clone(), &term)
                .await?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            let artists = search_artists(pool.clone(), &term)
                .await?
                .map(|r| r.hits.into_iter().map(|h| h.document.into()).collect())
                .unwrap_or_default();
            (tracks, albums, artists)
        };

        Ok(SearchResults {
            tracks,
            albums,
            artists,
            liked_tracks: vec![],
            liked_albums: vec![],
        })
    }
}

#[derive(Default)]
pub struct LibraryMutation;

/// Mirror a like to the user's Rocksky account. Runs detached from the
/// mutation — the local favourite is already committed, so a failure here costs
/// the remote scrobble, not the like itself.
async fn sync_like_to_rocksky(pool: Pool<Sqlite>, track_id: &str) -> Result<(), anyhow::Error> {
    let Some(track) = repo::track::find(pool.clone(), track_id).await? else {
        return Ok(());
    };
    let Some(album) = repo::album::find(pool, &track.album_id).await? else {
        return Ok(());
    };
    rockbox_rocksky::like(track, album).await
}

/// Counterpart to [`sync_like_to_rocksky`].
async fn sync_unlike_to_rocksky(pool: Pool<Sqlite>, track_id: &str) -> Result<(), anyhow::Error> {
    let Some(track) = repo::track::find(pool, track_id).await? else {
        return Ok(());
    };
    rockbox_rocksky::unlike(track).await
}

#[Object]
impl LibraryMutation {
    async fn like_track(&self, ctx: &Context<'_>, id: String) -> Result<i32, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        repo::favourites::save(
            pool.clone(),
            Favourites {
                id: cuid::cuid1()?,
                track_id: Some(id.clone()),
                created_at: chrono::Utc::now(),
                album_id: None,
            },
        )
        .await?;

        // The local favourite is committed, which is all `likedTracks` reads —
        // so answer now and mirror to Rocksky in the background. That call
        // uploads the album cover and POSTs to api.rocksky.app with no timeout;
        // awaiting it held the mutation open for the whole round-trip.
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = sync_like_to_rocksky(pool, &id).await {
                tracing::warn!("rocksky like sync failed for {id}: {e}");
            }
        });
        Ok(0)
    }

    async fn like_album(&self, ctx: &Context<'_>, id: String) -> Result<i32, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        repo::favourites::save(
            pool.clone(),
            Favourites {
                id: cuid::cuid1()?,
                album_id: Some(id),
                created_at: chrono::Utc::now(),
                track_id: None,
            },
        )
        .await?;
        Ok(0)
    }

    async fn unlike_track(&self, ctx: &Context<'_>, id: String) -> Result<i32, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        repo::favourites::delete(pool.clone(), &id).await?;

        // Background, same as `like_track`.
        let pool = pool.clone();
        tokio::spawn(async move {
            if let Err(e) = sync_unlike_to_rocksky(pool, &id).await {
                tracing::warn!("rocksky unlike sync failed for {id}: {e}");
            }
        });
        Ok(0)
    }

    async fn unlike_album(&self, ctx: &Context<'_>, id: String) -> Result<i32, Error> {
        let pool = ctx.data::<Pool<Sqlite>>()?;
        repo::favourites::delete(pool.clone(), &id).await?;
        Ok(0)
    }

    async fn scan_library(&self, ctx: &Context<'_>) -> Result<i32, Error> {
        let client = ctx.data::<reqwest::Client>().unwrap();
        let url = format!("{}/scan-library", rockbox_url());
        client.put(&url).send().await?;
        Ok(0)
    }
}
