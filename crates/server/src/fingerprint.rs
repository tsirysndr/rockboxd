//! Background acoustic fingerprinting, and the tag completion it enables.
//!
//! Two stages, running concurrently, each throttled by what it is actually
//! contending for:
//!
//! 1. **Fingerprint.** Decode the first two minutes of every local track and
//!    compute a Chromaprint fingerprint. CPU-bound, so it runs a few files at a
//!    time — enough to use a machine that is idle, few enough that the daemon's
//!    real job, playing audio without gaps, never notices. The limit is a
//!    semaphore over `spawn_blocking`, not a thread pool: decode work stays off
//!    the async runtime entirely.
//! 2. **Identify.** Trade each fingerprint for an AcoustID and a MusicBrainz
//!    recording, and use it to fill in tags the file never had — what
//!    MusicBrainz Picard does, minus the clicking. Network-bound and rate
//!    limited by someone else's server, so it runs strictly one request at a
//!    time, spaced to stay inside AcoustID's three-per-second limit.
//!
//! The stages are separate because their failure modes are: no network, no API
//! key, or AcoustID down stops stage 2 and leaves stage 1 perfectly useful — a
//! fingerprint is what makes two copies of a track recognisable as the same
//! recording whether or not anyone ever looks it up.
//!
//! Nothing here blocks anything. `start` returns immediately, both stages begin
//! after the daemon has finished booting, and both idle-poll afterwards so that
//! files the watcher adds later are picked up without a restart.

use rockbox_fingerprint::{AcoustId, Identified};
use rockbox_library::repo::{self, fingerprint::IdentifiedTags};
use sqlx::{Pool, Sqlite};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How long to leave the daemon alone before starting.
///
/// Boot is the busiest the disk ever is — the firmware is loading, the first
/// scan is walking the music directory, and the key/BPM pass starts at 30
/// seconds. Fingerprinting is the least urgent of all of it.
const START_DELAY: Duration = Duration::from_secs(60);

/// How long to wait before looking for work again once there is none.
const IDLE_POLL: Duration = Duration::from_secs(300);

/// Breathing room between batches of decodes.
const BATCH_PAUSE: Duration = Duration::from_secs(2);

/// Spacing between AcoustID requests.
///
/// Their documented limit is three per second per key. Staying comfortably
/// under it costs nothing here — the pass has no deadline — and being the
/// client that gets a free service rate-limited is not worth the minutes it
/// would save.
const LOOKUP_SPACING: Duration = Duration::from_millis(400);

/// How long to stand back after AcoustID returns an error.
const LOOKUP_BACKOFF: Duration = Duration::from_secs(60);

/// Ceiling on concurrent decodes when the machine is big enough to want one.
///
/// Fingerprinting is disk-then-CPU, and past a handful of files the disk is the
/// limit anyway — so a 32-core machine gains nothing from 32 decoders except a
/// louder fan and a daemon competing with itself for I/O.
const MAX_CONCURRENCY: usize = 4;

/// Spawn the pass. Returns immediately; all the work happens on its own thread.
pub fn start(pool: Pool<Sqlite>) {
    let spawned = std::thread::Builder::new()
        .name("acoustic-fingerprint".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::warn!("fingerprint pass: no runtime: {e}");
                    return;
                }
            };
            rt.block_on(run(pool));
        });
    if let Err(e) = spawned {
        tracing::warn!("fingerprint pass: could not start: {e}");
    }
}

