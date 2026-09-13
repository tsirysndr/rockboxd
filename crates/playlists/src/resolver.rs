use crate::rules::{resolve, Candidate, RuleCriteria};
use crate::PlaylistStore;
use anyhow::Result;
use rockbox_library::repo;
use sqlx::{Pool, Sqlite};
use std::collections::{HashMap, HashSet};

/// Build the candidate vector from the library's local tracks, joined with
/// playback stats and likes. Reused by smart playlists and the artist/album
/// advanced filter endpoints.
pub async fn build_candidates(
    store: &PlaylistStore,
    pool: &Pool<Sqlite>,
) -> Result<(Vec<Candidate>, Vec<rockbox_library::entity::track::Track>)> {
    let all_tracks = repo::track::all(pool.clone()).await?;

    let stats_map: HashMap<String, crate::TrackStats> = store
        .get_all_track_stats()
        .await?
        .into_iter()
        .map(|s| (s.track_id.clone(), s))
        .collect();

    let liked_ids: HashSet<String> = repo::favourites::all_tracks(pool.clone())
        .await?
        .into_iter()
        .map(|t| t.id)
        .collect();

    let candidates: Vec<Candidate> = all_tracks
        .iter()
        .map(|t| {
            let stats = stats_map.get(&t.id);
            Candidate {
                id: t.id.clone(),
                title: t.title.clone(),
                artist: t.artist.clone(),
                album: t.album.clone(),
                year: t.year.map(|y| y as i64),
                genre: t.genre.clone(),
                duration_ms: t.length as i64 * 1000,
                bitrate: t.bitrate as i64,
                date_added_ts: t.created_at.timestamp(),
                play_count: stats.map(|s| s.play_count).unwrap_or(0),
                skip_count: stats.map(|s| s.skip_count).unwrap_or(0),
                last_played: stats.and_then(|s| s.last_played),
                last_skipped: stats.and_then(|s| s.last_skipped),
                is_liked: liked_ids.contains(&t.id),
            }
        })
        .collect();

    Ok((candidates, all_tracks))
}

/// The rsql sort key for a structured `SortField`, so an rsql playlist can
/// still be sorted with the same dropdown the structured editor offers.
fn rsql_sort_key(field: &crate::rules::SortField) -> Option<&'static str> {
    use crate::rules::SortField::*;
    Some(match field {
        Random => rockbox_rsql::SORT_RANDOM,
        PlayCount => "playcount",
        SkipCount => "skipcount",
        LastPlayed => "lastplayed",
        DateAdded => "added",
        Year => "year",
        Title => "title",
        Artist => "artist",
        Album => "album",
        DurationMs => "duration",
    })
}

/// The matching track ids for an rsql expression, compiled and run as SQL.
async fn rsql_track_ids(
    pool: &Pool<Sqlite>,
    criteria: &RuleCriteria,
    expression: &str,
) -> Result<Vec<String>> {
    let spec = rockbox_rsql::QuerySpec {
        filter: expression.to_string(),
        sort_by: criteria
            .sort_by
            .as_ref()
            .and_then(rsql_sort_key)
            .map(String::from),
        sort_order: match criteria.sort_order {
            Some(crate::rules::SortOrder::Asc) => rockbox_rsql::SortOrder::Asc,
            _ => rockbox_rsql::SortOrder::Desc,
        },
        limit: criteria.limit.map(|l| l as u32),
    };
    let query = rockbox_rsql::build(&spec, &rockbox_rsql::TRACKS)
        .map_err(|e| anyhow::anyhow!("rsql: {e}"))?;
    let mut q = sqlx::query_scalar::<_, String>(&query.sql);
    for param in &query.params {
        q = match param {
            rockbox_rsql::Value::Text(s) => q.bind(s.clone()),
            rockbox_rsql::Value::Integer(i) => q.bind(*i),
        };
    }
    Ok(q.fetch_all(pool).await?)
}

/// Resolve a rule criteria over the library and return matching tracks
/// (in the order produced by the rule resolver — i.e. honouring sort/limit).
///
/// An `rsql` expression, when present, replaces the structured conditions and
/// runs as SQL; the structured path stays for every playlist created with the
/// rule editor.
pub async fn resolve_tracks(
    store: &PlaylistStore,
    pool: &Pool<Sqlite>,
    criteria: &RuleCriteria,
) -> Result<Vec<rockbox_library::entity::track::Track>> {
    if let Some(expr) = criteria.rsql.as_deref().filter(|e| !e.trim().is_empty()) {
        let ids = rsql_track_ids(pool, criteria, expr).await?;
        let mut tracks = Vec::with_capacity(ids.len());
        for id in &ids {
            if let Some(t) = repo::track::find(pool.clone(), id).await? {
                tracks.push(t);
            }
        }
        return Ok(tracks);
    }
    let (candidates, all_tracks) = build_candidates(store, pool).await?;
    let resolved = resolve(criteria, candidates);
    let track_map: HashMap<&str, &rockbox_library::entity::track::Track> =
        all_tracks.iter().map(|t| (t.id.as_str(), t)).collect();
    Ok(resolved
        .iter()
        .filter_map(|c| track_map.get(c.id.as_str()).map(|t| (*t).clone()))
        .collect())
}

