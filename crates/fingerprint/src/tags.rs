//! Writing identified metadata back into the file, the way Picard does.
//!
//! Optional, and off unless asked for. The database always gets the identified
//! metadata; the *file* only gets it when the user has said to write tags,
//! because rewriting someone's music files is not a side effect a background
//! pass should have by default.
//!
//! The rule when it is on: **fill, never overwrite.** Only fields the caller
//! passes are touched, and the caller only passes what the file didn't already
//! say. A wrong AcoustID match should cost an empty tag staying empty, never a
//! correct tag being replaced by a wrong one.

use anyhow::{anyhow, Result};
use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag, TagExt};
use std::path::Path;

/// The fields to write. `None` means leave whatever is in the file alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TagUpdate {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    /// MusicBrainz ids, so a later Picard run — or a later pass of this one —
    /// recognises the file as already identified.
    pub recording_id: Option<String>,
    pub artist_id: Option<String>,
    pub release_id: Option<String>,
    pub release_group_id: Option<String>,
}

impl TagUpdate {
    /// Whether there is anything here to write.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Write `update` into the file's tags.
///
/// Blocking file I/O; call it off the async runtime.
pub fn write(path: impl AsRef<Path>, update: &TagUpdate) -> Result<()> {
    if update.is_empty() {
        return Ok(());
    }
    let path = path.as_ref();

    let mut tagged = Probe::open(path)?.read()?;
    // An untagged file — exactly the case this whole pass exists for — has no
    // tag to fill, so it gets the one its format uses.
    let tag_type = tagged.primary_tag_type();
    if tagged.tag(tag_type).is_none() {
        tagged.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged
        .tag_mut(tag_type)
        .ok_or_else(|| anyhow!("{} supports no writable tag", path.display()))?;

    if let Some(ref title) = update.title {
        tag.set_title(title.clone());
    }
    if let Some(ref artist) = update.artist {
        tag.set_artist(artist.clone());
    }
    if let Some(ref album) = update.album {
        tag.set_album(album.clone());
    }
    if let Some(ref album_artist) = update.album_artist {
        tag.insert_text(ItemKey::AlbumArtist, album_artist.clone());
    }
    if let Some(year) = update.year {
        tag.set_year(year);
    }
    if let Some(track) = update.track_number {
        tag.set_track(track);
    }
    if let Some(disc) = update.disc_number {
        tag.set_disk(disc);
    }
    for (key, value) in [
        (ItemKey::MusicBrainzRecordingId, &update.recording_id),
        (ItemKey::MusicBrainzArtistId, &update.artist_id),
        (ItemKey::MusicBrainzReleaseId, &update.release_id),
        (ItemKey::MusicBrainzReleaseGroupId, &update.release_group_id),
    ] {
        if let Some(value) = value {
            // Not every tag format has a home for these (a WAV INFO chunk
            // doesn't). A format that can't hold an MBID is not a reason to
            // abandon the title and artist that were already set.
            if !tag.insert_text(key.clone(), value.clone()) {
                tracing::debug!("{} cannot store {key:?}", path.display());
            }
        }
    }

    tag.save_to_path(path, WriteOptions::default())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing to write must not open the file at all — the pass calls this for
    /// every identified track, including the fully-tagged ones.
    #[test]
    fn an_empty_update_touches_nothing() {
        let update = TagUpdate::default();
        assert!(update.is_empty());
        assert!(write("/nonexistent/none.mp3", &update).is_ok());
    }

    #[test]
    fn an_update_with_one_field_is_not_empty() {
        let update = TagUpdate {
            title: Some("Tainted Love".into()),
            ..Default::default()
        };
        assert!(!update.is_empty());
        // And it does reach the filesystem, where a missing file is an error.
        assert!(write("/nonexistent/none.mp3", &update).is_err());
    }
}