async fn run(pool: Pool<Sqlite>) {
    let settings = rockbox_settings::read_settings().unwrap_or_default();

    if settings.fingerprint_enabled == Some(false) {
        tracing::info!("fingerprint pass: disabled by settings");
        return;
    }

    let concurrency = settings
        .fingerprint_concurrency
        .map(|n| n as usize)
        .unwrap_or_else(default_concurrency)
        .clamp(1, 32);

    // The env var is the override for a one-off run; settings.toml is where it
    // normally lives.
    let api_key = std::env::var("ACOUSTID_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .or(settings.acoustid_api_key.clone());
    let acoustid = match settings.acoustid_enabled {
        Some(false) => None,
        _ => AcoustId::from_key(api_key),
    };
    let write_tags = settings.acoustid_write_tags == Some(true);

    match (&acoustid, write_tags) {
        (Some(_), true) => tracing::info!(
            "fingerprint pass: {concurrency} decoders, AcoustID tag completion (writing tags to files)"
        ),
        (Some(_), false) => tracing::info!(
            "fingerprint pass: {concurrency} decoders, AcoustID tag completion (library only)"
        ),
        (None, _) => tracing::info!(
            "fingerprint pass: {concurrency} decoders, no AcoustID key — fingerprints only"
        ),
    }

    let idle = Arc::new(AtomicBool::new(false));
    tokio::join!(
        fingerprint_stage(pool.clone(), concurrency, idle.clone()),
        identify_stage(pool, acoustid, write_tags, idle),
    );
}

/// Half the cores, capped — leaving the other half for playback, the servers,
/// and whatever else the machine is for.
fn default_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
        .clamp(1, MAX_CONCURRENCY)
}

