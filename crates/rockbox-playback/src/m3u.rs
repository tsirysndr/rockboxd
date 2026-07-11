//! First-class `.m3u` / `.m3u8` playlist support — the same UTF-8 extended
//! M3U format Rockbox reads and writes (`apps/playlist.c`).
//!
//! - **Import**: [`read`] / [`read_paths`] parse a file into entries,
//!   resolving relative paths against the playlist's directory (as Rockbox
//!   does) and reading `#EXTINF` duration/title hints.
//! - **Export / update**: [`write_paths`] / [`write_entries`] write a
//!   playlist back out atomically (temp + rename), always UTF-8 (`.m3u8`
//!   semantics) with an `#EXTM3U` header.
//!
//! The [`Player`](crate::Player) wraps these for the live queue —
//! `import_m3u`, `load_m3u`, `export_m3u`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// One playlist entry: a track path plus optional `#EXTINF` metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M3uEntry {
    /// Track path, resolved to absolute when a base directory was known.
    pub path: PathBuf,
    /// Duration from the preceding `#EXTINF`, if present and non-negative.
    pub duration: Option<Duration>,
    /// Display title from the preceding `#EXTINF`, if present.
    pub title: Option<String>,
}

impl M3uEntry {
    /// A bare entry with no `#EXTINF` metadata.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        M3uEntry {
            path: path.into(),
            duration: None,
            title: None,
        }
    }
}

/// Parse extended-M3U `content`. Relative track paths are resolved against
/// `base_dir` (typically the playlist file's directory); absolute paths and
/// `scheme://` URLs are left untouched.
pub fn parse(content: &str, base_dir: Option<&Path>) -> Vec<M3uEntry> {
    let mut entries = Vec::new();
    let mut pending_dur: Option<Duration> = None;
    let mut pending_title: Option<String> = None;

    for raw in content.lines() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            // "#EXTINF:<seconds>,<title>" — seconds may be -1 for streams.
            let (secs, title) = match rest.split_once(',') {
                Some((s, t)) => (s.trim(), Some(t.trim().to_string())),
                None => (rest.trim(), None),
            };
            pending_dur = secs
                .parse::<i64>()
                .ok()
                .filter(|s| *s >= 0)
                .map(|s| Duration::from_secs(s as u64));
            pending_title = title.filter(|t| !t.is_empty());
            continue;
        }
        if line.starts_with('#') {
            continue; // #EXTM3U and any other directive/comment
        }
        entries.push(M3uEntry {
            path: resolve(PathBuf::from(line), base_dir),
            duration: pending_dur.take(),
            title: pending_title.take(),
        });
    }
    entries
}

/// Read and parse a playlist file. Relative paths resolve against the file's
/// own directory.
pub fn read(path: &Path) -> io::Result<Vec<M3uEntry>> {
    let content = fs::read_to_string(path)?;
    Ok(parse(&content, path.parent()))
}

/// Like [`read`] but returns just the resolved track paths.
pub fn read_paths(path: &Path) -> io::Result<Vec<PathBuf>> {
    Ok(read(path)?.into_iter().map(|e| e.path).collect())
}

/// Write `paths` as a minimal `#EXTM3U` playlist (one path per line).
pub fn write_paths(path: &Path, paths: &[PathBuf]) -> io::Result<()> {
    let mut out = String::from("#EXTM3U\n");
    append_paths(&mut out, paths);
    atomic_write(path, &out)
}

/// Write full entries, emitting `#EXTINF` lines where duration/title are
/// known.
pub fn write_entries(path: &Path, entries: &[M3uEntry]) -> io::Result<()> {
    let mut out = String::from("#EXTM3U\n");
    for e in entries {
        let line = e.path.to_string_lossy();
        if line.contains('\n') {
            continue;
        }
        if e.duration.is_some() || e.title.is_some() {
            let secs = e.duration.map(|d| d.as_secs() as i64).unwrap_or(-1);
            out.push_str("#EXTINF:");
            out.push_str(&secs.to_string());
            out.push(',');
            out.push_str(e.title.as_deref().unwrap_or(""));
            out.push('\n');
        }
        out.push_str(&line);
        out.push('\n');
    }
    atomic_write(path, &out)
}

/// Append one-path-per-line (skipping paths with embedded newlines, which
/// M3U can't represent). Shared with the resume writer.
pub(crate) fn append_paths(out: &mut String, paths: &[PathBuf]) {
    for p in paths {
        let line = p.to_string_lossy();
        if line.contains('\n') {
            continue;
        }
        out.push_str(&line);
        out.push('\n');
    }
}

/// Write `content` to `path` atomically: a sibling temp file then a rename,
/// so a crash mid-write never leaves a truncated playlist. Falls back to a
/// direct write if the rename crosses a filesystem boundary.
pub(crate) fn atomic_write(path: &Path, content: &str) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, content)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) => {
            let res = fs::write(path, content);
            let _ = fs::remove_file(&tmp);
            res
        }
    }
}

fn resolve(p: PathBuf, base_dir: Option<&Path>) -> PathBuf {
    if p.is_absolute() {
        return p;
    }
    if let Some(s) = p.to_str() {
        if s.contains("://") {
            return p; // http(s)/other URL — leave as-is
        }
    }
    match base_dir {
        Some(d) => d.join(p),
        None => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_extinf_and_resolves_relative() {
        let content =
            "#EXTM3U\n#EXTINF:212,Artist - Song\nsub/a.flac\n#EXTINF:-1,Stream\nhttp://x/y\n/abs/b.mp3\n";
        let base = Path::new("/music");
        let e = parse(content, Some(base));
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].path, PathBuf::from("/music/sub/a.flac"));
        assert_eq!(e[0].duration, Some(Duration::from_secs(212)));
        assert_eq!(e[0].title.as_deref(), Some("Artist - Song"));
        assert_eq!(e[1].path, PathBuf::from("http://x/y")); // URL untouched
        assert_eq!(e[1].duration, None); // -1 → None
        assert_eq!(e[2].path, PathBuf::from("/abs/b.mp3")); // absolute untouched
    }

    #[test]
    fn handles_crlf_and_blank_lines() {
        let content = "#EXTM3U\r\n\r\n/a.flac\r\n/b.flac\r\n";
        let e = parse(content, None);
        assert_eq!(
            e.iter().map(|x| x.path.clone()).collect::<Vec<_>>(),
            vec![PathBuf::from("/a.flac"), PathBuf::from("/b.flac")]
        );
    }

    #[test]
    fn write_then_read_round_trips_paths() {
        let path = std::env::temp_dir().join(format!("rbm3u_{}_rt.m3u8", std::process::id()));
        let paths = vec![PathBuf::from("/m/a.flac"), PathBuf::from("/m/b c.mp3")];
        write_paths(&path, &paths).unwrap();
        assert_eq!(read_paths(&path).unwrap(), paths);
        let _ = fs::remove_file(&path);
    }
}
