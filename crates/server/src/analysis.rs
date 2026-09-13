//! Background key/BPM analysis over the local library.
//!
//! Walks every local track whose `key` is NULL, runs `rockbox-analysis` on the
//! file (tag first, detection as the fallback — see that crate for why tags
//! win), and writes the result into `track.key` / `track.bpm`, where rsql
//! filters (`key==Am`, `bpm>=120`) and smart playlists read it.
//!
//! Runs once per boot, single file at a time, throttled — analysis decodes the
//! whole file, and the daemon's first job is playing audio, not crunching it.
//! A track that fails analysis is marked with an empty-string key so it is not
//! reattempted every boot; `key=null=` in a filter still treats it as
//! unanalysed-adjacent ("" is not a key), and a rescan after a fix can reset
//! it.

use sqlx::{Pool, Sqlite};
use std::time::Duration;

/// Spawn the pass. Returns immediately; work happens on a blocking thread.
pub fn start(pool: Pool<Sqlite>) {
    std::thread::Builder::new()
        .name("key-bpm-analysis".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("analysis runtime");
            rt.block_on(run(pool));
        })
        .expect("spawn analysis thread");
}

async fn run(pool: Pool<Sqlite>) {
    // Let the daemon finish booting — gRPC, first scan — before competing
    // for the disk.
    tokio::time::sleep(Duration::from_secs(30)).await;

    loop {
        // One at a time, re-queried each round, so a scan that adds tracks
        // mid-pass extends the pass instead of being missed.
        let next: Option<(String, String)> = sqlx::query_as(
            "SELECT id, path FROM track \
             WHERE key IS NULL AND is_remote = 0 AND path NOT LIKE 'http%' \
             LIMIT 1",
        )
        .fetch_optional(&pool)
        .await
        .ok()
        .flatten();

        let Some((id, path)) = next else {
            tracing::info!("key/bpm analysis: library fully analysed");
            return;
        };

        let extension = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());

        // Decode + analyse off the async thread; it is CPU-bound.
        let analysed = tokio::task::spawn_blocking(move || {
            let bytes = std::fs::read(&path)?;
            rockbox_analysis::analyze(&bytes, extension.as_deref())
        })
        .await;

        let (key, bpm, waveform) = match analysed {
            Ok(Ok(a)) => (a.key.unwrap_or_default(), a.bpm, Some(a.waveform)),
            Ok(Err(e)) => {
                tracing::debug!("analysis failed for {id}: {e}");
                (String::new(), None, None)
            }
            Err(e) => {
                tracing::warn!("analysis task panicked for {id}: {e}");
                (String::new(), None, None)
            }
        };

        if let Err(e) = sqlx::query("UPDATE track SET key = ?, bpm = ?, waveform = ? WHERE id = ?")
            .bind(&key)
            .bind(bpm)
            .bind(waveform)
            .bind(&id)
            .execute(&pool)
            .await
        {
            tracing::warn!("analysis write failed for {id}: {e}");
            // A failing write would spin on the same row for ever; back off
            // hard and let the next boot retry.
            tokio::time::sleep(Duration::from_secs(600)).await;
        }

        // The throttle: at one file a second the disk and CPU barely notice,
        // and a few thousand tracks still finish within the first hour.
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
