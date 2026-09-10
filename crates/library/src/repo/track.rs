use crate::entity::track::Track;
use sqlx::{Error, Pool, Sqlite};

pub async fn save(pool: Pool<Sqlite>, track: Track) -> Result<String, Error> {
    match sqlx::query(
        r#"
        INSERT INTO track (
          id, 
          path, 
          title, 
          artist,
          album,
          genre,
          year,
          track_number,
          disc_number,
          year_string,
          composer,
          album_artist,
          bitrate,
          frequency,
          filesize,
          length,
          md5,
          created_at,
          updated_at,
          artist_id,
          album_id,
          album_art,
          is_remote
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
        "#,
    )
    .bind(&track.id)
    .bind(&track.path)
    .bind(&track.title)
    .bind(&track.artist)
    .bind(&track.album)
    .bind(track.genre)
    .bind(track.year)
    .bind(track.track_number)
    .bind(track.disc_number)
    .bind(&track.year_string)
    .bind(&track.composer)
    .bind(&track.album_artist)
    .bind(track.bitrate)
    .bind(track.frequency)
    .bind(track.filesize)
    .bind(track.length)
    .bind(&track.md5)
    .bind(track.created_at)
    .bind(track.updated_at)
    .bind(&track.artist_id)
    .bind(&track.album_id)
    .bind(&track.album_art)
    .bind(track.is_remote)
    .execute(&pool)
    .await {
        Ok(_) => Ok(track.id.clone()),
        Err(_e) => {
            // eprintln!("Error saving track: {:?}", e);
            let track = find_by_md5(pool.clone(), &track.md5).await?;
            Ok(track.unwrap().id)
        }
    }
}

pub async fn find(pool: Pool<Sqlite>, id: &str) -> Result<Option<Track>, Error> {
    let result: Option<Track> = sqlx::query_as("SELECT * FROM track WHERE id = $1")
        .bind(id)
        .fetch_optional(&pool)
        .await?;
    Ok(result)
}

pub async fn filter(
    pool: Pool<Sqlite>,
    r#where: (String, Vec<String>),
) -> Result<Vec<Track>, Error> {
    let sql = format!("SELECT * FROM track WHERE is_remote = 0 AND {}", r#where.0);
    let mut query = sqlx::query_as(&sql);

    for value in r#where.1 {
        query = query.bind(value.clone());
    }

    let result = query.fetch_all(&pool).await?;
    Ok(result)
}

pub async fn find_by_md5(pool: Pool<Sqlite>, md5: &str) -> Result<Option<Track>, Error> {
    let result: Option<Track> = sqlx::query_as("SELECT * FROM track WHERE md5 = $1")
        .bind(md5)
        .fetch_optional(&pool)
        .await?;
    Ok(result)
}

pub async fn find_by_path(pool: Pool<Sqlite>, path: &str) -> Result<Option<Track>, Error> {
    let result: Option<Track> = sqlx::query_as("SELECT * FROM track WHERE path = $1")
        .bind(path)
        .fetch_optional(&pool)
        .await?;
    Ok(result)
}

pub async fn delete_by_path(pool: Pool<Sqlite>, path: &str) -> Result<Option<Track>, Error> {
    let track: Option<Track> = sqlx::query_as("SELECT * FROM track WHERE path = $1")
        .bind(path)
        .fetch_optional(&pool)
        .await?;
    let Some(track) = track else {
        return Ok(None);
    };

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM album_tracks WHERE track_id = $1")
        .bind(&track.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM artist_tracks WHERE track_id = $1")
        .bind(&track.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM playlist_tracks WHERE track_id = $1")
        .bind(&track.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM favourites WHERE track_id = $1")
        .bind(&track.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM track WHERE id = $1")
        .bind(&track.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Some(track))
}

pub async fn all(pool: Pool<Sqlite>) -> Result<Vec<Track>, Error> {
    let result: Vec<Track> =
        sqlx::query_as("SELECT * FROM track WHERE is_remote = 0 ORDER BY title ASC")
            .fetch_all(&pool)
            .await?;
    Ok(result)
}

/// `(id, path)` for every local track. Cheap projection for the scan's delete
/// reconciliation, which only needs to stat each path — pulling full rows for
/// a 50k-track library just to call `Path::exists()` is wasteful.
pub async fn local_paths(pool: Pool<Sqlite>) -> Result<Vec<(String, String)>, Error> {
    let result: Vec<(String, String)> =
        sqlx::query_as("SELECT id, path FROM track WHERE is_remote = 0")
            .fetch_all(&pool)
            .await?;
    Ok(result)
}

/// Delete tracks by id together with every row that references them, in a
/// single transaction. Returns the number of `track` rows removed.
pub async fn delete_by_ids(pool: Pool<Sqlite>, ids: &[String]) -> Result<u64, Error> {
    if ids.is_empty() {
        return Ok(0);
    }

    let placeholders = vec!["?"; ids.len()].join(",");
    let mut tx = pool.begin().await?;

    // Referencing tables keyed by track_id. `track_stats` goes too: its
    // play/skip counters are keyed by the old cuid and a re-added file always
    // gets a fresh id, so they could never be reattached.
    for table in [
        "album_tracks",
        "artist_tracks",
        "playlist_tracks",
        "saved_playlist_tracks",
        "favourites",
        "track_stats",
    ] {
        let sql = format!("DELETE FROM {} WHERE track_id IN ({})", table, placeholders);
        let mut query = sqlx::query(&sql);
        for id in ids {
            query = query.bind(id);
        }
        query.execute(&mut *tx).await?;
    }

    let sql = format!("DELETE FROM track WHERE id IN ({})", placeholders);
    let mut query = sqlx::query(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let deleted = query.execute(&mut *tx).await?.rows_affected();

    tx.commit().await?;
    Ok(deleted)
}

/// Paginated track list narrowed by Jellyfin's alpha-jump filter params —
/// see [`super::artist::filtered`] for the parameter semantics.
pub async fn filtered(
    pool: Pool<Sqlite>,
    name_starts_with: Option<&str>,
    name_starts_with_or_greater: Option<&str>,
    name_less_than: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Track>, Error> {
    let (where_sql, binds) = super::name_filter::sql(
        "title",
        name_starts_with,
        name_starts_with_or_greater,
        name_less_than,
    );
    let limit_idx = binds.len() + 1;
    let offset_idx = binds.len() + 2;
    let sql = format!(
        "SELECT * FROM track WHERE is_remote = 0 {extra}
         ORDER BY title COLLATE NOCASE LIMIT ?{limit_idx} OFFSET ?{offset_idx}",
        extra = if where_sql.is_empty() {
            String::new()
        } else {
            format!("AND {}", where_sql.trim_start_matches("WHERE "))
        }
    );
    let mut q = sqlx::query_as::<_, Track>(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    q.bind(limit).bind(offset).fetch_all(&pool).await
}

pub async fn count_filtered(
    pool: Pool<Sqlite>,
    name_starts_with: Option<&str>,
    name_starts_with_or_greater: Option<&str>,
    name_less_than: Option<&str>,
) -> Result<i64, Error> {
    let (where_sql, binds) = super::name_filter::sql(
        "title",
        name_starts_with,
        name_starts_with_or_greater,
        name_less_than,
    );
    let sql = format!(
        "SELECT COUNT(*) FROM track WHERE is_remote = 0 {extra}",
        extra = if where_sql.is_empty() {
            String::new()
        } else {
            format!("AND {}", where_sql.trim_start_matches("WHERE "))
        }
    );
    let mut q = sqlx::query_scalar::<_, i64>(&sql);
    for b in &binds {
        q = q.bind(b);
    }
    q.fetch_one(&pool).await
}

pub async fn name_prefixes(pool: Pool<Sqlite>) -> Result<Vec<String>, Error> {
    super::name_filter::prefixes(&pool, "track", "title", Some("is_remote = 0")).await
}

pub async fn update_album_art(pool: Pool<Sqlite>, id: &str, album_art: &str) -> Result<(), Error> {
    sqlx::query("UPDATE track SET album_art = $2 WHERE id = $1")
        .bind(id)
        .bind(album_art)
        .execute(&pool)
        .await?;
    Ok(())
}

pub async fn find_by_artist(pool: Pool<Sqlite>, artist: &str) -> Result<Vec<Track>, Error> {
    let result: Vec<Track> = sqlx::query_as(
        "SELECT * FROM track WHERE is_remote = 0 AND artist = $1 ORDER BY title ASC",
    )
    .bind(artist)
    .fetch_all(&pool)
    .await?;
    Ok(result)
}

pub async fn find_by_album(pool: Pool<Sqlite>, album: &str) -> Result<Vec<Track>, Error> {
    let result: Vec<Track> =
        sqlx::query_as("SELECT * FROM track WHERE is_remote = 0 AND album = $1 ORDER BY title ASC")
            .bind(album)
            .fetch_all(&pool)
            .await?;
    Ok(result)
}

pub async fn find_by_title(pool: Pool<Sqlite>, title: &str) -> Result<Vec<Track>, Error> {
    let result: Vec<Track> =
        sqlx::query_as("SELECT * FROM track WHERE is_remote = 0 AND title = $1 ORDER BY title ASC")
            .bind(title)
            .fetch_all(&pool)
            .await?;
    Ok(result)
}

pub async fn find_by_filename(pool: Pool<Sqlite>, filename: &str) -> Result<Vec<Track>, Error> {
    let result: Vec<Track> =
        sqlx::query_as("SELECT * FROM track WHERE path = $1 ORDER BY title ASC")
            .bind(filename)
            .fetch_all(&pool)
            .await?;
    Ok(result)
}

pub async fn update_stream_metadata(
    pool: Pool<Sqlite>,
    md5: &str,
    title: &str,
    artist: &str,
    album: &str,
    length: u32,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE track SET title = $2, artist = $3, album = $4, length = $5, updated_at = $6 WHERE md5 = $1",
    )
    .bind(md5)
    .bind(title)
    .bind(artist)
    .bind(album)
    .bind(length)
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await?;
    Ok(())
}

pub async fn find_by_artist_album_date(
    pool: Pool<Sqlite>,
    artist: &str,
    album: &str,
    date: &str,
) -> Result<Vec<Track>, Error> {
    let result: Vec<Track> = sqlx::query_as(
        "SELECT * FROM track WHERE is_remote = 0 AND artist = $1 AND album = $2 AND year_string = $3 ORDER BY title ASC",
    )
    .bind(artist)
    .bind(album)
    .bind(date)
    .fetch_all(&pool)
    .await?;
    Ok(result)
}
