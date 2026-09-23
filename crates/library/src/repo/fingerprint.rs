//! The fingerprint pass's view of the library.
//!
//! Two work queues and the writes that drain them: tracks with no fingerprint
//! yet, and fingerprinted tracks not yet identified against AcoustID. Plus the
//! part that is more than a column write — [`fill_unknown`], which takes
//! identified metadata and puts it into a track *without* disturbing anything
//! the file already said.
//!
//! Filling a track's artist or album is never just an `UPDATE track`: the
//! library reaches tracks through `artist` and `album` rows and the join tables
//! between them, so a track whose artist changes has to be moved to the artist
//! it now belongs to. A half-applied fill — right name on the track row, still
//! joined to "Unknown Artist" — reads worse in every UI than no fill at all.

use crate::entity::{album::Album, album_tracks::AlbumTracks, artist::Artist, track::Track};
use sqlx::{Error, FromRow, Pool, Sqlite};

/// A track waiting to be fingerprinted.
#[derive(Clone, Debug, FromRow)]
pub struct Pending {
    pub id: String,
    pub path: String,
}

/// A fingerprinted track waiting to be identified.
#[derive(Clone, Debug, FromRow)]
pub struct Unidentified {
    pub id: String,
    pub path: String,
    pub fingerprint: String,
    pub fingerprint_duration: Option<f64>,
    /// The tagged length in seconds, as the fallback when the fingerprint pass
    /// could not measure one.
    pub length: u32,
}

/// Identified metadata, as far as the library is concerned.
///
/// Deliberately a plain struct rather than the fingerprint crate's type: the
/// library knows what a track is and what "unknown" means for one, and should
/// not have to link a decoder and an HTTP client to say so.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdentifiedTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
}

