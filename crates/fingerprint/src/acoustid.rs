//! Trading a fingerprint for metadata, the way Picard does.
//!
//! [AcoustID] maps fingerprints to MusicBrainz recordings. Send it a
//! fingerprint and a duration, get back an AcoustID (a stable id for "this
//! recording, however it was encoded") and the MusicBrainz recordings it points
//! at, with titles, artists and releases attached.
//!
//! Two things make this a careful client rather than a `get` and a `?`:
//!
//! - **The response is a list of candidates, not an answer.** Picking one is a
//!   judgement — [`Identified::from_response`] makes it explicitly, on score
//!   first and then on how closely each candidate's duration matches the file
//!   in hand, which is what separates a track from the seven-minute extended
//!   mix that shares its fingerprint prefix.
//! - **Every field is optional.** A recording with no artist credit, a release
//!   with no date, a medium with no tracks: all normal, none an error. Nothing
//!   here fails because a field was missing; it just doesn't fill that tag.
//!
//! [AcoustID]: https://acoustid.org/webservice

use anyhow::{anyhow, Result};
use serde::Deserialize;
use std::time::Duration;

const LOOKUP_URL: &str = "https://api.acoustid.org/v2/lookup";

/// Below this score, a match is not worth writing tags from.
///
/// AcoustID scores how much of the submitted fingerprint it recognised. Under a
/// half the two recordings share a passage but are not the same thing — a remix
/// quoting an original, a track that samples another.
pub const MIN_SCORE: f32 = 0.5;

/// A duration this far from the file's own is a different recording, whatever
/// the fingerprint says.
///
/// Seven seconds is what AcoustID itself uses to group submissions.
const MAX_DURATION_DELTA: f32 = 7.0;

/// What one identification found.
///
/// Everything past `acoustid` and `score` is best-effort: the caller fills the
/// tags it got and leaves the rest alone.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Identified {
    /// The AcoustID — stable across encodings of the same recording, and
    /// therefore the key two files have in common when they are duplicates.
    pub acoustid: String,
    /// How much of the fingerprint AcoustID recognised, 0–1.
    pub score: f32,
    /// MusicBrainz recording MBID.
    pub recording_id: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub release_id: Option<String>,
    pub release_group_id: Option<String>,
    pub year: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
}

impl Identified {
    /// Whether this says anything a tag could be written from.
    ///
    /// A bare AcoustID with no recording behind it is still worth storing — it
    /// identifies the file against other files — but there is nothing in it to
    /// tag with.
    pub fn has_tags(&self) -> bool {
        self.title.is_some() || self.artist.is_some() || self.album.is_some()
    }

    /// Choose the best match in a response, for a file of `duration` seconds.
    ///
    /// Separated from the request so the choice can be tested against real
    /// response shapes without a network.
    fn from_response(response: Response, duration: f32) -> Option<Self> {
        let result = response
            .results
            .into_iter()
            .filter(|r| r.score >= MIN_SCORE)
            .max_by(|a, b| a.score.total_cmp(&b.score))?;

        let mut identified = Identified {
            acoustid: result.id,
            score: result.score,
            ..Default::default()
        };

        // Among the recordings behind one AcoustID — the same music on a
        // single, an album and a compilation — the one whose length matches
        // the file is the one the file actually is.
        let recording = result
            .recordings
            .into_iter()
            .filter(|r| match r.duration {
                Some(d) => (d as f32 - duration).abs() <= MAX_DURATION_DELTA,
                // A recording with no stated duration is still a candidate; it
                // just can't be preferred on that basis.
                None => true,
            })
            .min_by(|a, b| {
                let delta = |r: &Recording| match r.duration {
                    Some(d) => (d as f32 - duration).abs(),
                    None => MAX_DURATION_DELTA,
                };
                delta(a).total_cmp(&delta(b))
            });

        let Some(recording) = recording else {
            return Some(identified);
        };

        identified.recording_id = Some(recording.id.clone()).filter(|id| !id.is_empty());
        identified.title = non_empty(recording.title);
        identified.artist = credit_name(&recording.artists);
        identified.artist_id = recording
            .artists
            .first()
            .and_then(|a| non_empty(a.id.clone()));

        // The earliest release is the original one: the album a track was
        // first on, rather than whichever greatest-hits compilation also
        // carries it.
        let release = recording
            .releases
            .iter()
            .min_by_key(|r| r.date.as_ref().and_then(|d| d.year).unwrap_or(u32::MAX));

        if let Some(release) = release {
            identified.album = non_empty(release.title.clone());
            identified.release_id = non_empty(release.id.clone());
            identified.album_artist = credit_name(&release.artists);
            identified.year = release.date.as_ref().and_then(|d| d.year);

            // Where this recording sits on that release. The medium is the
            // disc, the track's position within it the track number.
            for medium in &release.mediums {
                if let Some(position) = medium
                    .tracks
                    .iter()
                    .find(|t| t.id.as_deref() == Some(recording.id.as_str()))
                    .and_then(|t| t.position)
                {
                    identified.track_number = Some(position);
                    identified.disc_number = medium.position;
                    break;
                }
            }
        }

        // Release groups carry the album title for a recording whose releases
        // AcoustID didn't return, and the better album artist when it did.
        if let Some(group) = recording.releasegroups.first() {
            identified.release_group_id = non_empty(group.id.clone());
            identified.album = identified.album.or_else(|| non_empty(group.title.clone()));
            identified.album_artist = identified
                .album_artist
                .or_else(|| credit_name(&group.artists));
        }

        Some(identified)
    }
}

