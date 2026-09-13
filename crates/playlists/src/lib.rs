pub mod resolver;
pub mod rules;

use anyhow::{anyhow, Result};
use chrono::Utc;
use rules::RuleCriteria;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistFolder {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub folder_id: Option<String>,
    pub track_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistTrack {
    pub id: String,
    pub playlist_id: String,
    pub track_id: String,
    pub position: i32,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartPlaylist {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub image: Option<String>,
    pub folder_id: Option<String>,
    pub is_system: bool,
    pub rules: RuleCriteria,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackStats {
    pub track_id: String,
    pub play_count: i64,
    pub skip_count: i64,
    pub last_played: Option<i64>,
    pub last_skipped: Option<i64>,
    pub updated_at: i64,
}

#[derive(Clone)]
pub struct PlaylistStore {
    pool: Pool<Sqlite>,
}

impl PlaylistStore {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    pub async fn seed(&self) -> Result<()> {
        self.seed_system_smart_playlists().await
    }

    // ── Folders ────────────────────────────────────────────────────────────

    pub async fn create_folder(&self, name: &str) -> Result<PlaylistFolder> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO playlist_folders (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(PlaylistFolder {
            id,
            name: name.to_string(),
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn list_folders(&self) -> Result<Vec<PlaylistFolder>> {
        let rows = sqlx::query(
            "SELECT id, name, created_at, updated_at FROM playlist_folders ORDER BY name ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| PlaylistFolder {
                id: r.get(0),
                name: r.get(1),
                created_at: r.get(2),
                updated_at: r.get(3),
            })
            .collect())
    }

    pub async fn get_folder(&self, id: &str) -> Result<Option<PlaylistFolder>> {
        let row = sqlx::query(
            "SELECT id, name, created_at, updated_at FROM playlist_folders WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| PlaylistFolder {
            id: r.get(0),
            name: r.get(1),
            created_at: r.get(2),
            updated_at: r.get(3),
        }))
    }

    pub async fn delete_folder(&self, id: &str) -> Result<bool> {
        sqlx::query("UPDATE saved_playlists SET folder_id = NULL WHERE folder_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        let result = sqlx::query("DELETE FROM playlist_folders WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Playlists ──────────────────────────────────────────────────────────

    pub async fn create(
        &self,
        name: &str,
        description: Option<&str>,
        image: Option<&str>,
        folder_id: Option<&str>,
    ) -> Result<Playlist> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO saved_playlists (id, name, description, image, folder_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(image)
        .bind(folder_id)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(Playlist {
            id,
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            image: image.map(|s| s.to_string()),
            folder_id: folder_id.map(|s| s.to_string()),
            track_count: 0,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn list(&self) -> Result<Vec<Playlist>> {
        let rows = sqlx::query(
            "SELECT p.id, p.name, p.description, p.image, p.folder_id,
                    (SELECT COUNT(*) FROM saved_playlist_tracks pt WHERE pt.playlist_id = p.id) AS track_count,
                    p.created_at, p.updated_at
             FROM saved_playlists p
             ORDER BY p.created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_playlist).collect())
    }

    pub async fn list_by_folder(&self, folder_id: &str) -> Result<Vec<Playlist>> {
        let rows = sqlx::query(
            "SELECT p.id, p.name, p.description, p.image, p.folder_id,
                    (SELECT COUNT(*) FROM saved_playlist_tracks pt WHERE pt.playlist_id = p.id) AS track_count,
                    p.created_at, p.updated_at
             FROM saved_playlists p
             WHERE p.folder_id = ?
             ORDER BY p.created_at DESC",
        )
        .bind(folder_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_playlist).collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<Playlist>> {
        let row = sqlx::query(
            "SELECT p.id, p.name, p.description, p.image, p.folder_id,
                    (SELECT COUNT(*) FROM saved_playlist_tracks pt WHERE pt.playlist_id = p.id) AS track_count,
                    p.created_at, p.updated_at
             FROM saved_playlists p WHERE p.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_playlist))
    }

    pub async fn update(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        image: Option<&str>,
        folder_id: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        sqlx::query(
            "UPDATE saved_playlists SET name = ?, description = ?, image = ?, folder_id = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(image)
        .bind(folder_id)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<bool> {
        sqlx::query("DELETE FROM saved_playlist_tracks WHERE playlist_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        let result = sqlx::query("DELETE FROM saved_playlists WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Tracks ─────────────────────────────────────────────────────────────

    pub async fn add_tracks(&self, playlist_id: &str, track_ids: &[String]) -> Result<()> {
        let max_pos: i32 = sqlx::query(
            "SELECT COALESCE(MAX(position), -1) FROM saved_playlist_tracks WHERE playlist_id = ?",
        )
        .bind(playlist_id)
        .fetch_one(&self.pool)
        .await
        .map(|r| r.get::<i32, _>(0))
        .unwrap_or(-1);

        for (i, track_id) in track_ids.iter().enumerate() {
            let id = Uuid::new_v4().to_string();
            let now = Utc::now().timestamp();
            let position = max_pos + 1 + i as i32;
            sqlx::query(
                "INSERT INTO saved_playlist_tracks (id, playlist_id, track_id, position, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(playlist_id)
            .bind(track_id)
            .bind(position)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn remove_track(&self, playlist_id: &str, track_id: &str) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM saved_playlist_tracks WHERE playlist_id = ? AND track_id = ?")
                .bind(playlist_id)
                .bind(track_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn get_track_ids(&self, playlist_id: &str) -> Result<Vec<String>> {
        let rows = sqlx::query(
            "SELECT track_id FROM saved_playlist_tracks WHERE playlist_id = ? ORDER BY position ASC",
        )
        .bind(playlist_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }

    // ── Smart playlists ────────────────────────────────────────────────────

    async fn seed_system_smart_playlists(&self) -> Result<()> {
        use serde_json::json;

        let defaults: &[(&str, &str, &str, serde_json::Value)] = &[
            (
                "sys_recently_added",
                "Recently Added",
                "Tracks added to your library in the last 30 days.",
                json!({ "match_type": "all", "conditions": [], "limit": 50,
                         "sort_by": "date_added", "sort_order": "DESC" }),
            ),
            (
                "sys_recently_played",
                "Recently Played",
                "Tracks you've listened to in the last 14 days.",
                json!({ "match_type": "all",
                  "conditions": [{"field":"last_played","operator":"in_last","value":14,"unit":"days"}],
                  "limit": 50, "sort_by": "last_played", "sort_order": "DESC" }),
            ),
            (
                "sys_rarely_played",
                "Rarely Played",
                "Tracks you've played fewer than 3 times.",
                json!({ "match_type": "all",
                  "conditions": [
                    {"field":"play_count","operator":"greater_than","value":0},
                    {"field":"play_count","operator":"less_than","value":3}
                  ],
                  "limit": 50, "sort_by": "play_count", "sort_order": "ASC" }),
            ),
            (
                "sys_most_played",
                "Most Played",
                "Your most listened-to tracks of all time.",
                json!({ "match_type": "all",
                  "conditions": [{"field":"play_count","operator":"greater_than","value":0}],
                  "limit": 50, "sort_by": "play_count", "sort_order": "DESC" }),
            ),
            (
                "sys_forgotten_favorites",
                "Forgotten Favorites",
                "Tracks you used to love but haven't played in over 6 months.",
                json!({ "match_type": "all",
                  "conditions": [
                    {"field":"play_count","operator":"greater_than","value":10},
                    {"field":"last_played","operator":"not_in_last","value":180,"unit":"days"}
                  ],
                  "limit": 50, "sort_by": "last_played", "sort_order": "ASC" }),
            ),
            (
                "sys_new_favorites",
                "New Favorites",
                "Recently added tracks you're already playing a lot.",
                json!({ "match_type": "all",
                  "conditions": [
                    {"field":"play_count","operator":"greater_than","value":5},
                    {"field":"date_added","operator":"in_last","value":90,"unit":"days"}
                  ],
                  "limit": 30, "sort_by": "play_count", "sort_order": "DESC" }),
            ),
            (
                "sys_old_but_gold",
                "Old But Gold",
                "Classic tracks from before 2000 that you keep coming back to.",
                json!({ "match_type": "all",
                  "conditions": [
                    {"field":"year","operator":"less_than","value":2000},
                    {"field":"play_count","operator":"greater_than","value":5}
                  ],
                  "limit": 50, "sort_by": "play_count", "sort_order": "DESC" }),
            ),
            (
                "sys_90s_mix",
                "90s Mix",
                "A shuffle of your 90s music.",
                json!({ "match_type": "all",
                  "conditions": [
                    {"field":"year","operator":"greater_than_or_equal","value":1990},
                    {"field":"year","operator":"less_than_or_equal","value":1999}
                  ],
                  "limit": 50, "sort_by": "random", "sort_order": "DESC" }),
            ),
            (
                "sys_discovery_queue",
                "Discovery Queue",
                "Tracks in your library you've never played.",
                json!({ "match_type": "all",
                  "conditions": [{"field":"play_count","operator":"equals","value":0}],
                  "limit": 50, "sort_by": "date_added", "sort_order": "DESC" }),
            ),
            (
                "sys_rediscover",
                "Rediscover",
                "Tracks you've played before but not for over 3 months.",
                json!({ "match_type": "all",
                  "conditions": [
                    {"field":"play_count","operator":"greater_than","value":0},
                    {"field":"last_played","operator":"not_in_last","value":90,"unit":"days"}
                  ],
                  "limit": 50, "sort_by": "random", "sort_order": "DESC" }),
            ),
            (
                "sys_skipped_too_often",
                "Skipped Too Often",
                "Tracks you keep skipping — maybe time to remove them?",
                json!({ "match_type": "all",
                  "conditions": [{"field":"skip_count","operator":"greater_than","value":3}],
                  "limit": 50, "sort_by": "skip_count", "sort_order": "DESC" }),
            ),
            (
                "sys_loved_rarely_played",
                "Loved But Rarely Played",
                "Tracks you've liked but haven't played much.",
                json!({ "match_type": "all",
                  "conditions": [
                    {"field":"is_liked","operator":"is","value":true},
                    {"field":"play_count","operator":"less_than","value":5}
                  ],
                  "limit": 50, "sort_by": "date_added", "sort_order": "DESC" }),
            ),
            (
                "sys_smart_daily_mix",
                "Smart Daily Mix",
                "A curated daily mix based on your listening habits.",
                json!({ "match_type": "any",
                  "conditions": [
                    {"field":"last_played","operator":"in_last","value":30,"unit":"days"},
                    {"field":"play_count","operator":"equals","value":0},
                    {"field":"play_count","operator":"greater_than","value":10}
                  ],
                  "limit": 25, "sort_by": "random", "sort_order": "DESC" }),
            ),
        ];

        let now = Utc::now().timestamp();
        for (id, name, description, rules) in defaults {
            let count: i64 = sqlx::query("SELECT COUNT(*) FROM smart_playlists WHERE id = ?")
                .bind(*id)
                .fetch_one(&self.pool)
                .await
                .map(|r| r.get(0))
                .unwrap_or(0);
            if count > 0 {
                continue;
            }
            sqlx::query(
                "INSERT INTO smart_playlists
                 (id, name, description, image, folder_id, is_system, rules, created_at, updated_at)
                 VALUES (?, ?, ?, NULL, NULL, 1, ?, ?, ?)",
            )
            .bind(*id)
            .bind(*name)
            .bind(*description)
            .bind(rules.to_string())
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn list_smart_playlists(&self) -> Result<Vec<SmartPlaylist>> {
        let rows = sqlx::query(
            "SELECT id, name, description, image, folder_id, is_system, rules, created_at, updated_at
             FROM smart_playlists ORDER BY is_system DESC, name ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_smart_playlist).collect())
    }

    pub async fn get_smart_playlist(&self, id: &str) -> Result<Option<SmartPlaylist>> {
        let row = sqlx::query(
            "SELECT id, name, description, image, folder_id, is_system, rules, created_at, updated_at
             FROM smart_playlists WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_smart_playlist))
    }

    pub async fn create_smart_playlist(
        &self,
        name: &str,
        description: Option<&str>,
        image: Option<&str>,
        folder_id: Option<&str>,
        rules: &RuleCriteria,
    ) -> Result<SmartPlaylist> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().timestamp();
        let rules_json = serde_json::to_string(rules)?;
        sqlx::query(
            "INSERT INTO smart_playlists
             (id, name, description, image, folder_id, is_system, rules, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(description)
        .bind(image)
        .bind(folder_id)
        .bind(&rules_json)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(SmartPlaylist {
            id,
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            image: image.map(|s| s.to_string()),
            folder_id: folder_id.map(|s| s.to_string()),
            is_system: false,
            rules: rules.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn update_smart_playlist(
        &self,
        id: &str,
        name: &str,
        description: Option<&str>,
        image: Option<&str>,
        folder_id: Option<&str>,
        rules: &RuleCriteria,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        let rules_json = serde_json::to_string(rules)?;
        let result = sqlx::query(
            "UPDATE smart_playlists
             SET name = ?, description = ?, image = ?, folder_id = ?, rules = ?, updated_at = ?
             WHERE id = ? AND is_system = 0",
        )
        .bind(name)
        .bind(description)
        .bind(image)
        .bind(folder_id)
        .bind(&rules_json)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;
        if result.rows_affected() == 0 {
            return Err(anyhow!("Smart playlist not found or is a system playlist"));
        }
        Ok(())
    }

    pub async fn delete_smart_playlist(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM smart_playlists WHERE id = ? AND is_system = 0")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ── Track stats ────────────────────────────────────────────────────────

    pub async fn record_play(&self, track_id: &str) -> Result<()> {
        self.record_listen(track_id, false, 0, 0).await
    }

    pub async fn record_skip(&self, track_id: &str) -> Result<()> {
        self.record_listen(track_id, true, 0, 0).await
    }

    /// One listen: bumps the `track_stats` counters and appends to the
    /// `play_history` log. The counters are the fast path every smart-playlist
    /// rule reads; the log is what makes "this week" answerable at all —
    /// a single overwritten row cannot say *when*.
    ///
    /// `ms_played` / `length_ms` may be zero when the caller does not know
    /// them (the counters-only callers above); the log keeps whatever was
    /// known at the time.
    pub async fn record_listen(
        &self,
        track_id: &str,
        skipped: bool,
        ms_played: u64,
        length_ms: u64,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        if skipped {
            sqlx::query(
                "INSERT INTO track_stats (track_id, play_count, skip_count, last_played, last_skipped, updated_at)
                 VALUES (?, 0, 1, NULL, ?, ?)
                 ON CONFLICT(track_id) DO UPDATE SET
                   skip_count = skip_count + 1,
                   last_skipped = excluded.last_skipped,
                   updated_at = excluded.updated_at",
            )
            .bind(track_id)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "INSERT INTO track_stats (track_id, play_count, skip_count, last_played, last_skipped, updated_at)
                 VALUES (?, 1, 0, ?, NULL, ?)
                 ON CONFLICT(track_id) DO UPDATE SET
                   play_count = play_count + 1,
                   last_played = excluded.last_played,
                   updated_at = excluded.updated_at",
            )
            .bind(track_id)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        // Best-effort append: history is an analytics record, and losing one
        // row of it must never fail the play that produced it.
        if let Err(e) = sqlx::query(
            "INSERT INTO play_history (track_id, played_at, ms_played, length_ms, skipped)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(track_id)
        .bind(now)
        .bind(ms_played as i64)
        .bind(length_ms as i64)
        .bind(skipped as i32)
        .execute(&self.pool)
        .await
        {
            tracing::warn!("play_history append failed for {track_id}: {e}");
        }
        Ok(())
    }

    pub async fn get_track_stats(&self, track_id: &str) -> Result<Option<TrackStats>> {
        let row = sqlx::query(
            "SELECT track_id, play_count, skip_count, last_played, last_skipped, updated_at
             FROM track_stats WHERE track_id = ?",
        )
        .bind(track_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_track_stats))
    }

    pub async fn get_all_track_stats(&self) -> Result<Vec<TrackStats>> {
        let rows = sqlx::query(
            "SELECT track_id, play_count, skip_count, last_played, last_skipped, updated_at
             FROM track_stats",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_track_stats).collect())
    }
}

fn row_to_playlist(r: sqlx::sqlite::SqliteRow) -> Playlist {
    Playlist {
        id: r.get(0),
        name: r.get(1),
        description: r.get(2),
        image: r.get(3),
        folder_id: r.get(4),
        track_count: r.get(5),
        created_at: r.get(6),
        updated_at: r.get(7),
    }
}

fn row_to_smart_playlist(r: sqlx::sqlite::SqliteRow) -> SmartPlaylist {
    let is_system: i64 = r.get(5);
    let rules_str: String = r.get(6);
    SmartPlaylist {
        id: r.get(0),
        name: r.get(1),
        description: r.get(2),
        image: r.get(3),
        folder_id: r.get(4),
        is_system: is_system != 0,
        rules: serde_json::from_str(&rules_str).unwrap_or_default(),
        created_at: r.get(7),
        updated_at: r.get(8),
    }
}

fn row_to_track_stats(r: sqlx::sqlite::SqliteRow) -> TrackStats {
    TrackStats {
        track_id: r.get(0),
        play_count: r.get(1),
        skip_count: r.get(2),
        last_played: r.get(3),
        last_skipped: r.get(4),
        updated_at: r.get(5),
    }
}