impl IdentifiedTags {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Local tracks with no fingerprint attempt recorded yet.
///
/// Recently played first, for the same reason the key/BPM pass does it: the
/// pass takes a while on a large library and the tracks the user is listening
/// to now are the ones whose metadata they are looking at.
pub async fn pending_fingerprint(pool: Pool<Sqlite>, limit: i64) -> Result<Vec<Pending>, Error> {
    sqlx::query_as(
        "SELECT t.id, t.path FROM track t \
         LEFT JOIN track_stats s ON s.track_id = t.id \
         WHERE t.fingerprint IS NULL AND t.is_remote = 0 AND t.path NOT LIKE 'http%' \
         ORDER BY s.last_played DESC NULLS LAST, t.id \
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await
}

/// Record a fingerprint, or the fact that there isn't going to be one.
///
/// `None` stores the empty string: the file could not be decoded, or held too
/// little audio to fingerprint. That is a durable property of the file, so it
/// is remembered rather than retried on every boot.
pub async fn set_fingerprint(
    pool: Pool<Sqlite>,
    id: &str,
    fingerprint: Option<(&str, f32)>,
) -> Result<(), Error> {
    let (value, duration) = match fingerprint {
        Some((value, duration)) => (value, Some(duration as f64)),
        None => ("", None),
    };
    sqlx::query("UPDATE track SET fingerprint = ?, fingerprint_duration = ? WHERE id = ?")
        .bind(value)
        .bind(duration)
        .bind(id)
        .execute(&pool)
        .await?;
    Ok(())
}

/// Fingerprinted tracks that have not been looked up against AcoustID.
///
/// Skips the rows whose fingerprint is the empty marker — there is nothing to
/// look up — and the ones already answered, match or no match.
pub async fn pending_identification(
    pool: Pool<Sqlite>,
    limit: i64,
) -> Result<Vec<Unidentified>, Error> {
    sqlx::query_as(
        "SELECT t.id, t.path, t.fingerprint, t.fingerprint_duration, t.length FROM track t \
         LEFT JOIN track_stats s ON s.track_id = t.id \
         WHERE t.acoustid IS NULL AND t.fingerprint IS NOT NULL AND t.fingerprint != '' \
           AND t.is_remote = 0 \
         ORDER BY s.last_played DESC NULLS LAST, t.id \
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await
}

/// Record what AcoustID said, including when it said nothing.
///
/// An empty `acoustid` is the no-match marker — most bootlegs and home
/// recordings land there, and asking again next boot would only spend the
/// user's rate limit on the same answer.
pub async fn set_acoustid(
    pool: Pool<Sqlite>,
    id: &str,
    acoustid: &str,
    recording_id: Option<&str>,
) -> Result<(), Error> {
    sqlx::query("UPDATE track SET acoustid = ?, musicbrainz_recording_id = ? WHERE id = ?")
        .bind(acoustid)
        .bind(recording_id)
        .bind(id)
        .execute(&pool)
        .await?;
    Ok(())
}

/// Every group of tracks that are the same recording in more than one file.
///
/// The fingerprint's other use: an AcoustID is stable across encodings, so two
/// rows sharing one are a FLAC and its MP3, or the same track from an album and
/// a compilation.
pub async fn duplicate_groups(pool: Pool<Sqlite>) -> Result<Vec<(String, Vec<String>)>, Error> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT acoustid, id FROM track \
         WHERE acoustid IS NOT NULL AND acoustid != '' \
           AND acoustid IN (SELECT acoustid FROM track \
                            WHERE acoustid IS NOT NULL AND acoustid != '' \
                            GROUP BY acoustid HAVING COUNT(*) > 1) \
         ORDER BY acoustid, id",
    )
    .fetch_all(&pool)
    .await?;

    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for (acoustid, track_id) in rows {
        match groups.last_mut() {
            Some((current, ids)) if *current == acoustid => ids.push(track_id),
            _ => groups.push((acoustid, vec![track_id])),
        }
    }
    Ok(groups)
}

/// Fill in what the file never said, and leave everything else alone.
///
/// Returns what was actually applied — empty when the track was already
/// tagged, which is most of a normal library. The caller uses that to decide
/// whether the *file* needs rewriting too.
pub async fn fill_unknown(
    pool: Pool<Sqlite>,
    track: &Track,
    tags: &IdentifiedTags,
) -> Result<IdentifiedTags, Error> {
    let applied = IdentifiedTags {
        title: tags
            .title
            .clone()
            .filter(|_| is_unknown_title(&track.title, &track.path)),
        artist: tags
            .artist
            .clone()
            .filter(|_| is_unknown(&track.artist, "Unknown Artist")),
        album: tags
            .album
            .clone()
            .filter(|_| is_unknown(&track.album, "Unknown Album")),
        album_artist: tags
            .album_artist
            .clone()
            .filter(|_| is_unknown(&track.album_artist, "Unknown Artist")),
        year: tags.year.filter(|_| track.year.is_none()),
        track_number: tags.track_number.filter(|_| track.track_number.is_none()),
        // Zero is what the scanner writes when a file states no disc at all.
        disc_number: tags.disc_number.filter(|_| track.disc_number == 0),
    };

    if applied.is_empty() {
        return Ok(applied);
    }

    // The values the row will end up with, tag or fill.
    let title = applied.title.clone().unwrap_or_else(|| track.title.clone());
    let artist = applied
        .artist
        .clone()
        .unwrap_or_else(|| track.artist.clone());
    let album = applied.album.clone().unwrap_or_else(|| track.album.clone());
    let album_artist = applied
        .album_artist
        .clone()
        .unwrap_or_else(|| track.album_artist.clone());
    let year = applied.year.or(track.year);
    let track_number = applied.track_number.or(track.track_number);
    let disc_number = applied.disc_number.unwrap_or(track.disc_number);

    // Moving the track to the artist and album it now belongs to. Both saves
    // are upserts on the natural key, so an artist the library already has
    // comes back with its existing id rather than a second row.
    let artist_id = if applied.artist.is_some() {
        repo_artist_id(pool.clone(), &artist).await?
    } else {
        track.artist_id.clone()
    };

    let album_id = if applied.album.is_some() || applied.album_artist.is_some() {
        repo_album_id(pool.clone(), &album, &album_artist, year, &artist_id, track).await?
    } else {
        track.album_id.clone()
    };

    sqlx::query(
        "UPDATE track SET title = ?, artist = ?, album = ?, album_artist = ?, year = ?, \
         year_string = ?, track_number = ?, disc_number = ?, artist_id = ?, album_id = ?, \
         updated_at = ? WHERE id = ?",
    )
    .bind(&title)
    .bind(&artist)
    .bind(&album)
    .bind(&album_artist)
    .bind(year)
    .bind(year.map(|y| y.to_string()))
    .bind(track_number)
    .bind(disc_number)
    .bind(&artist_id)
    .bind(&album_id)
    .bind(chrono::Utc::now())
    .bind(&track.id)
    .execute(&pool)
    .await?;

    if artist_id != track.artist_id {
        sqlx::query("DELETE FROM artist_tracks WHERE track_id = ?")
            .bind(&track.id)
            .execute(&pool)
            .await?;
        let link = crate::entity::artist_tracks::ArtistTracks {
            id: cuid_or_fallback(&track.id, "artist"),
            artist_id: artist_id.clone(),
            track_id: track.id.clone(),
        };
        super::artist_tracks::save(pool.clone(), link).await?;
    }

    if album_id != track.album_id {
        sqlx::query("DELETE FROM album_tracks WHERE track_id = ?")
            .bind(&track.id)
            .execute(&pool)
            .await?;
        let link = AlbumTracks {
            id: cuid_or_fallback(&track.id, "album"),
            album_id: album_id.clone(),
            track_id: track.id.clone(),
        };
        super::album_tracks::save(pool.clone(), link).await?;
    }

    // The "Unknown Artist" / "Unknown Album" rows the track just left may now
    // hold nothing. Failing to sweep them is cosmetic, not a reason to undo a
    // good fill, so it is logged rather than propagated.
    if album_id != track.album_id {
        if let Err(e) = super::album::delete_orphans(pool.clone()).await {
            tracing::debug!("fingerprint fill: album orphan cleanup: {e}");
        }
    }
    if artist_id != track.artist_id {
        if let Err(e) = super::artist::delete_orphans(pool.clone()).await {
            tracing::debug!("fingerprint fill: artist orphan cleanup: {e}");
        }
    }

    Ok(applied)
}

async fn repo_artist_id(pool: Pool<Sqlite>, name: &str) -> Result<String, Error> {
    super::artist::save(
        pool,
        Artist {
            id: cuid_or_fallback(name, "artist"),
            name: name.to_string(),
            bio: None,
            image: None,
            genres: None,
        },
    )
    .await
}

async fn repo_album_id(
    pool: Pool<Sqlite>,
    album: &str,
    album_artist: &str,
    year: Option<u32>,
    artist_id: &str,
    track: &Track,
) -> Result<String, Error> {
    // The same key the scanner builds, so a fill lands on the album row the
    // scanner would have created for a properly tagged copy of this file.
    let md5 = format!(
        "{:x}",
        md5::compute(format!("{}{}{}", album_artist, album, year.unwrap_or(0)).as_bytes())
    );
    super::album::save(
        pool,
        Album {
            id: cuid_or_fallback(&md5, "album"),
            title: album.to_string(),
            artist: album_artist.to_string(),
            year: year.unwrap_or(0),
            year_string: year.map(|y| y.to_string()).unwrap_or_default(),
            album_art: track.album_art.clone(),
            md5,
            artist_id: artist_id.to_string(),
            label: None,
            copyright_message: None,
        },
    )
    .await
}

/// A cuid, or something unique enough when the generator refuses.
///
/// `cuid1()` fails only on a broken clock or hostname; a fill is not worth
/// abandoning over that, and these ids are opaque.
fn cuid_or_fallback(seed: &str, kind: &str) -> String {
    cuid::cuid1().unwrap_or_else(|_| {
        format!(
            "{}-{:x}",
            kind,
            md5::compute(format!("{seed}{kind}{:?}", std::time::SystemTime::now()).as_bytes())
        )
    })
}

/// Whether a field still holds the scanner's placeholder rather than anything
/// the file said.
fn is_unknown(value: &str, placeholder: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.eq_ignore_ascii_case(placeholder)
}

/// Whether a title is a real title.
///
/// Untagged files are the whole point of this pass, and the scanner gives one
/// of those its file name as a title — so a title that is just the file name is
/// exactly as unknown as an empty one, and the common case rather than an edge
/// case.
fn is_unknown_title(title: &str, path: &str) -> bool {
    if is_unknown(title, "Unknown Title") {
        return true;
    }
    let Some(filename) = path.rsplit('/').next() else {
        return false;
    };
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    let title = title.trim();
    title.eq_ignore_ascii_case(filename) || title.eq_ignore_ascii_case(stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_placeholder_is_not_a_value() {
        assert!(is_unknown("", "Unknown Artist"));
        assert!(is_unknown("   ", "Unknown Artist"));
        assert!(is_unknown("Unknown Artist", "Unknown Artist"));
        assert!(is_unknown("unknown artist", "Unknown Artist"));
        assert!(!is_unknown("Soft Cell", "Unknown Artist"));
        // A band actually called this would be a shame, but "Unknown Mortal
        // Orchestra" is not the placeholder and must survive.
        assert!(!is_unknown("Unknown Mortal Orchestra", "Unknown Artist"));
    }

    /// The case the whole pass exists for: a file with no tags at all, whose
    /// "title" is whatever it happens to be called on disk.
    #[test]
    fn a_filename_is_not_a_title() {
        assert!(is_unknown_title("01 - track.mp3", "/music/01 - track.mp3"));
        assert!(is_unknown_title("01 - track", "/music/01 - track.mp3"));
        assert!(is_unknown_title("", "/music/01 - track.mp3"));
        assert!(is_unknown_title("Unknown Title", "/music/01 - track.mp3"));
        // A real title stays a real title, even one that reads like a file
        // name.
        assert!(!is_unknown_title("Tainted Love", "/music/01 - track.mp3"));
    }

    /// A title that happens to match its file name because the file was named
    /// after the song is still replaceable — by the same string. No harm, and
    /// the alternative is missing every genuinely untagged file.
    #[test]
    fn a_file_named_after_its_song_is_still_treated_as_untagged() {
        assert!(is_unknown_title("Tainted Love", "/music/Tainted Love.mp3"));
    }

    #[test]
    fn an_empty_fill_is_recognised_as_empty() {
        assert!(IdentifiedTags::default().is_empty());
        assert!(!IdentifiedTags {
            title: Some("T".into()),
            ..Default::default()
        }
        .is_empty());
    }
}

/// Against the real schema, on a fresh in-memory database.
///
/// The interesting half of this module is not the column writes — it is what
/// [`fill_unknown`] does to the rows *around* a track, which no amount of
/// reading the function proves.
#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::entity::artist_tracks::ArtistTracks;
    use chrono::Utc;
    use sqlx::{Executor, Row, SqlitePool};

    async fn db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        for migration in [
            include_str!("../../migrations/20240923093823_create_tables.sql"),
            include_str!("../../migrations/20241011011557_add_artist_id_column.sql"),
            include_str!("../../migrations/20241020125757_add-album_id-column.sql"),
            include_str!("../../migrations/20251218042124_add_album_label.sql"),
            include_str!("../../migrations/20251218044147_add_album_copyright_message.sql"),
            include_str!("../../migrations/20260425000000_add_playlist_tables.sql"),
            include_str!("../../migrations/20260428000000_add_is_remote_to_track.sql"),
            include_str!("../../migrations/20260913000000_add_play_history.sql"),
            include_str!("../../migrations/20260923000000_add_fingerprint.sql"),
            include_str!("../../migrations/20260923000001_add_fingerprint_indexes.sql"),
        ] {
            pool.execute(migration).await.unwrap();
        }
        // `artist::delete_orphans` sweeps this table too. Its own migration
        // also carries an `ALTER TABLE ... ADD CONSTRAINT`, which SQLite does
        // not accept and which production tolerates by ignoring the failure —
        // so the table is created directly rather than by running that script.
        for statement in [
            "CREATE TABLE IF NOT EXISTS artist_genres (id VARCHAR(255) PRIMARY KEY, \
             artist_id VARCHAR(255) NOT NULL, genre_id VARCHAR(255) NOT NULL, \
             UNIQUE (artist_id, genre_id))",
            "ALTER TABLE artist ADD COLUMN genres VARCHAR(255) DEFAULT NULL",
        ] {
            pool.execute(statement).await.unwrap();
        }
        pool
    }

