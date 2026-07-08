//! Player CLI — decodes with the Rockbox codecs and plays through cpal:
//!
//! ```sh
//! cargo run --example play -- /path/to/song.opus
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rockbox_codecs::Decoder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: play <audio-file>")?;

    let mut dec = Decoder::open(&path)?;
    let meta = dec.metadata().clone();
    let secs = meta.duration.as_secs();
    println!("codec   : {}", meta.codec);
    println!("title   : {}", meta.title);
    println!("artist  : {}", meta.artist);
    println!("album   : {}", meta.album);
    println!("length  : {}:{:02}", secs / 60, secs % 60);

    // First chunk tells us the codec's real output rate (e.g. opus → 48 kHz).
    let Some(first) = dec.next_chunk() else {
        return Err(format!("no audio decoded (status {:?})", dec.status()).into());
    };
    let sample_rate = first.sample_rate;
    println!("stream  : {} Hz, {} kbit/s", sample_rate, meta.bitrate);

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default audio output device")?;
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let queue: Arc<Mutex<VecDeque<i16>>> = Arc::new(Mutex::new(VecDeque::new()));
    queue.lock().unwrap().extend(first.pcm.iter());

    let queue_out = Arc::clone(&queue);
    let stream = device.build_output_stream(
        &config,
        move |data: &mut [f32], _| {
            let mut q = queue_out.lock().unwrap();
            for sample in data.iter_mut() {
                *sample = q.pop_front().map(|s| s as f32 / 32768.0).unwrap_or(0.0);
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;
    stream.play()?;

    // Keep ~2 s buffered; the codec thread blocks on the channel otherwise.
    let high_water = sample_rate as usize * 2 * 2;
    while let Some(chunk) = dec.next_chunk() {
        {
            let mut q = queue.lock().unwrap();
            q.extend(chunk.pcm.iter());
        }
        while queue.lock().unwrap().len() > high_water {
            print!(
                "\r{:>3}:{:02} / {}:{:02} ",
                dec.elapsed().as_secs() / 60,
                dec.elapsed().as_secs() % 60,
                secs / 60,
                secs % 60
            );
            use std::io::Write;
            std::io::stdout().flush().ok();
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    while !queue.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(50));
    }
    println!("\ndone (status {:?})", dec.status());
    Ok(())
}