/// Stage 1: decode and fingerprint, `concurrency` files at a time.
async fn fingerprint_stage(pool: Pool<Sqlite>, concurrency: usize, idle: Arc<AtomicBool>) {
    use futures_util::stream::StreamExt;

    tokio::time::sleep(START_DELAY).await;

    loop {
        // A batch rather than the whole table: the queue is re-read every
        // round, so tracks the watcher adds mid-pass join it instead of
        // waiting for a restart.
        let batch =
            match repo::fingerprint::pending_fingerprint(pool.clone(), (concurrency * 4) as i64)
                .await
            {
                Ok(batch) => batch,
                Err(e) => {
                    tracing::warn!("fingerprint pass: could not read the queue: {e}");
                    tokio::time::sleep(IDLE_POLL).await;
                    continue;
                }
            };

        if batch.is_empty() {
            idle.store(true, Ordering::Relaxed);
            tokio::time::sleep(IDLE_POLL).await;
            continue;
        }
        idle.store(false, Ordering::Relaxed);

        let marked: Vec<bool> = futures_util::stream::iter(batch)
            .map(|pending| {
                let pool = pool.clone();
                async move { fingerprint_one(pool, pending).await }
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        // Every write failing means the queue hands back the same files next
        // round — a hot loop that re-decodes them every couple of seconds for
        // ever. Back right off instead and let the database recover.
        if marked.iter().all(|ok| !ok) {
            tracing::warn!("fingerprint pass: no result could be written, backing off");
            tokio::time::sleep(IDLE_POLL).await;
            continue;
        }

        tokio::time::sleep(BATCH_PAUSE).await;
    }
}

/// Fingerprint one file and record the outcome. Returns whether the row was
/// taken off the queue — `false` means it will come back next round.
async fn fingerprint_one(pool: Pool<Sqlite>, pending: repo::fingerprint::Pending) -> bool {
    let path = pending.path.clone();
    let computed =
        tokio::task::spawn_blocking(move || rockbox_fingerprint::fingerprint_file(&path)).await;

    // Failures record the empty marker rather than leaving NULL: a file that
    // cannot be decoded today cannot be decoded tomorrow either, and the queue
    // must not hand back the same broken file for ever.
    let stored = match computed {
        Ok(Ok(fingerprint)) => {
            tracing::debug!(
                "fingerprinted {} ({:.0}s)",
                pending.path,
                fingerprint.duration
            );
            repo::fingerprint::set_fingerprint(
                pool,
                &pending.id,
                Some((&fingerprint.compressed, fingerprint.duration)),
            )
            .await
        }
        Ok(Err(e)) => {
            tracing::debug!("fingerprint failed for {}: {e}", pending.path);
            repo::fingerprint::set_fingerprint(pool, &pending.id, None).await
        }
        Err(e) => {
            tracing::warn!("fingerprint task panicked for {}: {e}", pending.path);
            repo::fingerprint::set_fingerprint(pool, &pending.id, None).await
        }
    };

    match stored {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("fingerprint write failed for {}: {e}", pending.id);
            false
        }
    }
}

/// What to do after one lookup.
enum Next {
    /// Carry on to the next track after the usual spacing.
    Continue,
    /// AcoustID is unhappy — stand back and re-read the queue afterwards.
    BackOff,
}

/// Stage 2: identify fingerprints and fill in the tags the files never had.
async fn identify_stage(
    pool: Pool<Sqlite>,
    acoustid: Option<AcoustId>,
    write_tags: bool,
    idle: Arc<AtomicBool>,
) {
    let Some(acoustid) = acoustid else {
        // No key, or switched off. Stage 1 still runs, and its fingerprints are
        // still worth having.
        return;
    };

    tokio::time::sleep(START_DELAY).await;

    loop {
        let batch = match repo::fingerprint::pending_identification(pool.clone(), 50).await {
            Ok(batch) => batch,
            Err(e) => {
                tracing::warn!("acoustid: could not read the queue: {e}");
                tokio::time::sleep(IDLE_POLL).await;
                continue;
            }
        };

        if batch.is_empty() {
            // Nothing to look up. If stage 1 is still producing, check back
            // sooner than the idle poll — otherwise identification would
            // trail the fingerprinting by five minutes for no reason.
            let wait = if idle.load(Ordering::Relaxed) {
                IDLE_POLL
            } else {
                BATCH_PAUSE
            };
            tokio::time::sleep(wait).await;
            continue;
        }

        for pending in batch {
            match identify_one(&pool, &acoustid, write_tags, pending).await {
                Next::Continue => tokio::time::sleep(LOOKUP_SPACING).await,
                Next::BackOff => {
                    tokio::time::sleep(LOOKUP_BACKOFF).await;
                    break;
                }
            }
        }
    }
}

async fn identify_one(
    pool: &Pool<Sqlite>,
    acoustid: &AcoustId,
    write_tags: bool,
    pending: repo::fingerprint::Unidentified,
) -> Next {
    // The decoder's measurement when it has one, the tagged length otherwise.
    // AcoustID matches on duration as well as fingerprint, so a wrong one here
    // means no match rather than a wrong match.
    let duration = pending
        .fingerprint_duration
        .map(|d| d.round().max(0.0) as u32)
        .filter(|d| *d > 0)
        .unwrap_or(pending.length);

    if duration == 0 {
        tracing::debug!("acoustid: no duration for {}, skipping", pending.path);
        return marked_or_back_off(mark(pool, &pending.id, "", None).await);
    }

    let found = match acoustid.lookup(&pending.fingerprint, duration).await {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!("acoustid lookup failed for {}: {e}", pending.path);
            return Next::BackOff;
        }
    };

    let Some(found) = found else {
        // AcoustID has never heard this recording. A real answer, and one worth
        // remembering so the next boot doesn't ask again.
        tracing::debug!("acoustid: no match for {}", pending.path);
        return marked_or_back_off(mark(pool, &pending.id, "", None).await);
    };

    let recorded = mark(
        pool,
        &pending.id,
        &found.acoustid,
        found.recording_id.as_deref(),
    )
    .await;

    if !found.has_tags() {
        return marked_or_back_off(recorded);
    }

    if let Err(e) = complete_tags(pool, &pending, &found, write_tags).await {
        tracing::warn!("acoustid: tag completion failed for {}: {e}", pending.path);
    }

    marked_or_back_off(recorded)
}