/// An AcoustID web service client.
pub struct AcoustId {
    client: reqwest::Client,
    api_key: String,
}

impl AcoustId {
    /// Build a client, or `None` when no key is configured.
    ///
    /// Mirrors the Last.fm and MusicBrainz clients: a missing key disables the
    /// feature quietly rather than erroring at every call site. AcoustID keys
    /// are free and per-application — <https://acoustid.org/new-application>.
    pub fn from_key(api_key: Option<String>) -> Option<Self> {
        let api_key = api_key?;
        if api_key.trim().is_empty() {
            return None;
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .user_agent(concat!("rockboxd/", env!("CARGO_PKG_VERSION")))
            .build()
            .ok()?;
        Some(Self {
            client,
            api_key: api_key.trim().to_string(),
        })
    }

    /// Look one fingerprint up.
    ///
    /// `Ok(None)` means AcoustID has never seen this recording — a normal
    /// answer for a bootleg or a home recording, and one the caller should
    /// remember so it does not ask again. `Err` means the request itself
    /// failed, which is worth retrying.
    pub async fn lookup(&self, fingerprint: &str, duration: u32) -> Result<Option<Identified>> {
        // POST rather than GET: a two-minute fingerprint is a couple of
        // kilobytes of base64, past what some proxies will accept in a URL.
        let response = self
            .client
            .post(LOOKUP_URL)
            .form(&[
                ("client", self.api_key.as_str()),
                ("duration", &duration.to_string()),
                ("fingerprint", fingerprint),
                // Everything needed to fill a tag, and nothing else: the
                // recording for title and artist, releases for album, date and
                // track number, release groups as the fallback album.
                ("meta", "recordings+releasegroups+releases+tracks"),
            ])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!("acoustid lookup failed: {status}"));
        }

        let body: Response = response.json().await?;
        if body.status != "ok" {
            let message = body
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| body.status.clone());
            return Err(anyhow!("acoustid lookup failed: {message}"));
        }

        Ok(Identified::from_response(body, duration as f32))
    }
}

/// Join an artist credit the way MusicBrainz means it to read.
///
/// The join phrases are part of the credit — "Simon & Garfunkel" and "Jay-Z
/// feat. Alicia Keys" are what the credit *says*, and flattening them to commas
/// would make the artist tag disagree with every other tool in a library.
fn credit_name(artists: &[Artist]) -> Option<String> {
    if artists.is_empty() {
        return None;
    }
    let mut name = String::new();
    for (index, artist) in artists.iter().enumerate() {
        name.push_str(artist.name.trim());
        if index + 1 < artists.len() {
            let join = artist.joinphrase.as_deref().unwrap_or("");
            name.push_str(if join.is_empty() { ", " } else { join });
        }
    }
    non_empty(name)
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

// The response shape. Everything defaulted: AcoustID omits fields it has
// nothing for, and a missing release date must not fail the whole lookup.

#[derive(Debug, Default, Deserialize)]
struct Response {
    #[serde(default)]
    status: String,
    #[serde(default)]
    results: Vec<LookupResult>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct LookupResult {
    #[serde(default)]
    id: String,
    #[serde(default)]
    score: f32,
    #[serde(default)]
    recordings: Vec<Recording>,
}

#[derive(Debug, Deserialize)]
struct Recording {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    duration: Option<f64>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default)]
    releases: Vec<Release>,
    #[serde(default)]
    releasegroups: Vec<ReleaseGroup>,
}

