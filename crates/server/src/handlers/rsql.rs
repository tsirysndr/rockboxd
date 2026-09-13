//! RSQL filter endpoints: `POST /rsql/tracks|albums|artists`.
//!
//! The body is an rsql `QuerySpec` — filter, sort, limit — and the response is
//! the matching library rows in full:
//!
//! ```json
//! { "filter": "genre==rock;playcount>5;lastplayed=lt=30d",
//!   "sort_by": "playcount", "sort_order": "desc", "limit": 100 }
//! ```
//!
//! The filter compiles through `rockbox_rsql`, which resolves field names to
//! fixed column expressions and binds every literal as a parameter — user
//! input never reaches the SQL text. A filter that does not parse is the
//! caller's mistake, so it comes back as 400 with the parser's message rather
//! than a 500.

use actix_web::{
    error::{ErrorBadRequest, ErrorInternalServerError},
    web, HttpResponse,
};
use rockbox_library::repo;
use rockbox_rsql::{build, schema, QuerySpec, Value};

use crate::http::AppState;

type HandlerResult = actix_web::Result<HttpResponse>;

/// The matching ids for `spec` against `schema_name`, in query order.
async fn matching_ids(
    state: &AppState,
    schema_name: &str,
    spec: &QuerySpec,
) -> actix_web::Result<Vec<String>> {
    let schema = schema::by_name(schema_name)
        .ok_or_else(|| ErrorInternalServerError(format!("no schema {schema_name}")))?;
    let query = build(spec, schema).map_err(|e| ErrorBadRequest(e.to_string()))?;

    let mut q = sqlx::query_scalar::<_, String>(&query.sql);
    for param in &query.params {
        q = match param {
            Value::Text(s) => q.bind(s.clone()),
            Value::Integer(i) => q.bind(*i),
        };
    }
    q.fetch_all(&state.pool)
        .await
        .map_err(ErrorInternalServerError)
}

pub async fn filter_tracks(
    state: web::Data<AppState>,
    body: web::Json<QuerySpec>,
) -> HandlerResult {
    let ids = matching_ids(&state, "tracks", &body).await?;
    // Loaded one by one rather than via IN (...): the list is bounded by the
    // spec's limit, and this preserves the query's ORDER BY, which a single
    // unordered IN-query would throw away.
    let mut tracks = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Ok(Some(t)) = repo::track::find(state.pool.clone(), id).await {
            tracks.push(t);
        }
    }
    Ok(HttpResponse::Ok().json(tracks))
}

pub async fn filter_albums(
    state: web::Data<AppState>,
    body: web::Json<QuerySpec>,
) -> HandlerResult {
    let ids = matching_ids(&state, "albums", &body).await?;
    let mut albums = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Ok(Some(a)) = repo::album::find(state.pool.clone(), id).await {
            albums.push(a);
        }
    }
    Ok(HttpResponse::Ok().json(albums))
}

pub async fn filter_artists(
    state: web::Data<AppState>,
    body: web::Json<QuerySpec>,
) -> HandlerResult {
    let ids = matching_ids(&state, "artists", &body).await?;
    let mut artists = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Ok(Some(a)) = repo::artist::find(state.pool.clone(), id).await {
            artists.push(a);
        }
    }
    Ok(HttpResponse::Ok().json(artists))
}
