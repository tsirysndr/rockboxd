//! Last.fm HTTP client for the Similar endpoints.
//!
//! Gated on a `lastfm_api_key` in `settings.toml` — [`LastFm::from_key`]
//! returns `None` when the key is absent, and all callers must
//! short-circuit through that check before hitting the network.
//!
//! We only use two calls from the Last.fm API:
//!
//! * `artist.getsimilar` — seed artist → similar artists (name + MBID).
//! * `track.getsimilar` — seed track → similar tracks (title, artist,
//!   MBID). Album similarity has no official endpoint; the orchestrator
//!   in [`super::similar`] approximates it via `artist.getsimilar` +
//!   their top albums.

use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

const LASTFM_ROOT: &str = "https://ws.audioscrobbler.com/2.0/";

/// A similar-artist / similar-track suggestion coming back from Last.fm.
#[derive(Debug, Clone)]
pub struct SimilarArtist {
    pub name: String,
    pub mbid: Option<String>,
    /// Similarity score in the range `0.0..1.0`.
    pub score: f64,
}

#[derive(Debug, Clone)]
pub struct SimilarTrack {
    pub title: String,
    pub artist: String,
    pub artist_mbid: Option<String>,
    pub mbid: Option<String>,
    pub score: f64,
}

/// Enrichment data pulled from `artist.getInfo` — bio, tags, and the
/// largest available image URL. All fields optional per Last.fm's
/// (frequently sparse) responses.
#[derive(Debug, Clone, Default)]
pub struct ArtistInfo {
    pub mbid: Option<String>,
    pub bio: Option<String>,
    pub tags: Vec<String>,
    pub image_url: Option<String>,
}

/// Enrichment data pulled from `album.getInfo` — same shape as
/// `ArtistInfo` but sourced from the album entry.
#[derive(Debug, Clone, Default)]
pub struct AlbumInfo {
    pub mbid: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub image_url: Option<String>,
}

/// Handle to the Last.fm API. Cheap to clone — the underlying
/// `reqwest::Client` uses an `Arc` for connection reuse.
#[derive(Clone)]
pub struct LastFm {
    api_key: String,
    http: Client,
}

impl LastFm {
    /// Build a `LastFm` client when the config carries an API key. The
    /// `Option<String>` argument comes straight from
    /// `settings.lastfm_api_key`; passing `None` (or `Some("")`)
    /// returns `None` and every caller silently skips the network.
    pub fn from_key(api_key: Option<String>) -> Option<Self> {
        let key = api_key?;
        if key.trim().is_empty() {
            return None;
        }
        let http = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .ok()?;
        Some(Self { api_key: key, http })
    }

    async fn call<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        extra: &[(&str, &str)],
    ) -> anyhow::Result<T> {
        let mut params: Vec<(&str, &str)> = vec![
            ("method", method),
            ("api_key", &self.api_key),
            ("format", "json"),
        ];
        params.extend_from_slice(extra);
        let resp = self.http.get(LASTFM_ROOT).query(&params).send().await?;
        let status = resp.status();
        let body = resp.bytes().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "lastfm {method} → {status}: {}",
                String::from_utf8_lossy(&body)
            ));
        }
        Ok(serde_json::from_slice::<T>(&body)?)
    }

    /// `artist.getInfo` — bio, tags, and the largest available image
    /// URL. Prefers MBID over name when both are provided, and asks
    /// Last.fm to autocorrect misspellings so a fuzzy hit still
    /// returns a bio.
    pub async fn artist_info(
        &self,
        name: Option<&str>,
        mbid: Option<&str>,
    ) -> anyhow::Result<ArtistInfo> {
        let mut extra: Vec<(&str, &str)> = vec![("autocorrect", "1")];
        if let Some(m) = mbid {
            extra.push(("mbid", m));
        } else if let Some(n) = name {
            extra.push(("artist", n));
        } else {
            return Ok(ArtistInfo::default());
        }
        let body: ArtistInfoBody = self.call("artist.getInfo", &extra).await?;
        let mut info = ArtistInfo::default();
        let Some(artist) = body.artist else {
            return Ok(info);
        };
        info.mbid = artist.mbid.filter(|s| !s.is_empty());
        info.bio = artist
            .bio
            .and_then(|b| b.content.or(b.summary))
            .map(|s| clean_bio(&s));
        info.tags = artist
            .tags
            .map(|t| t.tag.into_iter().map(|row| row.name).collect())
            .unwrap_or_default();
        // Last.fm's image list is ordered small → mega; pick the
        // largest non-empty entry so clients get a hero-sized image.
        info.image_url = artist
            .image
            .unwrap_or_default()
            .into_iter()
            .rev()
            .find(|img| !img.text.is_empty())
            .map(|img| img.text);
        Ok(info)
    }

    /// `album.getInfo` — description (wiki summary), tags, and the
    /// largest available cover URL. Prefers MBID over `(artist, album)`
    /// when provided.
    pub async fn album_info(
        &self,
        artist: Option<&str>,
        album: Option<&str>,
        mbid: Option<&str>,
    ) -> anyhow::Result<AlbumInfo> {
        let mut extra: Vec<(&str, &str)> = vec![("autocorrect", "1")];
        if let Some(m) = mbid {
            extra.push(("mbid", m));
        } else if let (Some(a), Some(al)) = (artist, album) {
            extra.push(("artist", a));
            extra.push(("album", al));
        } else {
            return Ok(AlbumInfo::default());
        }
        let body: AlbumInfoBody = self.call("album.getInfo", &extra).await?;
        let mut info = AlbumInfo::default();
        let Some(album) = body.album else {
            return Ok(info);
        };
        info.mbid = album.mbid.filter(|s| !s.is_empty());
        info.description = album
            .wiki
            .and_then(|w| w.content.or(w.summary))
            .map(|s| clean_bio(&s));
        info.tags = album
            .tags
            .map(|t| t.tag.into_iter().map(|row| row.name).collect())
            .unwrap_or_default();
        info.image_url = album
            .image
            .unwrap_or_default()
            .into_iter()
            .rev()
            .find(|img| !img.text.is_empty())
            .map(|img| img.text);
        Ok(info)
    }

    /// `artist.getsimilar` — seeds with either an MBID (preferred, more
    /// stable) or a plain artist name.
    pub async fn similar_artists(
        &self,
        name: Option<&str>,
        mbid: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<SimilarArtist>> {
        let limit_str = limit.to_string();
        let mut extra: Vec<(&str, &str)> = vec![("limit", &limit_str), ("autocorrect", "1")];
        if let Some(m) = mbid {
            extra.push(("mbid", m));
        } else if let Some(n) = name {
            extra.push(("artist", n));
        } else {
            return Ok(Vec::new());
        }
        let body: SimilarArtistsBody = self.call("artist.getsimilar", &extra).await?;
        Ok(body
            .similarartists
            .artist
            .into_iter()
            .map(|a| SimilarArtist {
                name: a.name,
                mbid: a.mbid.filter(|s| !s.is_empty()),
                score: a.r#match.parse().unwrap_or(0.0),
            })
            .collect())
    }

    /// `track.getsimilar` — seeds with either (artist_name, track_name) or
    /// an MBID.
    pub async fn similar_tracks(
        &self,
        title: Option<&str>,
        artist: Option<&str>,
        mbid: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<SimilarTrack>> {
        let limit_str = limit.to_string();
        let mut extra: Vec<(&str, &str)> = vec![("limit", &limit_str), ("autocorrect", "1")];
        if let Some(m) = mbid {
            extra.push(("mbid", m));
        } else if let (Some(t), Some(a)) = (title, artist) {
            extra.push(("track", t));
            extra.push(("artist", a));
        } else {
            return Ok(Vec::new());
        }
        let body: SimilarTracksBody = self.call("track.getsimilar", &extra).await?;
        Ok(body
            .similartracks
            .track
            .into_iter()
            .map(|t| {
                let artist_mbid = t
                    .artist
                    .as_ref()
                    .and_then(|a| a.mbid.clone())
                    .filter(|s| !s.is_empty());
                let artist_name = t
                    .artist
                    .as_ref()
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                SimilarTrack {
                    title: t.name,
                    artist: artist_name,
                    artist_mbid,
                    mbid: t.mbid.filter(|s| !s.is_empty()),
                    score: t.r#match.unwrap_or(0.0),
                }
            })
            .collect())
    }
}

// ── Response shapes ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SimilarArtistsBody {
    #[serde(default)]
    similarartists: SimilarArtistsList,
}