/// Fill in whatever the file never said — and only that.
async fn complete_tags(
    pool: &Pool<Sqlite>,
    pending: &repo::fingerprint::Unidentified,
    found: &Identified,
    write_tags: bool,
) -> anyhow::Result<()> {
    let Some(track) = repo::track::find(pool.clone(), &pending.id).await? else {
        // Deleted between the lookup being queued and answered.
        return Ok(());
    };

    let applied = repo::fingerprint::fill_unknown(pool.clone(), &track, &tags_from(found)).await?;
    if applied.is_empty() {
        // Already fully tagged. The AcoustID is still recorded — it is what
        // makes this track recognisable as a duplicate of another file.
        return Ok(());
    }

    tracing::info!(
        "acoustid: {} identified as {} — {} (score {:.2})",
        pending.path,
        applied.artist.as_deref().unwrap_or(&track.artist),
        applied.title.as_deref().unwrap_or(&track.title),
        found.score,
    );

    if !write_tags {
        return Ok(());
    }

    // Only the fields that were genuinely filled reach the file. A wrong match
    // should cost an empty tag staying empty, never a good tag being replaced.
    let update = rockbox_fingerprint::TagUpdate {
        title: applied.title,
        artist: applied.artist,
        album: applied.album,
        album_artist: applied.album_artist,
        year: applied.year,
        track_number: applied.track_number,
        disc_number: applied.disc_number,
        recording_id: found.recording_id.clone(),
        artist_id: found.artist_id.clone(),
        release_id: found.release_id.clone(),
        release_group_id: found.release_group_id.clone(),
    };
    let path = pending.path.clone();
    tokio::task::spawn_blocking(move || rockbox_fingerprint::tags::write(&path, &update)).await??;

    Ok(())
}

fn tags_from(found: &Identified) -> IdentifiedTags {
    IdentifiedTags {
        title: found.title.clone(),
        artist: found.artist.clone(),
        album: found.album.clone(),
        album_artist: found.album_artist.clone(),
        year: found.year,
        track_number: found.track_number,
        disc_number: found.disc_number,
    }
}

async fn mark(pool: &Pool<Sqlite>, id: &str, acoustid: &str, recording_id: Option<&str>) -> bool {
    match repo::fingerprint::set_acoustid(pool.clone(), id, acoustid, recording_id).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!("acoustid write failed for {id}: {e}");
            false
        }
    }
}

/// A lookup whose answer could not be stored will simply be made again, so a
/// database refusing writes must not be met with more requests to someone
/// else's rate-limited service.
fn marked_or_back_off(marked: bool) -> Next {
    if marked {
        Next::Continue
    } else {
        Next::BackOff
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Never zero — a zero-permit semaphore would stall the stage for ever —
    /// and never more than the cap, however many cores the machine has.
    #[test]
    fn the_decoder_count_stays_in_range() {
        let n = default_concurrency();
        assert!((1..=MAX_CONCURRENCY).contains(&n), "{n} decoders");
    }

    #[test]
    fn identified_metadata_maps_across_whole() {
        let found = Identified {
            acoustid: "a".into(),
            score: 0.9,
            title: Some("Tainted Love".into()),
            artist: Some("Soft Cell".into()),
            album: Some("Non-Stop Erotic Cabaret".into()),
            album_artist: Some("Soft Cell".into()),
            year: Some(1981),
            track_number: Some(3),
            disc_number: Some(1),
            ..Default::default()
        };
        let tags = tags_from(&found);
        assert_eq!(tags.title.as_deref(), Some("Tainted Love"));
        assert_eq!(tags.album.as_deref(), Some("Non-Stop Erotic Cabaret"));
        assert_eq!(tags.year, Some(1981));
        assert_eq!(tags.track_number, Some(3));
        assert_eq!(tags.disc_number, Some(1));
        assert!(!tags.is_empty());
    }

    /// An AcoustID with nothing behind it maps to nothing to write — the id is
    /// still recorded, but no tag is invented from it.
    #[test]
    fn a_bare_acoustid_fills_nothing() {
        let found = Identified {
            acoustid: "a".into(),
            score: 0.9,
            ..Default::default()
        };
        assert!(tags_from(&found).is_empty());
        assert!(!found.has_tags());
    }
}
