use crate::album_art::extract_and_save_album_cover_with_key;
use crate::copyright_message::extract_copyright_message;
use crate::entity::album::Album;
use crate::entity::album_tracks::AlbumTracks;
use crate::entity::artist::Artist;
use crate::entity::artist_tracks::ArtistTracks;
use crate::label::extract_label;
use crate::{entity::track::Track, repo};
use anyhow::{anyhow, Error};
use chrono::Utc;
use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use owo_colors::OwoColorize;
use rockbox_sys as rb;
use rockbox_sys::types::mp3_entry::Mp3Entry;
use sqlx::{Pool, Sqlite};
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{fs, sync::Semaphore};

const AUDIO_EXTENSIONS: [&str; 18] = [
    "mp3", "ogg", "flac", "m4a", "aac", "mp4", "alac", "wav", "wv", "mpc", "aiff", "aif", "ac3",
    "opus", "spx", "sid", "ape", "wma",
];

const MAX_CONCURRENT_SCANS: usize = 1;

pub fn scan_audio_files(
    pool: Pool<Sqlite>,
    audio_dir: PathBuf,
) -> BoxFuture<'static, Result<Vec<PathBuf>, Error>> {
    Box::pin(async move {
        let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_SCANS));
        let found = scan_audio_files_inner(pool.clone(), audio_dir.clone(), sem).await?;

        // Every scan also purges rows whose backing file disappeared since the
        // last one, so a deleted file never lingers in the library.
        let indexed = repo::track::local_paths(pool.clone()).await?;

        // Guard: a scan that turned up nothing at all while the DB still holds
        // local tracks means the music dir is empty or its volume isn't
        // mounted — not that the user deleted their whole library.
        if found.is_empty() && !indexed.is_empty() {
            tracing::warn!(
                "scan of {} found no audio files — skipping delete reconciliation",
                audio_dir.display()
            );
            return Ok(found);
        }

        match prune_missing(pool, indexed).await {
            Ok(0) => {}
            Ok(n) => tracing::info!("scan: removed {} track(s) whose file no longer exists", n),
            Err(e) => tracing::warn!("scan: delete reconciliation failed: {}", e),
        }

        Ok(found)
    })
}

fn scan_audio_files_inner(
    pool: Pool<Sqlite>,
    audio_dir: PathBuf,
    sem: Arc<Semaphore>,
) -> BoxFuture<'static, Result<Vec<PathBuf>, Error>> {
    Box::pin(async move {
        // Hold permit only while reading directory entries and processing files.
        // Release before spawning/awaiting children to prevent deadlock.
        let _permit = sem.acquire().await?;

        let mut result = Vec::new();
        let mut subdirs = Vec::new();
        let mut dir = fs::read_dir(audio_dir).await?;

        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                println!("{} {:?}", "Scanning".bright_green(), path);
                subdirs.push(path);
            } else if path.is_file() {
                let path = path.to_str().unwrap();
                if !AUDIO_EXTENSIONS
                    .into_iter()
                    .any(|ext| path.ends_with(&format!(".{}", ext)))
                {
                    continue;
                }
                save_audio_metadata(pool.clone(), path, None).await?;
                result.push(PathBuf::from(path));
            }
        }

        drop(_permit);

        let mut futures = FuturesUnordered::new();
        for dir_path in subdirs {
            let cloned_pool = pool.clone();
            let cloned_sem = Arc::clone(&sem);
            futures.push(tokio::spawn(async move {
                scan_audio_files_inner(cloned_pool, dir_path, cloned_sem).await
            }));
        }

        while let Some(Ok(sub_result)) = futures.next().await {
            match sub_result {
                Ok(paths) => result.extend(paths),
                Err(e) => return Err(e),
            }
        }

        Ok(result)
    })
}