    /// A track as the scanner writes one for a file with no tags at all: the
    /// file name as a title, placeholders everywhere else.
    async fn untagged(pool: &SqlitePool) -> Track {
        let artist_id = super::super::artist::save(
            pool.clone(),
            Artist {
                id: "artist-unknown".into(),
                name: "Unknown Artist".into(),
                bio: None,
                image: None,
                genres: None,
            },
        )
        .await
        .unwrap();
        let album_id = super::super::album::save(
            pool.clone(),
            Album {
                id: "album-unknown".into(),
                title: "Unknown Album".into(),
                artist: "Unknown Artist".into(),
                year: 0,
                year_string: String::new(),
                album_art: None,
                md5: "album-md5".into(),
                artist_id: artist_id.clone(),
                label: None,
                copyright_message: None,
            },
        )
        .await
        .unwrap();

        let track = Track {
            id: "track-1".into(),
            path: "/music/03 - untitled.mp3".into(),
            title: "03 - untitled.mp3".into(),
            artist: "Unknown Artist".into(),
            album: "Unknown Album".into(),
            album_artist: "Unknown Artist".into(),
            md5: "track-md5".into(),
            length: 160,
            artist_id: artist_id.clone(),
            album_id: album_id.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            ..Default::default()
        };
        super::super::track::save(pool.clone(), track.clone())
            .await
            .unwrap();
        super::super::album_tracks::save(
            pool.clone(),
            AlbumTracks {
                id: "link-album".into(),
                album_id,
                track_id: track.id.clone(),
            },
        )
        .await
        .unwrap();
        super::super::artist_tracks::save(
            pool.clone(),
            ArtistTracks {
                id: "link-artist".into(),
                artist_id,
                track_id: track.id.clone(),
            },
        )
        .await
        .unwrap();
        track
    }

