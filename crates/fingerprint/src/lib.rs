//! Acoustic fingerprints, and what they can be traded for.
//!
//! A [Chromaprint] fingerprint is what a track *sounds* like, boiled down to a
//! couple of kilobytes. Two files that hold the same recording — a FLAC rip and
//! a 128 kbps re-encode with the tags stripped — produce near-identical
//! fingerprints, which makes this the only identifier a library has that
//! survives transcoding, retagging, and renaming.
//!
//! Two things follow from that, and this crate does both:
//!
//! - **Identity.** The fingerprint is stored per track, so duplicates can be
//!   found and a file can be recognised after it has been moved or re-encoded.
//! - **Tags.** Handed to [AcoustID] it comes back as a MusicBrainz recording,
//!   which is where a title, an artist and an album for an untagged file come
//!   from — the same trade MusicBrainz Picard makes.
//!
//! Fingerprinting is local and needs nothing but the file. Identification needs
//! the network and an AcoustID API key, and is therefore optional and separate:
//! a library with no key still gets fingerprints, and still gets duplicate
//! detection out of them.
//!
//! [Chromaprint]: https://acoustid.org/chromaprint
//! [AcoustID]: https://acoustid.org/webservice

pub mod acoustid;
mod decode;
pub mod tags;

pub use acoustid::{AcoustId, Identified};
pub use decode::FINGERPRINT_SECONDS;
pub use tags::TagUpdate;

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusty_chromaprint::{match_fingerprints, Configuration, FingerprintCompressor};
use std::path::Path;

/// One track's acoustic fingerprint.
#[derive(Clone, Debug)]
pub struct Fingerprint {
    /// The compressed fingerprint, base64url with no padding — byte-for-byte
    /// what `fpcalc` prints and what the AcoustID web service expects.
    pub compressed: String,
    /// The raw sub-fingerprints. [`similarity`] works on these; the compressed
    /// string is only a transport format.
    pub raw: Vec<u32>,
    /// The whole track's duration in seconds, which AcoustID matches on
    /// alongside the fingerprint itself.
    pub duration: f32,
}

impl Fingerprint {
    /// Duration as AcoustID wants it: whole seconds.
    pub fn duration_secs(&self) -> u32 {
        self.duration.round().max(0.0) as u32
    }
}

/// The algorithm every fingerprint here is computed with.
///
/// Chromaprint's TEST2 — algorithm 2, `fpcalc`'s default, and what the AcoustID
/// index is built from. Changing it would invalidate every stored fingerprint
/// and match nothing, so it is a constant rather than a setting.
fn configuration() -> Configuration {
    Configuration::default()
}

/// Fingerprint an audio file.
///
/// Decodes the first [`FINGERPRINT_SECONDS`] of audio; everything after that is
/// only counted, for the duration. Blocking and CPU-bound — call it from a
/// blocking thread, not an async one.
pub fn fingerprint_file(path: impl AsRef<Path>) -> Result<Fingerprint> {
    let config = configuration();
    let decoded = decode::fingerprint_file(path.as_ref(), &config)?;
    let compressed = FingerprintCompressor::from(&config).compress(&decoded.raw);
    Ok(Fingerprint {
        compressed: URL_SAFE_NO_PAD.encode(compressed),
        raw: decoded.raw,
        duration: decoded.duration,
    })
}

/// How alike two fingerprints are, 0 to 1.
///
/// The fraction of the shorter fingerprint that the two have in common, which
/// is the useful question for "are these the same recording?" — a single track
/// and the same track inside a DJ mix are the same recording for a stretch, and
/// this reports that stretch honestly instead of averaging it away.
///
/// Returns `None` when the two are too short or too dissimilar to align at all.
pub fn similarity(a: &[u32], b: &[u32]) -> Option<f32> {
    let config = configuration();
    let segments = match match_fingerprints(a, b, &config) {
        Ok(segments) => segments,
        Err(cause) => {
            tracing::debug!(%cause, "fingerprints could not be aligned");
            return None;
        }
    };
    if segments.is_empty() {
        return Some(0.0);
    }

    let matched: f32 = segments.iter().map(|s| s.duration(&config)).sum();
    let shorter = a.len().min(b.len()) as f32 * config.item_duration_in_seconds();
    if shorter <= 0.0 {
        return None;
    }
    Some((matched / shorter).clamp(0.0, 1.0))
}

/// Above this, two fingerprints are the same recording.
///
/// Chosen to survive a transcode — a 320 kbps MP3 of a FLAC scores well above
/// this — while staying clear of the scores a cover version or a live take of
/// the same song produce, which are genuinely different recordings and must not
/// be folded together as duplicates.
pub const DUPLICATE_THRESHOLD: f32 = 0.9;

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire format is base64url with no padding — `+`, `/` and `=` would
    /// all have to be escaped in a query string, which is why AcoustID uses
    /// this alphabet.
    #[test]
    fn the_compressed_form_is_url_safe() {
        let config = configuration();
        let raw: Vec<u32> = (0..64)
            .map(|i: u32| i.wrapping_mul(2_654_435_761))
            .collect();
        let compressed =
            URL_SAFE_NO_PAD.encode(FingerprintCompressor::from(&config).compress(&raw));
        assert!(!compressed.is_empty());
        assert!(
            !compressed.contains(['+', '/', '=']),
            "not url-safe: {compressed}"
        );
    }

    /// A fingerprint is identical to itself; the threshold must accept that.
    #[test]
    fn a_fingerprint_matches_itself() {
        // Long enough for the matcher to have something to align.
        let raw: Vec<u32> = (0..256)
            .map(|i: u32| i.wrapping_mul(2_654_435_761))
            .collect();
        let score = similarity(&raw, &raw).expect("a fingerprint aligns with itself");
        assert!(
            score >= DUPLICATE_THRESHOLD,
            "identical fingerprints scored {score}"
        );
    }

    #[test]
    fn duration_is_reported_in_whole_seconds() {
        let fp = Fingerprint {
            compressed: String::new(),
            raw: Vec::new(),
            duration: 241.6,
        };
        assert_eq!(fp.duration_secs(), 242);
    }
}
