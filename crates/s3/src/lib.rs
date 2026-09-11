//! S3-compatible HTTP API for rockboxd.
//!
//! Implements `PutObject`, `DeleteObject`, `GetObject`, `HeadObject`,
//! `ListObjectsV2` and `ListBuckets` against a single virtual bucket whose
//! contents map 1:1 to `music_dir`. The library DB stays in sync through the
//! existing filesystem watcher (`crates/library/src/watcher.rs`) — every PUT
//! triggers a `Create` event and every DELETE triggers `Remove`.
//!
//! Authentication is AWS Signature V4 (header form). Streaming chunked
//! payloads (`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`) are not yet supported —
//! clients should use `UNSIGNED-PAYLOAD` (the default for HTTPS in most SDKs;
//! pass `--checksum-algorithm CRC32` or `--no-verify-payload` for awscli over
//! plain HTTP) or sign the full body hash.

mod admin;
mod handlers;
mod sigv4;
mod xml;

use actix_web::{web, App, HttpServer};
use rockbox_settings::read_settings;
use std::path::PathBuf;

const AUDIO_EXTENSIONS: [&str; 18] = [
    "mp3", "ogg", "flac", "m4a", "aac", "mp4", "alac", "wav", "wv", "mpc", "aiff", "aif", "ac3",
    "opus", "spx", "sid", "ape", "wma",
];

/// Worker threads for this server — see `rockbox_server::http::workers` for
/// why the daemon caps them instead of taking actix's one-per-CPU default.
fn http_workers() -> usize {
    std::env::var("ROCKBOX_HTTP_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(4))
                .unwrap_or(4)
        })
}

/// Cap each individual PUT at 2 GiB. Larger files would need multipart
/// upload support, which is intentionally out of scope for v1.
const MAX_BODY_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// Region embedded in the SigV4 credential scope. Clients must use this
/// value (e.g. `aws --region us-east-1`) when signing requests.
pub const REGION: &str = "us-east-1";

/// The single virtual bucket exposed by the server. Its contents map 1:1 to
/// `music_dir` on disk.
pub const BUCKET: &str = "music";

pub struct AppState {
    pub music_dir: PathBuf,
    pub access_key: String,
    pub secret_key: String,
}

pub async fn start() -> anyhow::Result<()> {
    let settings = read_settings().unwrap_or_default();
    if !settings.s3_enabled.unwrap_or(false) {
        tracing::debug!("s3: disabled");
        return Ok(());
    }

    let access_key = match settings.s3_access_key {
        Some(s) if !s.is_empty() => s,
        _ => {
            tracing::warn!("s3: enabled but s3_access_key is empty, server not starting");
            return Ok(());
        }
    };
    let secret_key = match settings.s3_secret_key {
        Some(s) if !s.is_empty() => s,
        _ => {
            tracing::warn!("s3: enabled but s3_secret_key is empty, server not starting");
            return Ok(());
        }
    };

    let host = settings.s3_host.unwrap_or_else(|| "0.0.0.0".to_string());
    let port = settings.s3_port.unwrap_or(9000);
    let music_dir = PathBuf::from(rockbox_settings::get_music_dir()?);

    let addr = format!("{}:{}", host, port);
    tracing::info!(
        "s3 server listening on {addr} (bucket={}, region={}, music_dir={})",
        BUCKET,
        REGION,
        music_dir.display()
    );

    let state = web::Data::new(AppState {
        music_dir,
        access_key,
        secret_key,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .app_data(web::PayloadConfig::new(MAX_BODY_BYTES))
            .configure(admin::configure)
            .route("/", web::get().to(handlers::list_buckets))
            .route("/{bucket}", web::get().to(handlers::list_objects))
            .route("/{bucket}/", web::get().to(handlers::list_objects))
            .route("/{bucket}", web::head().to(handlers::head_bucket))
            .route("/{bucket}/", web::head().to(handlers::head_bucket))
            .route("/{bucket}/{key:.*}", web::put().to(handlers::put_object))
            .route(
                "/{bucket}/{key:.*}",
                web::delete().to(handlers::delete_object),
            )
            .route("/{bucket}/{key:.*}", web::get().to(handlers::get_object))
            .route("/{bucket}/{key:.*}", web::head().to(handlers::head_object))
    })
    .workers(http_workers())
    .bind(&addr)?
    .run()
    .await?;

    Ok(())
}

pub mod server {
    pub use super::start;
}
