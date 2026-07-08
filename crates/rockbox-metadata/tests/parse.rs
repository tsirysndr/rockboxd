//! Parses synthesized WAV and FLAC fixtures — no binary test assets needed.

use std::io::Write;
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rbmeta_{}_{}", std::process::id(), name))
}

struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, contents: &[u8]) -> Self {
        let path = fixture_path(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents).unwrap();
        TempFile(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Canonical 44.1 kHz / stereo / 16-bit PCM WAV with `seconds` of silence.
fn wav_fixture(seconds: u32) -> Vec<u8> {
    let sample_rate = 44100u32;
    let channels = 2u16;
    let bits = 16u16;
    let byte_rate = sample_rate * channels as u32 * (bits / 8) as u32;
    let block_align = channels * (bits / 8);
    let data_len = byte_rate * seconds;

    let mut v = Vec::new();
    v.extend(b"RIFF");
    v.extend(&(36 + data_len).to_le_bytes());
    v.extend(b"WAVE");
    v.extend(b"fmt ");
    v.extend(&16u32.to_le_bytes());
    v.extend(&1u16.to_le_bytes()); // PCM
    v.extend(&channels.to_le_bytes());
    v.extend(&sample_rate.to_le_bytes());
    v.extend(&byte_rate.to_le_bytes());
    v.extend(&block_align.to_le_bytes());
    v.extend(&bits.to_le_bytes());
    v.extend(b"data");
    v.extend(&data_len.to_le_bytes());
    v.resize(v.len() + data_len as usize, 0);
    v
}

/// FLAC header: STREAMINFO + VORBIS_COMMENT metadata blocks (no frames —
/// the parser only reads the metadata blocks).
fn flac_fixture(total_samples: u64, comments: &[&str]) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend(b"fLaC");

    // STREAMINFO (type 0), 34 bytes
    v.push(0x00);
    v.extend(&34u32.to_be_bytes()[1..]);
    v.extend(&4096u16.to_be_bytes()); // min blocksize
    v.extend(&4096u16.to_be_bytes()); // max blocksize
    v.extend(&[0u8; 3]); // min framesize (unknown)
    v.extend(&[0u8; 3]); // max framesize (unknown)
    let sample_rate = 44100u64;
    let channels = 2u64;
    let bps = 16u64;
    let packed: u64 =
        (sample_rate << 44) | ((channels - 1) << 41) | ((bps - 1) << 36) | total_samples;
    v.extend(&packed.to_be_bytes());
    v.extend(&[0u8; 16]); // MD5

    // VORBIS_COMMENT (type 4), last block
    let mut vc = Vec::new();
    let vendor = b"rockbox-metadata test";
    vc.extend(&(vendor.len() as u32).to_le_bytes());
    vc.extend(vendor);
    vc.extend(&(comments.len() as u32).to_le_bytes());
    for c in comments {
        vc.extend(&(c.len() as u32).to_le_bytes());
        vc.extend(c.as_bytes());
    }
    v.push(0x84);
    v.extend(&(vc.len() as u32).to_be_bytes()[1..]);
    v.extend(&vc);
    v
}

#[test]
fn parses_wav() {
    let file = TempFile::new("t.wav", &wav_fixture(2));
    let meta = rockbox_metadata::read(&file.0).unwrap();

    assert_eq!(meta.codec, "WAV");
    assert_eq!(meta.sample_rate, 44100);
    assert_eq!(meta.duration.as_millis(), 2000);
    assert!(meta.title.is_empty());
    assert!(meta.replaygain.is_empty());
}

#[test]
fn parses_flac_tags_and_replaygain() {
    let file = TempFile::new(
        "t.flac",
        &flac_fixture(
            441_000, // 10 s at 44.1 kHz
            &[
                "TITLE=Test Song",
                "ARTIST=Test Artist",
                "ALBUM=Test Album",
                "GENRE=Electronic",
                "TRACKNUMBER=7",
                "DATE=2024",
                "REPLAYGAIN_TRACK_GAIN=-6.50 dB",
                "REPLAYGAIN_TRACK_PEAK=0.988525",
            ],
        ),
    );
    let meta = rockbox_metadata::read(&file.0).unwrap();

    assert_eq!(meta.codec, "FLAC");
    assert_eq!(meta.title, "Test Song");
    assert_eq!(meta.artist, "Test Artist");
    assert_eq!(meta.album, "Test Album");
    assert_eq!(meta.genre, "Electronic");
    assert_eq!(meta.track_number, Some(7));
    assert_eq!(meta.sample_rate, 44100);
    // flac.c computes length from total_samples but doesn't store the
    // sample count itself (only the Vorbis/MP4 parsers fill id3->samples).
    assert_eq!(meta.duration.as_millis(), 10_000);

    let rg = meta.replaygain;
    let gain_db = rg.track_gain_db.expect("track gain parsed");
    assert!(
        (gain_db - (-6.5)).abs() < 0.01,
        "expected -6.5 dB, got {gain_db}"
    );
    let peak = rg.track_peak.expect("track peak parsed");
    assert!((peak - 0.988525).abs() < 0.001, "peak {peak}");
    // Raw Q7.24 linear gain for -6.5 dB ≈ 10^(-6.5/20) ≈ 0.4732
    let linear = rg.raw_track_gain as f64 / (1u32 << 24) as f64;
    assert!((linear - 0.4732).abs() < 0.005, "linear {linear}");
}

#[test]
fn unknown_files_error() {
    let file = TempFile::new("t.txt", b"definitely not audio");
    assert!(rockbox_metadata::read(&file.0).is_err());
    assert!(rockbox_metadata::read("/nonexistent/nope.mp3").is_err());
}

#[test]
fn probe_by_extension() {
    assert_eq!(rockbox_metadata::probe("x.mp3"), Some("MP3".to_string()));
    assert_eq!(rockbox_metadata::probe("x.flac"), Some("FLAC".to_string()));
    assert_eq!(rockbox_metadata::probe("x.opus"), Some("Opus".to_string()));
    assert_eq!(rockbox_metadata::probe("x.xyz"), None);
}