#[derive(Debug, Deserialize)]
struct Artist {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    joinphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Release {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    date: Option<PartialDate>,
    #[serde(default)]
    artists: Vec<Artist>,
    #[serde(default)]
    mediums: Vec<Medium>,
}

#[derive(Debug, Deserialize)]
struct ReleaseGroup {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artists: Vec<Artist>,
}

#[derive(Debug, Deserialize)]
struct Medium {
    #[serde(default)]
    position: Option<u32>,
    #[serde(default)]
    tracks: Vec<TrackRef>,
}

#[derive(Debug, Deserialize)]
struct TrackRef {
    /// The MusicBrainz *recording* id — what ties this slot on the disc back to
    /// the recording that was looked up.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    position: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct PartialDate {
    #[serde(default)]
    year: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Response {
        serde_json::from_str(json).expect("response parses")
    }

    /// The shape a real lookup comes back in, with every level populated.
    const FULL: &str = r#"{
      "status": "ok",
      "results": [{
        "id": "9ff43b6a-4f16-427c-93c2-92307ca505e0",
        "score": 0.98,
        "recordings": [{
          "id": "rec-album",
          "title": "Tainted Love",
          "duration": 160,
          "artists": [{"id": "art-1", "name": "Soft Cell"}],
          "releases": [
            {
              "id": "rel-1991",
              "title": "Greatest Hits",
              "date": {"year": 1991},
              "artists": [{"id": "art-v", "name": "Various Artists"}],
              "mediums": [{"position": 2, "tracks": [{"id": "rec-album", "position": 9}]}]
            },
            {
              "id": "rel-1981",
              "title": "Non-Stop Erotic Cabaret",
              "date": {"year": 1981},
              "artists": [{"id": "art-1", "name": "Soft Cell"}],
              "mediums": [{"position": 1, "tracks": [{"id": "rec-album", "position": 3}]}]
            }
          ],
          "releasegroups": [{"id": "rg-1", "title": "Non-Stop Erotic Cabaret", "artists": []}]
        }]
      }]
    }"#;

    #[test]
    fn a_full_response_fills_every_tag() {
        let found = Identified::from_response(parse(FULL), 160.0).expect("a match");
        assert_eq!(found.acoustid, "9ff43b6a-4f16-427c-93c2-92307ca505e0");
        assert_eq!(found.title.as_deref(), Some("Tainted Love"));
        assert_eq!(found.artist.as_deref(), Some("Soft Cell"));
        assert_eq!(found.recording_id.as_deref(), Some("rec-album"));
        // The 1981 release, not the 1991 compilation: a track belongs to the
        // album it came out on.
        assert_eq!(found.album.as_deref(), Some("Non-Stop Erotic Cabaret"));
        assert_eq!(found.year, Some(1981));
        assert_eq!(found.track_number, Some(3));
        assert_eq!(found.disc_number, Some(1));
        assert!(found.has_tags());
    }

    /// The whole point of sending a duration: one fingerprint, two recordings,
    /// and only the length says which one the file is.
    #[test]
    fn the_candidate_closest_in_length_wins() {
        let json = r#"{
          "status": "ok",
          "results": [{
            "id": "acoustid-1",
            "score": 0.95,
            "recordings": [
              {"id": "radio-edit", "title": "Radio Edit", "duration": 210, "artists": [], "releases": [], "releasegroups": []},
              {"id": "extended",   "title": "Extended Mix", "duration": 430, "artists": [], "releases": [], "releasegroups": []}
            ]
          }]
        }"#;
        let edit = Identified::from_response(parse(json), 212.0).expect("a match");
        assert_eq!(edit.title.as_deref(), Some("Radio Edit"));