    fn identified() -> IdentifiedTags {
        IdentifiedTags {
            title: Some("Tainted Love".into()),
            artist: Some("Soft Cell".into()),
            album: Some("Non-Stop Erotic Cabaret".into()),
            album_artist: Some("Soft Cell".into()),
            year: Some(1981),
            track_number: Some(3),
            disc_number: Some(1),
        }
    }

    /// The whole point: an untagged file comes out of this joined to the right
    /// artist and album, not merely *labelled* with their names.
    #[tokio::test]
    async fn an_untagged_track_is_moved_to_the_artist_it_turns_out_to_be() {
        let pool = db().await;
        let track = untagged(&pool).await;

        let applied = fill_unknown(pool.clone(), &track, &identified())
            .await
            .unwrap();
        assert_eq!(applied, identified(), "every placeholder was replaceable");

        let filled = super::super::track::find(pool.clone(), &track.id)
            .await
            .unwrap()
            .expect("the track is still there");
        assert_eq!(filled.title, "Tainted Love");
        assert_eq!(filled.artist, "Soft Cell");
        assert_eq!(filled.album, "Non-Stop Erotic Cabaret");
        assert_eq!(filled.year, Some(1981));
        assert_eq!(filled.track_number, Some(3));
        assert_eq!(filled.disc_number, 1);

        // Labelled is not enough — the row has to point at the new artist.
        let artist = super::super::artist::find_by_name(pool.clone(), "Soft Cell")
            .await
            .unwrap()
            .expect("the artist was created");
        assert_eq!(filled.artist_id, artist.id);
        assert_ne!(filled.album_id, track.album_id, "and at a new album");

        // …and be reachable through the join tables, which is how every list
        // in the UI finds it.
        let by_artist: Vec<String> =
            sqlx::query("SELECT artist_id FROM artist_tracks WHERE track_id = ?")
                .bind(&track.id)
                .fetch_all(&pool)
                .await
                .unwrap()
                .iter()
                .map(|r| r.get(0))
                .collect();
        assert_eq!(
            by_artist,
            vec![artist.id.clone()],
            "one link, to the new artist"
        );

        let by_album: Vec<String> =
            sqlx::query("SELECT album_id FROM album_tracks WHERE track_id = ?")
                .bind(&track.id)
                .fetch_all(&pool)
                .await
                .unwrap()
                .iter()
                .map(|r| r.get(0))
                .collect();
        assert_eq!(by_album, vec![filled.album_id.clone()]);

        // The placeholders the track just left held nothing else, so they go.
        assert!(
            super::super::artist::find_by_name(pool.clone(), "Unknown Artist")
                .await
                .unwrap()
                .is_none(),
            "the empty Unknown Artist row was swept up"
        );
    }