/// Remove tracks whose backing file no longer exists on disk.
///
/// Runs at the end of every [`scan_audio_files`], and periodically from the
/// watcher as a backstop for filesystem events that get dropped silently
/// (e.g. inotify on NFS/SMB/FUSE, kqueue coalescing on BSDs). Only touches
/// local rows (is_remote = 0) — a remote URL never "exists" on disk.
pub async fn reconcile_deletions(pool: Pool<Sqlite>) -> Result<u64, Error> {
    let indexed = repo::track::local_paths(pool.clone()).await?;
    prune_missing(pool, indexed).await
}

/// Delete the `(id, path)` rows whose `path` is gone from the filesystem, then
/// sweep up the albums and artists that lost their last track. Returns the
/// number of tracks removed.
async fn prune_missing(pool: Pool<Sqlite>, indexed: Vec<(String, String)>) -> Result<u64, Error> {
    if indexed.is_empty() {
        return Ok(0);
    }

    // stat() per track blocks; keep tens of thousands of them off the runtime.
    let missing: Vec<String> = tokio::task::spawn_blocking(move || {
        indexed
            .into_iter()
            .filter(|(_, path)| !Path::new(path).exists())
            .map(|(id, path)| {
                tracing::debug!("scan: {} no longer exists, removing from library", path);
                id
            })
            .collect()
    })
    .await?;

    if missing.is_empty() {
        return Ok(0);
    }

    // SQLite caps bound parameters per statement (999 on older builds).
    let mut removed = 0u64;
    for chunk in missing.chunks(500) {
        removed += repo::track::delete_by_ids(pool.clone(), chunk).await?;
    }

    // Albums first: an artist stays alive while it still owns an album.
    match repo::album::delete_orphans(pool.clone()).await {
        Ok(n) if n > 0 => tracing::info!("scan: removed {} album(s) left with no tracks", n),
        Ok(_) => {}
        Err(e) => tracing::warn!("scan: album orphan cleanup failed: {}", e),
    }
    match repo::artist::delete_orphans(pool).await {
        Ok(n) if n > 0 => tracing::info!("scan: removed {} artist(s) left with no tracks", n),
        Ok(_) => {}
        Err(e) => tracing::warn!("scan: artist orphan cleanup failed: {}", e),
    }

    Ok(removed)
}