        let extended = Identified::from_response(parse(json), 428.0).expect("a match");
        assert_eq!(extended.title.as_deref(), Some("Extended Mix"));
    }

    /// A low score means the fingerprints share a passage, not an identity.
    /// Tagging from that is how a library ends up with a remix labelled as the
    /// original.
    #[test]
    fn a_weak_match_is_no_match() {
        let json = r#"{"status":"ok","results":[{"id":"a","score":0.21,"recordings":[]}]}"#;
        assert_eq!(Identified::from_response(parse(json), 200.0), None);
    }

    /// AcoustID knows the file but MusicBrainz has nothing behind it. The id is
    /// still worth having — it is what makes two copies of this file
    /// recognisable as duplicates — but there is nothing to tag from.
    #[test]
    fn an_id_with_no_recordings_is_still_an_identity() {
        let json = r#"{"status":"ok","results":[{"id":"bare","score":0.9,"recordings":[]}]}"#;
        let found = Identified::from_response(parse(json), 200.0).expect("a match");
        assert_eq!(found.acoustid, "bare");
        assert!(!found.has_tags());
        assert_eq!(found.title, None);
    }

    /// Nothing in the index. Not an error — most bootlegs and home recordings
    /// land here.
    #[test]
    fn an_empty_result_list_is_not_a_match() {
        assert_eq!(
            Identified::from_response(parse(r#"{"status":"ok","results":[]}"#), 200.0),
            None
        );
    }

    /// A credit is a sentence, and the join phrases are part of it.
    #[test]
    fn artist_credits_read_the_way_musicbrainz_writes_them() {
        let json = r#"{
          "status": "ok",
          "results": [{"id":"a","score":0.9,"recordings":[{
            "id": "r", "title": "Empire State of Mind", "duration": 276,
            "artists": [
              {"id":"1","name":"JAY-Z","joinphrase":" feat. "},
              {"id":"2","name":"Alicia Keys"}
            ],
            "releases": [], "releasegroups": []
          }]}]
        }"#;
        let found = Identified::from_response(parse(json), 276.0).expect("a match");
        assert_eq!(found.artist.as_deref(), Some("JAY-Z feat. Alicia Keys"));
        // The first credited artist is the one the library links the track to.
        assert_eq!(found.artist_id.as_deref(), Some("1"));
    }

    /// Two artists with no join phrase still have to be joined by something.
    #[test]
    fn a_credit_with_no_join_phrase_falls_back_to_a_comma() {
        let json = r#"{
          "status": "ok",
          "results": [{"id":"a","score":0.9,"recordings":[{
            "id": "r", "title": "T", "duration": 100,
            "artists": [{"id":"1","name":"A"},{"id":"2","name":"B"}],
            "releases": [], "releasegroups": []
          }]}]
        }"#;
        let found = Identified::from_response(parse(json), 100.0).expect("a match");
        assert_eq!(found.artist.as_deref(), Some("A, B"));
    }

    /// Releases with no dates, no mediums, no artists — all routine, and none
    /// of it may turn into a panic in a background pass.
    #[test]
    fn missing_fields_are_absent_tags_not_failures() {
        let json = r#"{
          "status": "ok",
          "results": [{"id":"a","score":0.9,"recordings":[{
            "id": "r", "title": "", "duration": null, "artists": [],
            "releases": [{"id":"rel","title":"Album","artists":[],"mediums":[]}],
            "releasegroups": []
          }]}]
        }"#;
        let found = Identified::from_response(parse(json), 100.0).expect("a match");
        assert_eq!(found.title, None, "an empty title is no title");
        assert_eq!(found.artist, None);
        assert_eq!(found.album.as_deref(), Some("Album"));
        assert_eq!(found.year, None);
        assert_eq!(found.track_number, None);
    }

    /// A release group is the fallback when no release came back.
    #[test]
    fn the_release_group_supplies_an_album_when_releases_do_not() {
        let json = r#"{
          "status": "ok",
          "results": [{"id":"a","score":0.9,"recordings":[{
            "id": "r", "title": "T", "duration": 100, "artists": [], "releases": [],
            "releasegroups": [{"id":"rg","title":"The Album","artists":[{"id":"9","name":"The Band"}]}]
          }]}]
        }"#;
        let found = Identified::from_response(parse(json), 100.0).expect("a match");
        assert_eq!(found.album.as_deref(), Some("The Album"));
        assert_eq!(found.album_artist.as_deref(), Some("The Band"));
        assert_eq!(found.release_group_id.as_deref(), Some("rg"));
    }

    /// No key configured is not an error condition — it is the default state
    /// of a fresh install, and it must disable lookups rather than break them.
    #[test]
    fn no_key_means_no_client() {
        assert!(AcoustId::from_key(None).is_none());
        assert!(AcoustId::from_key(Some(String::new())).is_none());
        assert!(AcoustId::from_key(Some("   ".into())).is_none());
        assert!(AcoustId::from_key(Some("abc123".into())).is_some());
    }
}