/// Count how many tracks would be included in the smart playlist
/// described by `criteria`.
pub async fn count_tracks(
    store: &PlaylistStore,
    pool: &Pool<Sqlite>,
    criteria: &RuleCriteria,
) -> Result<i64> {
    if let Some(expr) = criteria.rsql.as_deref().filter(|e| !e.trim().is_empty()) {
        return Ok(rsql_track_ids(pool, criteria, expr).await?.len() as i64);
    }
    let (candidates, _) = build_candidates(store, pool).await?;
    let resolved = resolve(criteria, candidates);
    Ok(resolved.len() as i64)
}

/// Resolve and return the unique album ids from the matching tracks,
/// preserving the order in which the resolver returned them (i.e. the
/// album of the first matching track first, etc.).
pub async fn resolve_album_ids(
    store: &PlaylistStore,
    pool: &Pool<Sqlite>,
    criteria: &RuleCriteria,
) -> Result<Vec<String>> {
    let tracks = resolve_tracks(store, pool, criteria).await?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut ordered: Vec<String> = Vec::new();
    for t in tracks {
        if seen.insert(t.album_id.clone()) {
            ordered.push(t.album_id);
        }
    }
    Ok(ordered)
}

/// Resolve and return the unique artist ids from the matching tracks,
/// preserving order.
pub async fn resolve_artist_ids(
    store: &PlaylistStore,
    pool: &Pool<Sqlite>,
    criteria: &RuleCriteria,
) -> Result<Vec<String>> {
    let tracks = resolve_tracks(store, pool, criteria).await?;
    let mut seen: HashSet<String> = HashSet::new();
    let mut ordered: Vec<String> = Vec::new();
    for t in tracks {
        if seen.insert(t.artist_id.clone()) {
            ordered.push(t.artist_id);
        }
    }
    Ok(ordered)
}

/// Resolve a `RuleCriteria` and return the matching `Album`s. Albums are
/// returned in the order their first matching track appears in the resolver
/// output.
pub async fn filter_albums(
    store: &PlaylistStore,
    pool: &Pool<Sqlite>,
    criteria: &RuleCriteria,
) -> Result<Vec<rockbox_library::entity::album::Album>> {
    let ordered_ids = resolve_album_ids(store, pool, criteria).await?;
    let mut albums = Vec::with_capacity(ordered_ids.len());
    for id in ordered_ids {
        if let Some(album) = repo::album::find(pool.clone(), &id).await? {
            albums.push(album);
        }
    }
    Ok(albums)
}

/// Resolve a `RuleCriteria` and return the matching `Artist`s.
pub async fn filter_artists(
    store: &PlaylistStore,
    pool: &Pool<Sqlite>,
    criteria: &RuleCriteria,
) -> Result<Vec<rockbox_library::entity::artist::Artist>> {
    let ordered_ids = resolve_artist_ids(store, pool, criteria).await?;
    let mut artists = Vec::with_capacity(ordered_ids.len());
    for id in ordered_ids {
        if let Some(artist) = repo::artist::find(pool.clone(), &id).await? {
            artists.push(artist);
        }
    }
    Ok(artists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{RuleCriteria, SortField};

    /// Every structured sort field must map onto an rsql sort key, so an rsql
    /// playlist can be sorted with the same dropdown the rule editor offers.
    /// A field this misses would silently fall back to unsorted.
    #[test]
    fn every_sort_field_has_an_rsql_key() {
        for field in [
            SortField::Random,
            SortField::PlayCount,
            SortField::SkipCount,
            SortField::LastPlayed,
            SortField::DateAdded,
            SortField::Year,
            SortField::Title,
            SortField::Artist,
            SortField::Album,
            SortField::DurationMs,
        ] {
            assert!(rsql_sort_key(&field).is_some(), "{field:?} unmapped");
        }
    }

    /// The mapped keys must exist in the rsql TRACKS schema (except the
    /// shuffle sentinel), or the built query would name an unknown field and
    /// the playlist would 400 at resolve time.
    #[test]
    fn rsql_sort_keys_resolve_in_the_schema() {
        for field in [
            SortField::PlayCount,
            SortField::SkipCount,
            SortField::LastPlayed,
            SortField::DateAdded,
            SortField::Year,
            SortField::Title,
            SortField::Artist,
            SortField::Album,
            SortField::DurationMs,
        ] {
            let key = rsql_sort_key(&field).unwrap();
            assert!(
                rockbox_rsql::TRACKS.field(key).is_some(),
                "{key} not in TRACKS schema"
            );
        }
        assert_eq!(
            rsql_sort_key(&SortField::Random),
            Some(rockbox_rsql::SORT_RANDOM)
        );
    }

    /// Rules JSON stored before the `rsql` field existed must still load —
    /// every smart playlist in every existing library is in that shape.
    #[test]
    fn criteria_without_rsql_still_deserializes() {
        let old = r#"{"match_type":"all","conditions":[],"limit":25,"sort_by":"play_count","sort_order":"DESC"}"#;
        let criteria: RuleCriteria = serde_json::from_str(old).expect("pre-rsql JSON");
        assert!(criteria.rsql.is_none());

        let with = r#"{"rsql":"genre==rock;playcount>5"}"#;
        let criteria: RuleCriteria = serde_json::from_str(with).expect("rsql-only JSON");
        assert_eq!(criteria.rsql.as_deref(), Some("genre==rock;playcount>5"));
    }
}