    /// A properly tagged track must come out of an identification untouched —
    /// a wrong AcoustID match is a real possibility, and it may never cost the
    /// user metadata they already had.
    #[tokio::test]
    async fn a_tagged_track_is_left_exactly_as_it_was() {
        let pool = db().await;
        let mut track = untagged(&pool).await;
        track.title = "Something Else".into();
        track.artist = "Another Band".into();
        track.album = "Another Album".into();
        track.album_artist = "Another Band".into();
        track.year = Some(1999);
        track.track_number = Some(7);
        track.disc_number = 2;

        let applied = fill_unknown(pool.clone(), &track, &identified())
            .await
            .unwrap();
        assert!(
            applied.is_empty(),
            "nothing was missing, so nothing changed"
        );

        let after = super::super::track::find(pool.clone(), &track.id)
            .await
            .unwrap()
            .unwrap();
        // Untouched on disk as well as in the returned value: fill_unknown
        // returned early rather than writing the same values back.
        assert_eq!(after.title, "03 - untitled.mp3");
        assert_eq!(after.artist_id, track.artist_id);
    }

    /// The common half-tagged case: a title and artist, no album. Only the gap
    /// is filled, and only the album relinking happens.
    #[tokio::test]
    async fn a_partly_tagged_track_only_has_its_gaps_filled() {
        let pool = db().await;
        let mut track = untagged(&pool).await;
        track.title = "Tainted Love".into();
        track.artist = "Soft Cell".into();

        let applied = fill_unknown(pool.clone(), &track, &identified())
            .await
            .unwrap();
        assert_eq!(applied.title, None, "the title was already there");
        assert_eq!(applied.artist, None);
        assert_eq!(applied.album.as_deref(), Some("Non-Stop Erotic Cabaret"));
        assert_eq!(applied.year, Some(1981));

        let after = super::super::track::find(pool.clone(), &track.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.artist, "Soft Cell");
        assert_eq!(after.album, "Non-Stop Erotic Cabaret");
        assert_eq!(
            after.artist_id, track.artist_id,
            "the artist didn't change, so neither did the link"
        );
    }