pub async fn save_audio_metadata(
    pool: Pool<Sqlite>,
    path: &str,
    album_art_uri: Option<&str>,
) -> Result<(), Error> {
    if !is_supported_audio_path(path) {
        return Ok(());
    }
    let existing_track = repo::track::find_by_path(pool.clone(), path).await?;

    let filename = path.split('/').last().unwrap();
    let dir = path.replace(filename, "");
    println!(
        "{} {}{}",
        "Found".bright_green(),
        dir,
        filename.bright_yellow()
    );
    let remote_probe = if is_remote_path(path) {
        Some(download_partial_remote_file(path).await?)
    } else {
        None
    };
    let metadata_path = remote_probe
        .as_ref()
        .map(|probe| probe.path.as_str())
        .unwrap_or(path);

    // Run C FFI call on blocking thread pool to avoid thread-safety issues
    let metadata_path_owned = metadata_path.to_string();
    let entry =
        tokio::task::spawn_blocking(move || rb::metadata::get_metadata(-1, &metadata_path_owned))
            .await
            .map_err(|e| anyhow!("Failed to get metadata: {}", e))?;

    let title = track_title(&entry, path);
    let artist = track_artist(&entry);
    let album_artist = track_album_artist(&entry, &artist);
    let album = track_album(&entry, &title);
    let mut album_art = extract_and_save_album_cover_with_key(metadata_path, Some(&album))?;

    // Fall back to the UPnP albumArtURI when the audio file has no embedded art.
    if album_art.is_none() {
        if let Some(art_url) = album_art_uri {
            album_art = download_and_save_album_art(art_url, &album).await?;
        }
    }

    let title = match title.is_empty() {
        true => "Unknown Title".to_string(),
        false => title,
    };

    let artist = match artist.is_empty() {
        true => "Unknown Artist".to_string(),
        false => artist,
    };

    let album_artist = match album_artist.is_empty() {
        true => "Unknown Artist".to_string(),
        false => album_artist,
    };

    let album = match album.is_empty() {
        true => "Unknown Album".to_string(),
        false => album,
    };

    if let Some(existing_track) = existing_track {
        if let Some(ref album_art) = album_art {
            if existing_track.album_art.as_deref() != Some(album_art.as_str()) {
                repo::track::update_album_art(pool.clone(), &existing_track.id, album_art).await?;
            }

            if let Some(album) = repo::album::find(pool.clone(), &existing_track.album_id).await? {
                if album.album_art.as_deref() != Some(album_art.as_str()) {
                    repo::album::update_album_art(pool.clone(), &album.id, album_art).await?;
                }
            }
        }

        return Ok(());
    }
    let track_hash = format!("{:x}", md5::compute(path.as_bytes()));
    let artist_id = cuid::cuid1()?;
    let album_id = cuid::cuid1()?;
    let album_md5 = format!(
        "{:x}",
        md5::compute(format!("{}{}{}", album_artist, album, entry.year).as_bytes())
    );
    let artist_id = repo::artist::save(
        pool.clone(),
        Artist {
            id: artist_id.clone(),
            name: artist.clone(),
            bio: None,
            image: None,
            genres: None,
        },
    )
    .await?;

    let album_id = repo::album::save(
        pool.clone(),
        Album {
            id: album_id,
            title: album.clone(),
            artist: album_artist.clone(),
            year: clamp_i32_to_u32(entry.year).unwrap_or_default(),
            year_string: option_string(&entry.year_string).unwrap_or_default(),
            album_art: album_art.clone(),
            md5: album_md5,
            artist_id: artist_id.clone(),
            label: match is_remote_path(path) {
                true => None,
                false => extract_label(path)?,
            },
            copyright_message: match is_remote_path(path) {
                true => None,
                false => extract_copyright_message(path)?,
            },
        },
    )
    .await?;

    let track_id = repo::track::save(
        pool.clone(),
        Track {
            id: cuid::cuid1()?,
            path: path.to_string(),
            title,
            artist,
            album,
            genre: option_string(&entry.genre_string),
            year: clamp_i32_to_u32(entry.year),
            track_number: clamp_i32_to_u32(entry.tracknum),
            disc_number: entry.discnum.max(0) as u32,
            year_string: option_string(&entry.year_string),
            composer: entry.composer,
            album_artist,
            bitrate: entry.bitrate,
            frequency: clamp_u64_to_u32(entry.frequency),
            filesize: clamp_u64_to_u32(entry.filesize),
            length: clamp_u64_to_u32(entry.length),
            md5: track_hash,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            artist_id: artist_id.clone(),
            album_id: album_id.clone(),
            album_art,
            is_remote: is_remote_path(path),
            ..Default::default()
        },
    )
    .await?;

    repo::album_tracks::save(
        pool.clone(),
        AlbumTracks {
            id: cuid::cuid1()?,
            album_id,
            track_id: track_id.clone(),
        },
    )
    .await?;

    repo::artist_tracks::save(
        pool.clone(),
        ArtistTracks {
            id: cuid::cuid1()?,
            artist_id,
            track_id,
        },
    )
    .await?;

    Ok(())
}

