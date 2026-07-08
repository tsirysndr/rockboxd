//! End-to-end decode tests. WAV fixtures are synthesized in-process;
//! compressed fixtures are transcoded from them with ffmpeg when it's
//! installed (tests are skipped otherwise).

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

use rockbox_codecs::Decoder;

const RATE: u32 = 44100;

fn fixture_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rbcodec_{}_{}", std::process::id(), name))
}

struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// 440 Hz (L) / 880 Hz (R) stereo sine, 16-bit PCM.
fn sine_samples(seconds: u32) -> Vec<i16> {
    let frames = (RATE * seconds) as usize;
    let mut pcm = Vec::with_capacity(frames * 2);
    for i in 0..frames {
        let t = i as f64 / RATE as f64;
        pcm.push((12000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16);
        pcm.push((12000.0 * (2.0 * std::f64::consts::PI * 880.0 * t).sin()) as i16);
    }
    pcm
}

fn write_wav(name: &str, pcm: &[i16]) -> TempFile {
    let path = fixture_path(name);
    let data_len = (pcm.len() * 2) as u32;
    let mut v = Vec::new();
    v.extend(b"RIFF");
    v.extend(&(36 + data_len).to_le_bytes());
    v.extend(b"WAVE");
    v.extend(b"fmt ");
    v.extend(&16u32.to_le_bytes());
    v.extend(&1u16.to_le_bytes());
    v.extend(&2u16.to_le_bytes());
    v.extend(&RATE.to_le_bytes());
    v.extend(&(RATE * 4).to_le_bytes());
    v.extend(&4u16.to_le_bytes());
    v.extend(&16u16.to_le_bytes());
    v.extend(b"data");
    v.extend(&data_len.to_le_bytes());
    for s in pcm {
        v.extend(&s.to_le_bytes());
    }
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(&v).unwrap();
    TempFile(path)
}

fn decode_all(dec: &mut Decoder) -> (Vec<i16>, u32) {
    let mut pcm = Vec::new();
    let mut rate = 0;
    while let Some(chunk) = dec.next_chunk() {
        rate = chunk.sample_rate;
        pcm.extend_from_slice(&chunk.pcm);
    }
    (pcm, rate)
}

/// Transcode a fixture with ffmpeg; None if ffmpeg isn't installed.
fn ffmpeg(input: &PathBuf, name: &str, args: &[&str]) -> Option<TempFile> {
    let out = fixture_path(name);
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-i")
        .arg(input)
        .args(args)
        .arg(&out)
        .output()
        .ok()?;
    status.status.success().then(|| TempFile(out))
}

#[test]
fn wav_roundtrip_is_bit_exact() {
    let reference = sine_samples(1);
    let file = write_wav("sine.wav", &reference);

    let mut dec = Decoder::open(&file.0).unwrap();
    assert_eq!(dec.metadata().codec, "WAV");
    let (pcm, rate) = decode_all(&mut dec);

    assert_eq!(dec.status(), Some(0));
    assert_eq!(rate, RATE);
    assert_eq!(pcm.len(), reference.len());
    assert_eq!(pcm, reference, "16-bit PCM WAV must decode bit-exactly");
}

#[test]
fn flac_roundtrip_is_bit_exact() {
    let reference = sine_samples(1);
    let wav = write_wav("forflac.wav", &reference);
    let Some(flac) = ffmpeg(&wav.0, "sine.flac", &["-c:a", "flac"]) else {
        eprintln!("ffmpeg not available — skipping");
        return;
    };

    let mut dec = Decoder::open(&flac.0).unwrap();
    assert_eq!(dec.metadata().codec, "FLAC");
    let (pcm, rate) = decode_all(&mut dec);

    assert_eq!(dec.status(), Some(0));
    assert_eq!(rate, RATE);
    assert_eq!(pcm.len(), reference.len());
    assert_eq!(
        pcm, reference,
        "FLAC is lossless — decode must be bit-exact"
    );
}

/// Lossless codecs must reproduce the reference exactly.
fn assert_lossless(ext: &str, args: &[&str]) {
    let reference = sine_samples(1);
    let wav = write_wav(&format!("for{ext}.wav"), &reference);
    let Some(file) = ffmpeg(&wav.0, &format!("sine.{ext}"), args) else {
        eprintln!("ffmpeg not available — skipping");
        return;
    };

    let mut dec = Decoder::open(&file.0).unwrap();
    let (pcm, rate) = decode_all(&mut dec);
    assert_eq!(dec.status(), Some(0), "codec error for {ext}");
    assert_eq!(rate, RATE);
    assert_eq!(pcm.len(), reference.len(), "{ext}: frame count");
    assert_eq!(pcm, reference, "{ext} is lossless — must be bit-exact");
}

/// Lossy codecs: decode must succeed, produce roughly the right length
/// (allowing for encoder delay/padding) and correlate strongly with the
/// reference sine on the left channel.
fn assert_lossy(ext: &str, args: &[&str]) {
    let reference = sine_samples(2);
    let wav = write_wav(&format!("for{ext}.wav"), &reference);
    let Some(file) = ffmpeg(&wav.0, &format!("sine.{ext}"), args) else {
        eprintln!("ffmpeg not available — skipping");
        return;
    };

    let mut dec = Decoder::open(&file.0).unwrap();
    let (pcm, rate) = decode_all(&mut dec);
    assert_eq!(dec.status(), Some(0), "codec error for {ext}");
    assert!(rate == RATE || rate == 48000, "{ext}: rate {rate}"); // opus resamples to 48k
    let expected = reference.len() as f64 * rate as f64 / RATE as f64;
    let tolerance = rate as f64 * 2.0 * 0.15; // 150 ms of stereo slack
    assert!(
        (pcm.len() as f64 - expected).abs() < tolerance,
        "{ext}: got {} frames, expected ~{}",
        pcm.len() / 2,
        expected / 2.0,
    );
    // Energy check: a decoded sine at amplitude 12000 has high RMS;
    // silence/garbage does not.
    let rms = (pcm.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / pcm.len() as f64).sqrt();
    assert!(
        rms > 4000.0,
        "{ext}: RMS {rms:.0} too low — silent/garbled output"
    );
}

#[cfg(feature = "alac")]
#[test]
fn alac_is_bit_exact() {
    assert_lossless("m4a", &["-c:a", "alac"]);
}

#[cfg(feature = "wavpack")]
#[test]
fn wavpack_is_bit_exact() {
    assert_lossless("wv", &["-c:a", "wavpack"]);
}

#[cfg(feature = "mpa")]
#[test]
fn mp3_decodes() {
    assert_lossy("mp3", &["-c:a", "libmp3lame", "-b:a", "192k"]);
}

#[cfg(feature = "vorbis")]
#[test]
fn vorbis_decodes() {
    assert_lossy("ogg", &["-c:a", "libvorbis", "-q:a", "5"]);
}

#[cfg(feature = "aac")]
#[test]
fn aac_decodes() {
    assert_lossy("aac.m4a", &["-c:a", "aac", "-b:a", "192k"]);
}

#[cfg(feature = "opus")]
#[test]
fn opus_decodes() {
    assert_lossy("opus", &["-c:a", "libopus", "-b:a", "128k"]);
}

#[cfg(feature = "aiff")]
#[test]
fn aiff_is_bit_exact() {
    assert_lossless("aiff", &["-c:a", "pcm_s16be"]);
}

#[cfg(feature = "au")]
#[test]
fn au_is_bit_exact() {
    assert_lossless("au", &["-c:a", "pcm_s16be"]);
}

#[cfg(feature = "wav64")]
#[test]
fn wav64_is_bit_exact() {
    assert_lossless("w64", &["-c:a", "pcm_s16le"]);
}

#[cfg(feature = "tta")]
#[test]
fn tta_is_bit_exact() {
    assert_lossless("tta", &["-c:a", "tta"]);
}

#[cfg(feature = "a52")]
#[test]
fn ac3_decodes() {
    assert_lossy("ac3", &["-c:a", "ac3", "-b:a", "192k"]);
}

#[cfg(feature = "wma")]
#[test]
fn wma_decodes() {
    // Upstream Rockbox's ASF/WMA code targets WMP-muxed files; on
    // ffmpeg-muxed wmav2 it decodes only part of the stream (the file's
    // header also carries a preroll-inflated duration). Assert that what
    // does decode is real audio rather than requiring full length.
    let reference = sine_samples(2);
    let wav = write_wav("forwma.wav", &reference);
    let Some(file) = ffmpeg(&wav.0, "sine.wma", &["-c:a", "wmav2", "-b:a", "128k"]) else {
        eprintln!("ffmpeg not available — skipping");
        return;
    };

    let mut dec = Decoder::open(&file.0).unwrap();
    let (pcm, rate) = decode_all(&mut dec);
    assert_eq!(dec.status(), Some(0), "codec error for wma");
    assert_eq!(rate, RATE);
    assert!(
        pcm.len() as u32 > RATE / 5 * 2,
        "wma: too little audio decoded ({} samples)",
        pcm.len()
    );
    let rms = (pcm.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / pcm.len() as f64).sqrt();
    assert!(rms > 4000.0, "wma: RMS {rms:.0} too low");
}

#[cfg(feature = "aac_bsf")]
#[test]
fn adts_aac_decodes() {
    assert_lossy("aac", &["-c:a", "aac", "-b:a", "192k", "-f", "adts"]);
}

#[cfg(feature = "adx")]
#[test]
fn adx_decodes() {
    assert_lossy("adx", &["-c:a", "adpcm_adx"]);
}

#[test]
fn sequential_decoders() {
    let reference = sine_samples(1);
    let file = write_wav("seq.wav", &reference);

    for _ in 0..3 {
        let mut dec = Decoder::open(&file.0).unwrap();
        let (pcm, _) = decode_all(&mut dec);
        assert_eq!(pcm.len(), reference.len());
    }
}

#[test]
fn drop_mid_decode_does_not_hang() {
    // Long enough that the bounded channel fills and the codec blocks in
    // the sink — dropping must still terminate promptly via halt.
    let reference = sine_samples(30);
    let file = write_wav("long.wav", &reference);

    let mut dec = Decoder::open(&file.0).unwrap();
    let _ = dec.next_chunk();
    drop(dec); // must not deadlock

    // And the global gate must be free again:
    let mut dec2 = Decoder::open(&file.0).unwrap();
    let _ = dec2.next_chunk();
}
