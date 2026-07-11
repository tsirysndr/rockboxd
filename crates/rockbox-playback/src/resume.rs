//! Auto-persist / restore of the current playlist and the exact playback
//! position, mirroring Rockbox's *resume* feature.
//!
//! Rockbox stores two things so it can pick up exactly where it left off:
//! the playlist itself (an `.m3u8` list of file paths) and the resume
//! *position* — `resume_index` (which track) plus `resume_elapsed` (how far
//! into it) — in `global_status`, updated on every track change, pause and
//! shutdown and cleared when the playlist ends naturally
//! (`apps/playlist.c:playlist_update_resume_info`).
//!
//! Here both are folded into one file: a **valid `.m3u8`** (so any other
//! player can read the track list) whose header comments carry the resume
//! index and elapsed milliseconds. Writes are atomic (temp + rename) so a
//! crash mid-write can never corrupt it.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::m3u;

const INDEX_TAG: &str = "#RESUME-INDEX:";
const ELAPSED_TAG: &str = "#RESUME-ELAPSED:";

/// A restorable snapshot of the queue and the exact position within it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeState {
    /// The full queue, in order.
    pub tracks: Vec<PathBuf>,
    /// Index of the track that was playing.
    pub index: usize,
    /// Playback position within that track (what the listener last heard).
    pub elapsed: Duration,
}

/// Serialize `state` to `path` atomically as an extended `.m3u8`.
pub fn save(path: &Path, state: &ResumeState) -> io::Result<()> {
    let mut out = String::with_capacity(64 + state.tracks.len() * 32);
    out.push_str("#EXTM3U\n");
    out.push_str(INDEX_TAG);
    out.push_str(&state.index.to_string());
    out.push('\n');
    out.push_str(ELAPSED_TAG);
    out.push_str(&(state.elapsed.as_millis() as u64).to_string());
    out.push('\n');
    m3u::append_paths(&mut out, &state.tracks);
    m3u::atomic_write(path, &out)
}

/// Load a snapshot from `path`. Returns `None` when the file is missing,
/// unreadable or holds no tracks. Missing resume headers default to index 0
/// / elapsed 0, so a plain `.m3u8` resumes from its start. An index past the
/// end of the track list is clamped to the first track.
pub fn load(path: &Path) -> Option<ResumeState> {
    let content = std::fs::read_to_string(path).ok()?;

    let mut index = 0usize;
    let mut elapsed_ms = 0u64;
    for line in content.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix(INDEX_TAG) {
            index = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix(ELAPSED_TAG) {
            elapsed_ms = v.trim().parse().unwrap_or(0);
        }
    }

    let tracks: Vec<PathBuf> = m3u::parse(&content, path.parent())
        .into_iter()
        .map(|e| e.path)
        .collect();
    if tracks.is_empty() {
        return None;
    }
    if index >= tracks.len() {
        index = 0;
    }
    Some(ResumeState {
        tracks,
        index,
        elapsed: Duration::from_millis(elapsed_ms),
    })
}

/// Remove the resume file (called when the playlist ends naturally, so the
/// next launch doesn't resume a finished queue). Missing file is not an
/// error.
pub fn clear(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rbresume_{}_{}", std::process::id(), name))
    }

    #[test]
    fn round_trips_position() {
        let path = tmp("rt.m3u8");
        let state = ResumeState {
            tracks: vec![PathBuf::from("/music/a.flac"), PathBuf::from("/m/b c.mp3")],
            index: 1,
            elapsed: Duration::from_millis(12_345),
        };
        save(&path, &state).unwrap();
        assert_eq!(load(&path), Some(state));
        clear(&path);
        assert_eq!(load(&path), None);
    }

    #[test]
    fn resume_file_is_valid_m3u8() {
        // The stored file must parse as a normal playlist too.
        let path = tmp("valid.m3u8");
        save(
            &path,
            &ResumeState {
                tracks: vec![PathBuf::from("/x/a.flac"), PathBuf::from("/x/b.flac")],
                index: 1,
                elapsed: Duration::from_secs(3),
            },
        )
        .unwrap();
        let paths = m3u::read_paths(&path).unwrap();
        assert_eq!(
            paths,
            vec![PathBuf::from("/x/a.flac"), PathBuf::from("/x/b.flac")]
        );
        clear(&path);
    }

    #[test]
    fn clamps_out_of_range_index() {
        let path = tmp("clamp.m3u8");
        save(
            &path,
            &ResumeState {
                tracks: vec![PathBuf::from("only.flac")],
                index: 9,
                elapsed: Duration::ZERO,
            },
        )
        .unwrap();
        assert_eq!(load(&path).unwrap().index, 0);
        clear(&path);
    }

    #[test]
    fn plain_m3u8_resumes_from_start() {
        let path = tmp("plain.m3u8");
        std::fs::write(&path, "#EXTM3U\n/music/a.flac\n").unwrap();
        let st = load(&path).unwrap();
        assert_eq!(st.index, 0);
        assert_eq!(st.elapsed, Duration::ZERO);
        clear(&path);
    }
}
