use std::env;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Error, Executor, Pool, Sqlite,
};
use tracing::{debug, info, warn};

pub mod album_art;
pub mod artists;
pub mod audio_scan;
pub mod copyright_message;
pub mod entity;
pub mod genres;
pub mod label;
pub mod repo;
pub mod watcher;

pub async fn create_connection_pool() -> Result<Pool<Sqlite>, Error> {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    let rockbox_dir = format!("{}/.config/rockbox.org", home);
    let _ = std::fs::create_dir_all(&rockbox_dir);
    let rockbox_db_path = format!("{}/rockbox-library.db", rockbox_dir);
    let db_url = env::var("DATABASE_URL").unwrap_or(rockbox_db_path);
    debug!("db url {}", db_url);
    // Do NOT call env::set_var here — it races with other threads on macOS.
    let options = SqliteConnectOptions::new()
        .filename(db_url)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(30));
    // Every server thread (REST, GraphQL, Subsonic, Jellyfin, broker, …) builds
    // its own pool, and a WAL connection costs three fds (`.db`, `-wal`,
    // `-shm`). sqlx's default of 10 per pool therefore burns ~180 descriptors
    // on a daemon that is only ever serving a handful of concurrent queries.
    // Override with ROCKBOX_DB_MAX_CONNECTIONS.
    let max_connections = env::var("ROCKBOX_DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(4);
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await?;
    pool.execute(include_str!(
        "../migrations/20240923093823_create_tables.sql"
    ))
    .await?;
    match pool
        .execute(include_str!(
            "../migrations/20241011011557_add_artist_id_column.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("artist_id column already exists"),
    }

    match pool
        .execute(include_str!(
            "../migrations/20241020125757_add-album_id-column.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("album_id column already exists"),
    }
    match pool
        .execute(include_str!(
            "../migrations/20251218042124_add_album_label.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("label column already exists"),
    }

    match pool
        .execute(include_str!(
            "../migrations/20251218044147_add_album_copyright_message.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("copyright_message column already exists"),
    }

    match pool
        .execute(include_str!(
            "../migrations/20251218173111_add_artist_genres.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("genres column already exists"),
    }

    match pool
        .execute(include_str!(
            "../migrations/20260425000000_add_playlist_tables.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("playlist tables already exist"),
    }

    match pool
        .execute(include_str!(
            "../migrations/20260428000000_add_is_remote_to_track.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("is_remote column already exists"),
    }

    /*
    pool.execute(include_str!(
        "../migrations/20260501000000_fix_datetime_formats.sql"
    ))
    .await?;
    */

    // dedupe_genres modifies schema (DROP TABLE + RENAME) so it must run
    // synchronously before we return the pool. The skip guard makes it a
    // no-op O(1) check on every subsequent startup.
    let genre_unique: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='genre' AND sql LIKE '%UNIQUE%')",
    )
    .fetch_one(&pool)
    .await
    .unwrap_or(false);
    if !genre_unique {
        info!("Applying dedupe_genres migration...");
        match pool
            .execute(include_str!(
                "../migrations/20260504000000_dedupe_genres.sql"
            ))
            .await
        {
            Ok(_) => info!("dedupe_genres migration applied"),
            Err(e) => warn!("dedupe_genres migration: {}", e),
        }
    }

    // FTS5 index migration: safe to run in the background — triggers keep
    // it in sync after initial creation, and the skip guard is O(1) on
    // subsequent startups so the task exits almost immediately then.
    let bg_pool = pool.clone();
    tokio::spawn(async move {
        let fts_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='track_fts')")
                .fetch_one(&bg_pool)
                .await
                .unwrap_or(false);
        if !fts_exists {
            info!("Background: applying fts5 search migration...");
            match bg_pool
                .execute(include_str!(
                    "../migrations/20260503000000_add_fts5_search.sql"
                ))
                .await
            {
                Ok(_) => info!("Background: fts5 search migration applied"),
                Err(e) => warn!("Background: fts5 migration: {}", e),
            }
        }
    });

    Ok(pool)
}
