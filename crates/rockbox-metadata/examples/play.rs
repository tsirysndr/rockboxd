//! Minimal player CLI:
//!
//! ```sh
//! cargo run --example play -- /path/to/song.flac
//! ```
//!
//! Reads the tags with rockbox-metadata (title / artist / duration /
//! ReplayGain / …), decodes the file with symphonia, applies the parsed
//! ReplayGain track gain, and plays the PCM through cpal.

use std::collections::VecDeque;
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: play <audio-file>")?;

    // ---- tags via rockbox-metadata --------------------------------------
    let meta = rockbox_metadata::read(&path)?;

    let secs = meta.duration.as_secs();
    println!("file      : {path}");
    println!("codec     : {} (afmt {})", meta.codec, meta.codec_id);
    println!("title     : {}", meta.title);
    println!("artist    : {}", meta.artist);
    println!("album     : {}", meta.album);
    if !meta.genre.is_empty() {
        println!("genre     : {}", meta.genre);
    }
    if let (Some(track), Some(year)) = (meta.track_number, meta.year) {
        println!("track/year: {track} / {year}");
    }
    println!("length    : {}:{:02}", secs / 60, secs % 60);
    println!(
        "stream    : {} Hz, {} kbit/s{}",
        meta.sample_rate,
        meta.bitrate,
        if meta.vbr { " (VBR)" } else { "" }
    );
    if let Some(db) = meta.replaygain.track_gain_db {
        println!("replaygain: {db:+.2} dB (track)");
    }
    if let Some(art) = meta.album_art {
        println!(
            "album art : {:?}, {} bytes at offset {}",
            art.kind, art.size, art.offset
        );
    }

    // Linear ReplayGain factor from the raw Q7.24 value (1.0 when untagged).
    let gain = if meta.replaygain.raw_track_gain != 0 {
        meta.replaygain.raw_track_gain as f32 / (1u32 << 24) as f32
    } else {
        1.0
    };

    // ---- decoder (symphonia) ---------------------------------------------
    let file = File::open(&path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = Path::new(&path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format.default_track().ok_or("no default audio track")?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.unwrap_or(meta.sample_rate);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    // ---- output (cpal) ----------------------------------------------------
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default audio output device")?;
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let queue_out = Arc::clone(&queue);
    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            let mut q = queue_out.lock().unwrap();
            for sample in data.iter_mut() {
                *sample = q.pop_front().unwrap_or(0.0);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;
    stream.play()?;

    // ---- decode loop ------------------------------------------------------
    // Keep at most ~2 s of interleaved stereo queued.
    let high_water = sample_rate as usize * 2 * 2;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break, // EOF or unrecoverable
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymphoniaError::DecodeError(_)) => continue, // skip bad frame
            Err(_) => break,
        };

        let buf = sample_buf.get_or_insert_with(|| {
            SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec())
        });
        buf.copy_interleaved_ref(decoded);

        {
            let mut q = queue.lock().unwrap();
            match channels {
                1 => {
                    for &s in buf.samples() {
                        let s = s * gain;
                        q.push_back(s);
                        q.push_back(s);
                    }
                }
                _ => {
                    for frame in buf.samples().chunks_exact(channels) {
                        q.push_back(frame[0] * gain);
                        q.push_back(frame[1] * gain);
                    }
                }
            }
        }

        while queue.lock().unwrap().len() > high_water {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    // Drain what's left in the queue before exiting.
    while !queue.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(stream);

    Ok(())
}
