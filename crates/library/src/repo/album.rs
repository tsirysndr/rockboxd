use crate::entity::album::Album;
use sqlx::{Pool, Sqlite};
use tracing::warn;

pub async fn save(pool: Pool<Sqlite>, album: Album) -> Result<String, sqlx::Error> {
    match sqlx::query(
        r#"
        INSERT INTO album (
          id,
          title,
          artist,
          year,
          year_string,
          album_art,
          md5,
          artist_id,
          label,
          copyright_message
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(&album.id)
    .bind(&album.title)
    .bind(&album.artist)
    .bind(album.year)
    .bind(&album.year_string)
    .bind(&album.album_art)
    .bind(&album.md5)
    .bind(&album.artist_id)
    .bind(album.label)
    .bind(album.copyright_message)
    .execute(&pool)
    .await
    {
        Ok(_) => Ok(album.id.clone()),
        Err(_e) => {
            // eprintln!("Error saving album: {:?}", e);
            let album = find_by_md5(pool.clone(), &album.md5).await?;
            Ok(album.unwrap().id)
        }
    }
}

pub async fn filter(
    pool: Pool<Sqlite>,
    r#where: (String, Vec<String>),
) -> Result<Vec<Album>, sqlx::Error> {
    let sql = format!("SELECT * FROM album WHERE {}", r#where.0);
    let mut query = sqlx::query_as(&sql);

    for value in r#where.1 {
        query = query.bind(value.clone());
    }

    let result = query.fetch_all(&pool).await?;
    Ok(result)
}

pub async fn find_by_md5(pool: Pool<Sqlite>, md5: &str) -> Result<Option<Album>, sqlx::Error> {
    match sqlx::query_as::<_, Album>(
        r#"
        SELECT * FROM album WHERE md5 = $1
        "#,
    )
    .bind(md5)
    .fetch_optional(&pool)
    .await
    {
        Ok(album) => Ok(album),
        Err(e) => {
            warn!("Error finding album: {:?}", e);
            Err(e)
        }
    }
}

pub async fn find_by_artist(
    pool: Pool<Sqlite>,
    artist_id: &str,
) -> Result<Vec<Album>, sqlx::Error> {
    // Match albums where the artist is either the primary album artist
    // OR appears as a track artist on any track in the album.
    match sqlx::query_as::<_, Album>(
        r#"
        SELECT DISTINCT album.* FROM album
        WHERE (
            album.artist_id = $1
            OR EXISTS (
                SELECT 1 FROM track
                WHERE track.album_id = album.id
                  AND track.artist_id = $1
                  AND track.is_remote = 0
            )
        )
        AND EXISTS (
            SELECT 1 FROM track WHERE track.album_id = album.id AND track.is_remote = 0
        )
        ORDER BY album.title ASC
        "#,
    )
    .bind(artist_id)
    .fetch_all(&pool)
    .await
    {
        Ok(albums) => Ok(albums),
        Err(e) => {
            warn!("Error finding albums by artist: {:?}", e);
            Err(e)
        }
    }
}

/// Returns a map of artist_id → number of distinct albums that artist appears in,
/// either as primary album artist or as a track artist.
pub async fn count_by_artist(
    pool: Pool<Sqlite>,
) -> Result<std::collections::HashMap<String, usize>, sqlx::Error> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT artist_id, COUNT(DISTINCT album_id) as cnt FROM (
            SELECT album.artist_id, album.id AS album_id FROM album
            WHERE EXISTS (
                SELECT 1 FROM track WHERE track.album_id = album.id AND track.is_remote = 0
            )
            UNION
            SELECT track.artist_id, track.album_id FROM track
            WHERE track.is_remote = 0 AND track.artist_id != ''
        )
        GROUP BY artist_id
        "#,
    )
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, cnt)| (id, cnt as usize))
        .collect())
}

pub async fn find(pool: Pool<Sqlite>, id: &str) -> Result<Option<Album>, sqlx::Error> {
    match sqlx::query_as::<_, Album>(
        r#"
        SELECT * FROM album WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    {
        Ok(album) => Ok(album),
        Err(e) => {
            warn!("Error finding album: {:?}", e);
            Err(e)
        }
    }
}

pub async fn all(pool: Pool<Sqlite>) -> Result<Vec<Album>, sqlx::Error> {
    match sqlx::query_as::<_, Album>(
        r#"
        SELECT * FROM album WHERE EXISTS (
          SELECT 1 FROM track WHERE track.album_id = album.id AND track.is_remote = 0
        ) ORDER BY title ASC
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(albums) => Ok(albums),
        Err(e) => {
            warn!("Error finding albums: {:?}", e);
            Err(e)
        }
    }
}

/// Paginated album list narrowed by Jellyfin's alpha-jump filter params —
/// see [`super::artist::filtered`] for the parameter semantics.
pub async fn filtered(
    pool: Pool<Sqlite>,
    name_starts_with: Option<&str>,
    name_starts_with_or_greater: Option<&str>,
    name_less_than: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Album>, sqlx::Error> {
    let (where_sql, binds) = super::name_filter::sql(
        "title",
        name_starts_with,
        name_starts_with_or_greater,
        name_less_than,
    );
    let limit_idx = binds.len() + 1;
    let offset_idx = binds.len() + 2;
    let sql = format!(
        "SELECT * FROM album WHERE EXISTS (
           SELECT 1 FROM track WHERE track.album_id = album.id AND track.is_remote = 0
         ) {extra}
         ORDER BY title COLLATE NOCASE LIMIT ?{limit_idx} OFFSET ?{offset_idx}",
        extra = if where_sql.is_empty() {
            String::new()
        } else {
            format!("AND {}", where_sql.trim_start_matches("WHERE "))
        }
    );
    let mut q = sqlx::query_as::<_, Album>(&sql);
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
) -> Result<i64, sqlx::Error> {
    let (where_sql, binds) = super::name_filter::sql(
        "title",
        name_starts_with,
        name_starts_with_or_greater,
        name_less_than,
    );
    let sql = format!(
        "SELECT COUNT(*) FROM album WHERE EXISTS (
           SELECT 1 FROM track WHERE track.album_id = album.id AND track.is_remote = 0
         ) {extra}",
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

pub async fn name_prefixes(pool: Pool<Sqlite>) -> Result<Vec<String>, sqlx::Error> {
    super::name_filter::prefixes(
        &pool,
        "album",
        "title",
        Some(
            "EXISTS (SELECT 1 FROM track WHERE track.album_id = album.id AND track.is_remote = 0)",
        ),
    )
    .await
}

/// Drop albums that no longer have a single track, plus any `album_tracks`
/// row pointing at a track that is gone. Albums are only ever created by the
/// scanner (`audio_scan::save_audio_metadata`), so "no tracks" always means
/// "leftover from deleted files" — never a legitimately empty album.
/// Returns the number of `album` rows removed.
pub async fn delete_orphans(pool: Pool<Sqlite>) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM album_tracks
         WHERE NOT EXISTS (SELECT 1 FROM track WHERE track.id = album_tracks.track_id)",
    )
    .execute(&mut *tx)
    .await?;

    let deleted = sqlx::query(
        "DELETE FROM album
         WHERE NOT EXISTS (SELECT 1 FROM track WHERE track.album_id = album.id)",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(deleted)
}

pub async fn update_album_art(
    pool: Pool<Sqlite>,
    id: &str,
    album_art: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE album SET album_art = $2 WHERE id = $1")
        .bind(id)
        .bind(album_art)
        .execute(&pool)
        .await?;
    Ok(())
}
