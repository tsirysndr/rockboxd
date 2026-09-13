//! Listening analytics, read from the views the library migration defines
//! (`v_most_played`, `v_most_skipped`, `v_never_played`, `v_recently_added`,
//! `v_recently_played`). The views are the contract: this module adds only
//! pagination, so every other consumer of the database sees the same numbers.

use actix_web::{error::ErrorInternalServerError, web, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::http::AppState;

type HandlerResult = actix_web::Result<HttpResponse>;

#[derive(Deserialize)]
pub struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}

impl Page {
    fn limit(&self) -> i64 {
        self.limit.unwrap_or(100).clamp(1, 1000)
    }
    fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

#[derive(Serialize)]
struct StatRow {
    track_id: String,
    title: String,
    artist: String,
    album: String,
    /// Play count, skip count, or absent — depending on the view.
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<i64>,
    /// Unix seconds; last_played / last_skipped / created_at per view.
    #[serde(skip_serializing_if = "Option::is_none")]
    at: Option<i64>,
    /// ISO timestamp views (recently added) — kept as stored.
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
}

async fn view_rows(
    state: &AppState,
    sql: &str,
    page: &Page,
    map: impl Fn(sqlx::sqlite::SqliteRow) -> StatRow,
) -> actix_web::Result<Vec<StatRow>> {
    sqlx::query(sql)
        .bind(page.limit())
        .bind(page.offset())
        .fetch_all(&state.pool)
        .await
        .map(|rows| rows.into_iter().map(map).collect())
        .map_err(ErrorInternalServerError)
}

pub async fn most_played(state: web::Data<AppState>, q: web::Query<Page>) -> HandlerResult {
    let rows = view_rows(
        &state,
        "SELECT track_id, title, artist, album, play_count, last_played \
         FROM v_most_played LIMIT ? OFFSET ?",
        &q,
        |r| StatRow {
            track_id: r.get(0),
            title: r.get(1),
            artist: r.get(2),
            album: r.get(3),
            count: Some(r.get(4)),
            at: r.get(5),
            created_at: None,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

pub async fn most_skipped(state: web::Data<AppState>, q: web::Query<Page>) -> HandlerResult {
    let rows = view_rows(
        &state,
        "SELECT track_id, title, artist, album, skip_count, last_skipped \
         FROM v_most_skipped LIMIT ? OFFSET ?",
        &q,
        |r| StatRow {
            track_id: r.get(0),
            title: r.get(1),
            artist: r.get(2),
            album: r.get(3),
            count: Some(r.get(4)),
            at: r.get(5),
            created_at: None,
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

pub async fn never_played(state: web::Data<AppState>, q: web::Query<Page>) -> HandlerResult {
    let rows = view_rows(
        &state,
        "SELECT track_id, title, artist, album, created_at \
         FROM v_never_played LIMIT ? OFFSET ?",
        &q,
        |r| StatRow {
            track_id: r.get(0),
            title: r.get(1),
            artist: r.get(2),
            album: r.get(3),
            count: None,
            at: None,
            created_at: r.get(4),
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

pub async fn recently_added(state: web::Data<AppState>, q: web::Query<Page>) -> HandlerResult {
    let rows = view_rows(
        &state,
        "SELECT track_id, title, artist, album, created_at \
         FROM v_recently_added LIMIT ? OFFSET ?",
        &q,
        |r| StatRow {
            track_id: r.get(0),
            title: r.get(1),
            artist: r.get(2),
            album: r.get(3),
            count: None,
            at: None,
            created_at: r.get(4),
        },
    )
    .await?;
    Ok(HttpResponse::Ok().json(rows))
}

#[derive(Serialize)]
struct HistoryRow {
    track_id: String,
    title: String,
    artist: String,
    album: String,
    played_at: i64,
    ms_played: i64,
    length_ms: i64,
    skipped: bool,
}

/// The listen log itself, newest first — one row per play, not per track.
pub async fn recently_played(state: web::Data<AppState>, q: web::Query<Page>) -> HandlerResult {
    let rows = sqlx::query(
        "SELECT track_id, title, artist, album, played_at, ms_played, length_ms, skipped \
         FROM v_recently_played LIMIT ? OFFSET ?",
    )
    .bind(q.limit())
    .bind(q.offset())
    .fetch_all(&state.pool)
    .await
    .map_err(ErrorInternalServerError)?;
    let rows: Vec<HistoryRow> = rows
        .into_iter()
        .map(|r| HistoryRow {
            track_id: r.get(0),
            title: r.get(1),
            artist: r.get(2),
            album: r.get(3),
            played_at: r.get(4),
            ms_played: r.get(5),
            length_ms: r.get(6),
            skipped: r.get::<i64, _>(7) != 0,
        })
        .collect();
    Ok(HttpResponse::Ok().json(rows))
}