#[derive(Deserialize, Default)]
struct SimilarArtistsList {
    #[serde(default)]
    artist: Vec<ArtistRow>,
}

#[derive(Deserialize)]
struct ArtistRow {
    name: String,
    #[serde(default)]
    mbid: Option<String>,
    /// Last.fm returns similarity as a string, not a float.
    r#match: String,
}

#[derive(Deserialize)]
struct SimilarTracksBody {
    #[serde(default)]
    similartracks: SimilarTracksList,
}

#[derive(Deserialize, Default)]
struct SimilarTracksList {
    #[serde(default)]
    track: Vec<TrackRow>,
}

#[derive(Deserialize)]
struct TrackRow {
    name: String,
    #[serde(default)]
    mbid: Option<String>,
    /// Track endpoint returns a float directly (unlike artist).
    #[serde(default)]
    r#match: Option<f64>,
    #[serde(default)]
    artist: Option<TrackArtist>,
}

#[derive(Deserialize)]
struct TrackArtist {
    name: String,
    #[serde(default)]
    mbid: Option<String>,
}

#[derive(Deserialize)]
struct ArtistInfoBody {
    #[serde(default)]
    artist: Option<ArtistInfoRow>,
}

#[derive(Deserialize)]
struct ArtistInfoRow {
    #[serde(default)]
    mbid: Option<String>,
    #[serde(default)]
    bio: Option<ArtistBioRow>,
    #[serde(default)]
    tags: Option<ArtistTagsRow>,
    #[serde(default)]
    image: Option<Vec<ArtistImageRow>>,
}

#[derive(Deserialize)]
struct ArtistBioRow {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Deserialize)]
struct ArtistTagsRow {
    #[serde(default)]
    tag: Vec<ArtistTagEntry>,
}

#[derive(Deserialize)]
struct ArtistTagEntry {
    name: String,
}

#[derive(Deserialize)]
struct ArtistImageRow {
    #[serde(default, rename = "#text")]
    text: String,
}

#[derive(Deserialize)]
struct AlbumInfoBody {
    #[serde(default)]
    album: Option<AlbumInfoRow>,
}

#[derive(Deserialize)]
struct AlbumInfoRow {
    #[serde(default)]
    mbid: Option<String>,
    #[serde(default)]
    wiki: Option<AlbumWikiRow>,
    #[serde(default)]
    tags: Option<ArtistTagsRow>,
    #[serde(default)]
    image: Option<Vec<ArtistImageRow>>,
}

#[derive(Deserialize)]
struct AlbumWikiRow {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// Strip the trailing `<a href="https://www.last.fm/…">Read more on Last.fm</a>`
/// suffix Last.fm bakes into every bio. Leaves regular whitespace and
/// paragraph breaks alone so the client-side renderer still sees them.
fn clean_bio(input: &str) -> String {
    let cutoff = input
        .find("<a href=\"https://www.last.fm")
        .unwrap_or(input.len());
    input[..cutoff].trim().to_string()
}