/// Save metadata for a streaming URL directly to the DB without probing the stream.
/// Called by the UPnP renderer which already has title/artist/album/duration from DIDL-Lite.
pub async fn save_stream_metadata(
    pool: Pool<Sqlite>,
    url: &str,
    title: &str,
    artist: &str,
    album: &str,
    duration_ms: u32,
) -> Result<(), Error> {
    let track_md5 = format!("{:x}", md5::compute(url.as_bytes()));
    let length_secs = duration_ms / 1000;

    if repo::track::find_by_md5(pool.clone(), &track_md5)
        .await?
        .is_some()
    {
        repo::track::update_stream_metadata(pool, &track_md5, title, artist, album, length_secs)
            .await?;
        return Ok(());
    }

    let artist_id = repo::artist::save(
        pool.clone(),
        Artist {
            id: cuid::cuid1()?,
            name: artist.to_string(),
            bio: None,
            image: None,
            genres: None,
        },
    )
    .await?;

    let album_md5 = format!(
        "{:x}",
        md5::compute(format!("{}{}", artist, album).as_bytes())
    );
    let album_id = repo::album::save(
        pool.clone(),
        Album {
            id: cuid::cuid1()?,
            title: album.to_string(),
            artist: artist.to_string(),
            year: 0,
            year_string: String::new(),
            album_art: None,
            md5: album_md5,
            artist_id: artist_id.clone(),
            label: None,
            copyright_message: None,
        },
    )
    .await?;

    let track_id = repo::track::save(
        pool.clone(),
        Track {
            id: cuid::cuid1()?,
            path: url.to_string(),
            title: title.to_string(),
            artist: artist.to_string(),
            album: album.to_string(),
            album_artist: artist.to_string(),
            length: length_secs,
            md5: track_md5,
            artist_id: artist_id.clone(),
            album_id: album_id.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            is_remote: is_remote_path(url),
            ..Default::default()
        },
    )
    .await?;

    repo::album_tracks::save(
        pool.clone(),
        AlbumTracks {
            id: cuid::cuid1()?,
            album_id,
            track_id: track_id.clone(),
        },
    )
    .await?;

    repo::artist_tracks::save(
        pool.clone(),
        ArtistTracks {
            id: cuid::cuid1()?,
            artist_id,
            track_id,
        },
    )
    .await?;

    Ok(())
}

