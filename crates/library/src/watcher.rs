use crate::audio_scan::{save_audio_metadata, scan_audio_files};
use crate::repo;
use anyhow::Error;
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::{Pool, Sqlite};
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::{mpsc, Mutex};
use tokio::time::MissedTickBehavior;
use tracing::{debug, info, warn};

const DEFAULT_RESCAN_INTERVAL_SECS: u64 = 120;

const AUDIO_EXTENSIONS: [&str; 18] = [
    "mp3", "ogg", "flac", "m4a", "aac", "mp4", "alac", "wav", "wv", "mpc", "aiff", "aif", "ac3",
    "opus", "spx", "sid", "ape", "wma",
];

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            AUDIO_EXTENSIONS.iter().any(|x| *x == lower.as_str())
        })
        .unwrap_or(false)
}

/// Start watching `music_dir` recursively. New audio files are inserted into
/// the database; removed audio files are deleted from it. The watcher handle
/// is leaked so it lives for the lifetime of the process — dropping the
/// `RecommendedWatcher` stops its background thread.
pub fn start_watcher(pool: Pool<Sqlite>, music_dir: PathBuf) -> Result<(), Error> {
    if !music_dir.exists() {
        warn!(
            "watcher: music_dir does not exist, skipping: {}",
            music_dir.display()
        );
        return Ok(());
    }

    info!(
        "watcher: starting library watcher on {}",
        music_dir.display()
    );

    let (tx, mut rx) = mpsc::unbounded_channel::<notify::Event>();

    let mut watcher: RecommendedWatcher =
        notify::recommended_watcher(move |res: notify::Result<notify::Event>| match res {
            Ok(event) => {
                let _ = tx.send(event);
            }
            Err(e) => warn!("watcher: notify error: {}", e),
        })?;

    watcher.watch(&music_dir, RecursiveMode::Recursive)?;
    // Keep the watcher alive for the lifetime of the process.
    Box::leak(Box::new(watcher));

    let event_pool = pool.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let Err(e) = handle_event(event_pool.clone(), event).await {
                warn!("watcher: handler error: {}", e);
            }
        }
    });

    spawn_periodic_rescan(pool, music_dir);

    Ok(())
}

/// Background rescan + delete reconciliation, as a backstop for events the
/// filesystem watcher misses (NFS/SMB/FUSE inotify gaps on Linux, kqueue
/// coalescing on BSDs). Interval is read from `ROCKBOX_RESCAN_INTERVAL_SECS`,
/// defaulting to 120s; set to `0` to disable.
fn spawn_periodic_rescan(pool: Pool<Sqlite>, music_dir: PathBuf) {
    let interval_secs = env::var("ROCKBOX_RESCAN_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_RESCAN_INTERVAL_SECS);
    if interval_secs == 0 {
        info!("watcher: periodic rescan disabled");
        return;
    }

    info!(
        "watcher: periodic rescan every {}s of {}",
        interval_secs,
        music_dir.display()
    );

    let lock = std::sync::Arc::new(Mutex::new(()));
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let Ok(_guard) = lock.try_lock() else {
                debug!("watcher: rescan still running, skipping tick");
                continue;
            };
            // scan_audio_files reconciles deletions itself once the walk is
            // done, so no separate reconcile pass is needed here.
            if let Err(e) = scan_audio_files(pool.clone(), music_dir.clone()).await {
                warn!("watcher: periodic rescan failed: {}", e);
            }
        }
    });
}

async fn handle_event(pool: Pool<Sqlite>, event: notify::Event) -> Result<(), Error> {
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                add_path(&pool, &path).await;
            }
        }
        EventKind::Modify(ModifyKind::Data(_)) => {
            for path in event.paths {
                // Re-insert only if not already indexed; save_audio_metadata
                // is idempotent and will no-op for known paths.
                add_path(&pool, &path).await;
            }
        }
        EventKind::Modify(ModifyKind::Name(mode)) => {
            handle_rename(&pool, mode, event.paths).await;
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                remove_path(&pool, &path).await;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn add_path(pool: &Pool<Sqlite>, path: &Path) {
    if !is_audio_file(path) || !path.exists() {
        return;
    }
    let path_str = path.to_string_lossy().to_string();
    debug!("watcher: add {}", path_str);
    if let Err(e) = save_audio_metadata(pool.clone(), &path_str, None).await {
        warn!("watcher: failed to add {}: {}", path_str, e);
    }
}

async fn remove_path(pool: &Pool<Sqlite>, path: &Path) {
    if !is_audio_file(path) {
        return;
    }
    let path_str = path.to_string_lossy().to_string();
    match repo::track::delete_by_path(pool.clone(), &path_str).await {
        Ok(Some(track)) => info!("watcher: removed {} ({})", track.title, path_str),
        Ok(None) => debug!("watcher: remove for unknown path {}", path_str),
        Err(e) => warn!("watcher: failed to remove {}: {}", path_str, e),
    }
}

async fn handle_rename(pool: &Pool<Sqlite>, mode: RenameMode, paths: Vec<PathBuf>) {
    match mode {
        RenameMode::Both if paths.len() == 2 => {
            remove_path(pool, &paths[0]).await;
            add_path(pool, &paths[1]).await;
        }
        RenameMode::From => {
            for path in paths {
                remove_path(pool, &path).await;
            }
        }
        RenameMode::To => {
            for path in paths {
                add_path(pool, &path).await;
            }
        }
        _ => {}
    }
}
