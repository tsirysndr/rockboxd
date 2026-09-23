//! Decoding just enough audio to fingerprint it.
//!
//! **On agreeing with `fpcalc`.** Fed audio already at 11025 Hz these
//! fingerprints are byte-identical to Chromaprint's own — the `examples/fpcalc`
//! binary exists to check exactly that. Anything else has to be resampled
//! first, and rusty-chromaprint resamples with rubato where Chromaprint uses
//! ffmpeg's, so a 44.1 kHz track comes out very slightly different: measured on
//! a full-length MP3, 0.95 % of bits differ and 76 % of sub-fingerprints are
//! identical, which [`crate::similarity`] scores as a perfect 1.0 match. That
//! is far inside what AcoustID tolerates — it exists to match a FLAC against
//! its own 128 kbps re-encode, which differ by very much more than two
//! resamplers do — so the fingerprints look up and match normally. Worth
//! knowing before comparing output to `fpcalc` byte for byte and concluding
//! something is broken.
//!
//! Deliberately not sharing `rockbox-analysis`'s decoder. That one buffers the
//! whole track in f32 because the loudness meter needs every frame; a
//! fingerprint needs the first two minutes and nothing else, so this one
//! streams packets straight into the fingerprinter and never holds more than
//! one packet at a time. On a 10-minute FLAC that is the difference between
//! ~200 MB and ~64 KB — and the pass runs several files at once.

use anyhow::{Error, Result};
use rusty_chromaprint::{Configuration, Fingerprinter};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// How much audio goes into the fingerprint.
///
/// What `fpcalc` uses by default, and therefore what the fingerprints in the
/// AcoustID index were built from. A longer fingerprint is not a better one
/// here — it is a *different* one, and would match nothing.
pub const FINGERPRINT_SECONDS: f32 = 120.0;

/// What one decode pass yields.
pub struct Fingerprinted {
    /// The raw sub-fingerprints, before compression. Kept because matching two
    /// fingerprints against each other works on these, not on the string.
    pub raw: Vec<u32>,
    /// The whole track's length in seconds — not the fingerprinted prefix.
    ///
    /// AcoustID matches on fingerprint *and* duration, and rejects a candidate
    /// whose duration is more than a few seconds out. Reporting 120 for every
    /// track longer than two minutes would mean never matching anything.
    pub duration: f32,
}

/// Fingerprint an audio file.
pub fn fingerprint_file(path: &Path, config: &Configuration) -> Result<Fingerprinted> {
    let file = File::open(path)?;
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(extension);
    }

    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let probed = symphonia::default::get_probe().format(
        &hint,
        stream,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;

    let track = format
        .default_track()
        .ok_or_else(|| Error::msg("the file has no audio track"))?;
    let track_id = track.id;
    // The container's own duration, when it states one. Every local file
    // written by a sane encoder does; the fallback below is for the ones that
    // don't.
    let claimed_duration = match (track.codec_params.n_frames, track.codec_params.sample_rate) {
        (Some(frames), Some(rate)) if rate > 0 => Some(frames as f32 / rate as f32),
        _ => None,
    };
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut printer = Fingerprinter::new(config);
    let mut started = false;
    let mut buffer: Option<SampleBuffer<i16>> = None;
    let mut sample_rate = 0u32;
    let mut frames_seen: u64 = 0;
    // Frames past this go to the duration count only — the fingerprinter has
    // had its two minutes.
    let mut frames_wanted = u64::MAX;

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            // End of stream, and also what a truncated file looks like. Either
            // way, what has been decoded so far is a usable fingerprint.
            Err(SymphoniaError::IoError(_)) | Err(SymphoniaError::ResetRequired) => break,
            Err(cause) => return Err(cause.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }

        let audio = match decoder.decode(&packet) {
            Ok(audio) => audio,
            // One corrupt packet costs a few milliseconds of audio, not the
            // whole track.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(cause) => return Err(cause.into()),
        };

        let spec = *audio.spec();
        let channels = spec.channels.count().max(1);
        if !started {
            sample_rate = spec.rate;
            printer.start(spec.rate, channels as u32)?;
            frames_wanted = (FINGERPRINT_SECONDS * spec.rate as f32) as u64;
            started = true;
        }

        let buffer =
            buffer.get_or_insert_with(|| SampleBuffer::<i16>::new(audio.capacity() as u64, spec));
        buffer.copy_interleaved_ref(audio);
        let samples = buffer.samples();
        let frames = (samples.len() / channels) as u64;

        if frames_seen < frames_wanted {
            let take = (frames_wanted - frames_seen).min(frames) as usize;
            printer.consume(&samples[..take * channels]);
        } else if claimed_duration.is_some() {
            // Past the fingerprint window with a duration already in hand:
            // there is nothing left to learn from the rest of the file.
            frames_seen += frames;
            break;
        }
        frames_seen += frames;
    }

    if !started || sample_rate == 0 {
        return Err(Error::msg("nothing decoded"));
    }

    printer.finish();
    let raw = printer.fingerprint().to_vec();
    if raw.is_empty() {
        // Under ~3 seconds of audio produces no sub-fingerprints at all. Not an
        // error in the file, but not something AcoustID can match either.
        return Err(Error::msg("too short to fingerprint"));
    }

    Ok(Fingerprinted {
        raw,
        duration: claimed_duration.unwrap_or(frames_seen as f32 / sample_rate as f32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        assert!(fingerprint_file(
            Path::new("/nonexistent/none.mp3"),
            &Configuration::default()
        )
        .is_err());
    }
}
