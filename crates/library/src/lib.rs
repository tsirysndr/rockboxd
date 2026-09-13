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

    // Idempotent by construction (IF NOT EXISTS throughout), so it runs
    // unconditionally.
    match pool
        .execute(include_str!(
            "../migrations/20260913000000_add_play_history.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(e) => warn!("play_history migration: {}", e),
    }

    // Key/BPM columns for the audio analysis pass. Plain ALTERs, so they fail
    // harmlessly once applied.
    match pool
        .execute(include_str!("../migrations/20260913000001_add_key_bpm.sql"))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("key/bpm columns already exist"),
    }

    match pool
        .execute(include_str!(
            "../migrations/20260913000002_add_waveform.sql"
        ))
        .await
    {
        Ok(_) => {}
        Err(_) => warn!("waveform column already exists"),
    }

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

#[cfg(test)]
mod analytics_tests {
    use sqlx::{Executor, Row, SqlitePool};

    /// The real schema + the analytics migration, on a fresh in-memory DB.
    async fn db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(include_str!(
            "../migrations/20240923093823_create_tables.sql"
        ))
        .await
        .unwrap();
        pool.execute(include_str!(
            "../migrations/20260425000000_add_playlist_tables.sql"
        ))
        .await
        .unwrap();
        pool.execute(include_str!(
            "../migrations/20260913000000_add_play_history.sql"
        ))
        .await
        .unwrap();
        pool
    }

    async fn add_track(pool: &SqlitePool, id: &str, title: &str) {
        sqlx::query(
            "INSERT INTO track (id, path, title, artist, album, album_artist, bitrate, \
             composer, disc_number, filesize, frequency, length, md5, artist_id, album_id, \
             created_at, updated_at) \
             VALUES (?, ?, ?, 'Artist', 'Album', 'Artist', 320, '', 1, 1000, 44100, 200000, \
             ?, 'ar1', 'al1', '2026-01-01T00:00:00+00:00', '2026-01-01T00:00:00+00:00')",
        )
        .bind(id)
        .bind(format!("/music/{id}.mp3"))
        .bind(title)
        .bind(format!("md5-{id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    async fn stat(pool: &SqlitePool, id: &str, plays: i64, skips: i64) {
        sqlx::query(
            "INSERT INTO track_stats (track_id, play_count, skip_count, last_played, last_skipped, updated_at) \
             VALUES (?, ?, ?, 100, 100, 100)",
        )
        .bind(id)
        .bind(plays)
        .bind(skips)
        .execute(pool)
        .await
        .unwrap();
    }

    /// The views are the analytics contract every API reads; this pins their
    /// semantics against the real migration SQL rather than a re-statement.
    #[tokio::test]
    async fn the_views_agree_on_who_was_played() {
        let pool = db().await;
        add_track(&pool, "a", "Played").await;
        add_track(&pool, "b", "Skipped only").await;
        add_track(&pool, "c", "Untouched").await;
        stat(&pool, "a", 5, 0).await;
        stat(&pool, "b", 0, 3).await;

        let most: Vec<String> = sqlx::query("SELECT track_id FROM v_most_played")
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        assert_eq!(most, vec!["a"], "only the played track charts");

        let skipped: Vec<String> = sqlx::query("SELECT track_id FROM v_most_skipped")
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        assert_eq!(skipped, vec!["b"]);

        // A track skipped ten times has still never been *listened* to: the
        // never-played view must include both the skipped and the untouched.
        let mut never: Vec<String> = sqlx::query("SELECT track_id FROM v_never_played")
            .fetch_all(&pool)
            .await
            .unwrap()
            .iter()
            .map(|r| r.get(0))
            .collect();
        never.sort();
        assert_eq!(never, vec!["b", "c"]);
    }

    /// One row per listen: history is a log, not a counter.
    #[tokio::test]
    async fn recently_played_lists_every_listen() {
        let pool = db().await;
        add_track(&pool, "a", "On repeat").await;
        for played_at in [100, 200, 300] {
            sqlx::query(
                "INSERT INTO play_history (track_id, played_at, ms_played, length_ms, skipped) \
                 VALUES ('a', ?, 180000, 200000, 0)",
            )
            .bind(played_at)
            .execute(&pool)
            .await
            .unwrap();
        }

        let rows = sqlx::query("SELECT played_at FROM v_recently_played")
            .fetch_all(&pool)
            .await
            .unwrap();
        let times: Vec<i64> = rows.iter().map(|r| r.get(0)).collect();
        assert_eq!(times, vec![300, 200, 100], "newest first, one per listen");
    }
}
