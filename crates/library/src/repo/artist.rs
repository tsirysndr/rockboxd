use crate::entity::artist::Artist;
use sqlx::{Error, Pool, Sqlite};

pub async fn save(pool: Pool<Sqlite>, artist: Artist) -> Result<String, Error> {
    if artist.name.is_empty() {
        return Err(Error::ColumnNotFound("name".to_string()));
    }

    match sqlx::query(
        r#"
        INSERT INTO artist (
          id,
          name,
          bio,
          image
        )
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(&artist.id)
    .bind(&artist.name)
    .bind(&artist.bio)
    .bind(&artist.image)
    .execute(&pool)
    .await
    {
        Ok(_) => Ok(artist.id.clone()),
        Err(_e) => {
            // eprintln!("Error saving artist: {:?}", e);
            // get the artist by name and return the id
            let artist = find_by_name(pool.clone(), &artist.name).await?;
            Ok(artist.unwrap().id)
        }
    }
}

pub async fn find_by_name(pool: Pool<Sqlite>, name: &str) -> Result<Option<Artist>, Error> {
    match sqlx::query_as::<_, Artist>(
        r#"
        SELECT * FROM artist WHERE name = $1
        "#,
    )
    .bind(name)
    .fetch_optional(&pool)
    .await
    {
        Ok(artist) => Ok(artist),
        Err(e) => {
            eprintln!("Error finding artist: {:?}", e);
            Err(e)
        }
    }
}

pub async fn find(pool: Pool<Sqlite>, id: &str) -> Result<Option<Artist>, Error> {
    match sqlx::query_as::<_, Artist>(
        r#"
        SELECT * FROM artist WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    {
        Ok(artist) => Ok(artist),
        Err(e) => {
            eprintln!("Error finding artist: {:?}", e);
            Err(e)
        }
    }
}

pub async fn filter(
    pool: Pool<Sqlite>,
    r#where: (String, Vec<String>),
) -> Result<Vec<Artist>, Error> {
    let sql = format!("SELECT * FROM artist WHERE {}", r#where.0);
    let mut query = sqlx::query_as(&sql);

    for value in r#where.1 {
        query = query.bind(value.clone());
    }

    let result = query.fetch_all(&pool).await?;
    Ok(result)
}

pub async fn all(pool: Pool<Sqlite>) -> Result<Vec<Artist>, Error> {
    match sqlx::query_as::<_, Artist>(
        r#"
        SELECT * FROM artist WHERE EXISTS (
          SELECT 1 FROM track WHERE track.artist_id = artist.id AND track.is_remote = 0
        ) ORDER BY name ASC
        "#,
    )
    .fetch_all(&pool)
    .await
    {
        Ok(artists) => Ok(artists),
        Err(e) => {
            eprintln!("Error finding artists: {:?}", e);
            Err(e)
        }
    }
}

/// Paginated artist list narrowed by Jellyfin's alpha-jump filter params.
/// `name_starts_with` does a case-insensitive prefix match; the `_or_greater`
/// and `_less_than` variants compare the first character to bracket a letter
/// range. All three compose. None set → returns the full library page.
pub async fn filtered(
    pool: Pool<Sqlite>,
    name_starts_with: Option<&str>,
    name_starts_with_or_greater: Option<&str>,
    name_less_than: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<Artist>, Error> {
    let (where_sql, binds) = super::name_filter::sql(
        "name",
        name_starts_with,
        name_starts_with_or_greater,
        name_less_than,
    );
    let limit_idx = binds.len() + 1;
    let offset_idx = binds.len() + 2;
    let sql = format!(
        "SELECT * FROM artist WHERE EXISTS (
           SELECT 1 FROM track WHERE track.artist_id = artist.id AND track.is_remote = 0
         ) {extra}
         ORDER BY name COLLATE NOCASE LIMIT ?{limit_idx} OFFSET ?{offset_idx}",
        extra = if where_sql.is_empty() {
            String::new()
        } else {
            format!("AND {}", where_sql.trim_start_matches("WHERE "))
        }
    );
    let mut q = sqlx::query_as::<_, Artist>(&sql);
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
        "name",
        name_starts_with,
        name_starts_with_or_greater,
        name_less_than,
    );
    let sql = format!(
        "SELECT COUNT(*) FROM artist WHERE EXISTS (
           SELECT 1 FROM track WHERE track.artist_id = artist.id AND track.is_remote = 0
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

pub async fn name_prefixes(pool: Pool<Sqlite>) -> Result<Vec<String>, Error> {
    super::name_filter::prefixes(
        &pool,
        "artist",
        "name",
        Some("EXISTS (SELECT 1 FROM track WHERE track.artist_id = artist.id AND track.is_remote = 0)"),
    )
    .await
}

pub async fn update_picture(pool: &Pool<Sqlite>, id: &str, picture: &str) -> Result<(), Error> {
    match sqlx::query(
        r#"
        UPDATE artist SET image = $2 WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(picture)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error updating artist picture: {:?}", e);
            Err(e)
        }
    }
}

pub async fn update_genres(pool: &Pool<Sqlite>, id: &str, genres: &str) -> Result<(), Error> {
    match sqlx::query(
        r#"
        UPDATE artist SET genres = $2 WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(genres)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error updating artist genres: {:?}", e);
            Err(e)
        }
    }
}

/// Drop artists left with neither a track nor an album, plus the dangling
/// `artist_tracks` / `artist_genres` rows. Run this *after*
/// [`super::album::delete_orphans`] so albums of a fully-deleted artist are
/// already gone — otherwise the album check keeps the artist alive.
/// Returns the number of `artist` rows removed.
pub async fn delete_orphans(pool: Pool<Sqlite>) -> Result<u64, Error> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        "DELETE FROM artist_tracks
         WHERE NOT EXISTS (SELECT 1 FROM track WHERE track.id = artist_tracks.track_id)",
    )
    .execute(&mut *tx)
    .await?;

    let deleted = sqlx::query(
        "DELETE FROM artist
         WHERE NOT EXISTS (SELECT 1 FROM track WHERE track.artist_id = artist.id)
           AND NOT EXISTS (SELECT 1 FROM album WHERE album.artist_id = artist.id)",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    sqlx::query(
        "DELETE FROM artist_genres
         WHERE NOT EXISTS (SELECT 1 FROM artist WHERE artist.id = artist_genres.artist_id)",
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(deleted)
}

pub async fn save_artist_genre(
    pool: &Pool<Sqlite>,
    id: &str,
    artist_id: &str,
    genre_id: &str,
) -> Result<(), Error> {
    match sqlx::query(
        r#"
        INSERT OR IGNORE INTO artist_genres (id, artist_id, genre_id) VALUES ($1, $2, $3)
        "#,
    )
    .bind(id)
    .bind(artist_id)
    .bind(genre_id)
    .execute(pool)
    .await
    {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Error saving artist genre: {:?}", e);
            Err(e)
        }
    }
}