    /// The two queues, and the markers that take a track out of them.
    #[tokio::test]
    async fn the_work_queues_drain_and_stay_drained() {
        let pool = db().await;
        let track = untagged(&pool).await;

        let queue = pending_fingerprint(pool.clone(), 10).await.unwrap();
        assert_eq!(queue.len(), 1, "a fresh track is waiting");
        assert_eq!(queue[0].id, track.id);
        // Nothing to identify yet — there is no fingerprint to look up.
        assert!(pending_identification(pool.clone(), 10)
            .await
            .unwrap()
            .is_empty());

        set_fingerprint(pool.clone(), &track.id, Some(("AQADtEs", 241.6)))
            .await
            .unwrap();
        assert!(pending_fingerprint(pool.clone(), 10)
            .await
            .unwrap()
            .is_empty());

        let queue = pending_identification(pool.clone(), 10).await.unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].fingerprint, "AQADtEs");
        // Stored from an f32, so it comes back widened rather than exact —
        // which is fine: AcoustID is given whole seconds.
        assert!((queue[0].fingerprint_duration.unwrap() - 241.6).abs() < 0.01);

        // A no-match is an answer, and takes the track out of the queue for
        // good rather than leaving it to be asked again every boot.
        set_acoustid(pool.clone(), &track.id, "", None)
            .await
            .unwrap();
        assert!(pending_identification(pool.clone(), 10)
            .await
            .unwrap()
            .is_empty());
    }

    /// A file that cannot be decoded is remembered as such — the empty marker
    /// must not put it back in the identification queue either.
    #[tokio::test]
    async fn an_undecodable_file_leaves_both_queues() {
        let pool = db().await;
        let track = untagged(&pool).await;

        set_fingerprint(pool.clone(), &track.id, None)
            .await
            .unwrap();
        assert!(pending_fingerprint(pool.clone(), 10)
            .await
            .unwrap()
            .is_empty());
        assert!(
            pending_identification(pool.clone(), 10)
                .await
                .unwrap()
                .is_empty(),
            "there is no fingerprint to look up"
        );
    }

    /// Duplicates are the fingerprint's other job: same AcoustID, different
    /// files.
    #[tokio::test]
    async fn tracks_sharing_an_acoustid_are_reported_as_one_group() {
        let pool = db().await;
        let track = untagged(&pool).await;

        let mut other = track.clone();
        other.id = "track-2".into();
        other.path = "/music/flac/03.flac".into();
        other.md5 = "track-md5-2".into();
        super::super::track::save(pool.clone(), other.clone())
            .await
            .unwrap();

        // The same recording in two files, plus one that is something else and
        // one that was looked up and not found.
        let mut lonely = track.clone();
        lonely.id = "track-3".into();
        lonely.path = "/music/other.mp3".into();
        lonely.md5 = "track-md5-3".into();
        super::super::track::save(pool.clone(), lonely.clone())
            .await
            .unwrap();

        set_acoustid(pool.clone(), &track.id, "shared-id", Some("mbid"))
            .await
            .unwrap();
        set_acoustid(pool.clone(), &other.id, "shared-id", Some("mbid"))
            .await
            .unwrap();
        set_acoustid(pool.clone(), &lonely.id, "", None)
            .await
            .unwrap();

        let groups = duplicate_groups(pool.clone()).await.unwrap();
        assert_eq!(groups.len(), 1, "only the shared id is a duplicate group");
        assert_eq!(groups[0].0, "shared-id");
        assert_eq!(groups[0].1, vec!["track-1", "track-2"]);
    }
}