fn is_remote_path(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

fn is_supported_audio_path(path: &str) -> bool {
    is_remote_path(path)
        || AUDIO_EXTENSIONS
            .into_iter()
            .any(|ext| path.ends_with(&format!(".{}", ext)))
}

fn option_string(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn clamp_i32_to_u32(value: i32) -> Option<u32> {
    if value > 0 {
        Some(value as u32)
    } else {
        None
    }
}

fn clamp_u64_to_u32(value: u64) -> u32 {
    value.min(u32::MAX as u64) as u32
}

fn track_title(entry: &Mp3Entry, path: &str) -> String {
    if !entry.title.trim().is_empty() {
        return entry.title.clone();
    }

    path.rsplit('/')
        .next()
        .and_then(|segment| segment.split('?').next())
        .filter(|segment| !segment.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn track_artist(entry: &Mp3Entry) -> String {
    if !entry.artist.trim().is_empty() {
        return entry.artist.clone();
    }

    if !entry.albumartist.trim().is_empty() {
        return entry.albumartist.clone();
    }

    "Unknown Artist".to_string()
}

fn track_album_artist(entry: &Mp3Entry, artist: &str) -> String {
    if !entry.albumartist.trim().is_empty() {
        entry.albumartist.clone()
    } else {
        artist.to_string()
    }
}

fn track_album(entry: &Mp3Entry, title: &str) -> String {
    if !entry.album.trim().is_empty() {
        entry.album.clone()
    } else {
        title.to_string()
    }
}

struct RemoteProbeFile {
    path: String,
}

impl Drop for RemoteProbeFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn download_partial_remote_file(url: &str) -> Result<RemoteProbeFile, Error> {
    const MAX_PROBE_BYTES: usize = 8 * 1024 * 1024;

    let client = reqwest::Client::new();

    // Try a bounded range first. If the file is smaller than MAX_PROBE_BYTES
    // some servers return 416; in that case fall back to an open-ended range
    // which downloads only to the (small) end of file.
    let mut response = client
        .get(url)
        .header(
            reqwest::header::RANGE,
            format!("bytes=0-{}", MAX_PROBE_BYTES - 1),
        )
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        tracing::info!(
            "probe remote media {}: 416 Range Not Satisfiable, retrying with open-ended range",
            url
        );
        response = client
            .get(url)
            .header(reqwest::header::RANGE, "bytes=0-")
            .send()
            .await?;
    }

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT
    {
        return Err(anyhow!(
            "failed to probe remote media {}: {}",
            url,
            response.status()
        ));
    }

    let extension = match response.headers().get(reqwest::header::CONTENT_TYPE) {
        Some(content_type) => match content_type.to_str() {
            Ok(content_type) => content_type_to_extension(content_type)
                .or_else(|| extension_from_path(url))
                .unwrap_or("mp3"),
            Err(_) => extension_from_path(url).unwrap_or("mp3"),
        },
        None => extension_from_path(url).unwrap_or("mp3"),
    };
    let probe_path = std::env::temp_dir().join(format!(
        "rockbox-remote-probe-{:x}.{}",
        md5::compute(url.as_bytes()),
        extension
    ));

    let mut file = std::fs::File::create(&probe_path)?;
    let mut written = 0usize;

    while let Some(chunk) = response.chunk().await? {
        if written >= MAX_PROBE_BYTES {
            break;
        }

        let remaining = MAX_PROBE_BYTES - written;
        let bytes = if chunk.len() > remaining {
            &chunk[..remaining]
        } else {
            chunk.as_ref()
        };

        file.write_all(bytes)?;
        written += bytes.len();
    }

    if written == 0 {
        return Err(anyhow!(
            "failed to probe remote media {}: empty response",
            url
        ));
    }

    Ok(RemoteProbeFile {
        path: probe_path.to_string_lossy().to_string(),
    })
}

fn extension_from_path(path: &str) -> Option<&str> {
    let path = path.split('?').next().unwrap_or(path);
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    AUDIO_EXTENSIONS
        .into_iter()
        .find(|candidate| *candidate == extension.as_str())
}

fn content_type_to_extension(content_type: &str) -> Option<&'static str> {
    match content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
    {
        "audio/mpeg" => Some("mp3"),
        "audio/ogg" => Some("ogg"),
        "audio/flac" => Some("flac"),
        "audio/x-m4a" | "audio/m4a" => Some("m4a"),
        "audio/aac" => Some("aac"),
        "video/mp4" | "audio/mp4" => Some("mp4"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "audio/x-wavpack" => Some("wv"),
        "audio/x-musepack" => Some("mpc"),
        "audio/aiff" | "audio/x-aiff" => Some("aiff"),
        "audio/ac3" | "audio/vnd.dolby.dd-raw" => Some("ac3"),
        "audio/opus" => Some("opus"),
        "audio/x-speex" => Some("spx"),
        "audio/prs.sid" => Some("sid"),
        "audio/ape" | "audio/x-monkeys-audio" => Some("ape"),
        "audio/x-ms-wma" => Some("wma"),
        _ => None,
    }
}

/// Download album art from a URL (e.g. UPnP `albumArtURI`) and save it to the covers directory.
/// Returns the filename (e.g. `"abc123.jpg"`) on success, or `None` if unavailable.
async fn download_and_save_album_art(
    art_url: &str,
    album_key: &str,
) -> Result<Option<String>, Error> {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    let response = match client.get(art_url).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            tracing::debug!("album art download {} returned {}", art_url, r.status());
            return Ok(None);
        }
        Err(e) => {
            tracing::debug!("album art download {} failed: {}", art_url, e);
            return Ok(None);
        }
    };

    let ext = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .map(|ct| {
            if ct.contains("png") {
                "png"
            } else if ct.contains("gif") {
                "gif"
            } else if ct.contains("bmp") {
                "bmp"
            } else {
                "jpg"
            }
        })
        .unwrap_or("jpg");

    let bytes = match response.bytes().await {
        Ok(b) if !b.is_empty() => b,
        _ => return Ok(None),
    };

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let covers_path = format!("{}/.config/rockbox.org/covers", home);
    std::fs::create_dir_all(&covers_path)?;

    let album_md5 = md5::compute(album_key.as_bytes());
    let filename = format!("{:x}.{}", album_md5, ext);
    std::fs::write(format!("{}/{}", covers_path, filename), &bytes)?;

    Ok(Some(filename))
}
