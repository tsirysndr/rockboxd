//! Minimal symphonia + cpal player running the Rockbox DSP pipeline, with
//! the 10-band EQ loaded from rockboxd's settings file
//! (`~/.config/rockbox.org/settings.toml`, `eq_enabled` + `[[eq_band_settings]]`).
//!
//!     cargo run --release -p rockbox-dsp --example play -- /path/to/track.flac
//!
//! Decode (symphonia) → Rockbox DSP (EQ + resample to device rate) → cpal.

use std::collections::VecDeque;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::probe::Hint;

use rockbox_dsp::{compressor_settings, eq_band_setting, Dsp, EQ_NUM_BANDS};

/* ~/.config/rockbox.org/settings.toml — only the EQ part matters here */

#[derive(serde::Deserialize, Default)]
struct Settings {
    #[serde(default)]
    eq_enabled: bool,
    #[serde(default)]
    eq_band_settings: Vec<EqBand>,
    #[serde(default)]
    bass: i32, // dB (tone control low shelf)
    #[serde(default)]
    treble: i32, // dB (tone control high shelf)
    #[serde(default)]
    bass_cutoff: i32, // Hz, 0 = default 200
    #[serde(default)]
    treble_cutoff: i32, // Hz, 0 = default 3500
    #[serde(default)]
    surround_enabled: i32, // Haas delay in ms, 0 = off
    #[serde(default)]
    surround_balance: i32, // %
    #[serde(default)]
    surround_fx1: i32, // Hz
    #[serde(default)]
    surround_fx2: i32, // Hz
    #[serde(default = "default_stereo_width")]
    stereo_width: i32, // %, audible with channel_config = custom (2)
    #[serde(default)]
    channel_config: i32, // SOUND_CHAN_*: 0 stereo, 1 mono, 2 custom, …
    #[serde(default)]
    stereosw_mode: i32, // no DSP backing — read and reported only
    #[serde(default)]
    compressor_settings: CompressorToml,
}

fn default_stereo_width() -> i32 {
    100
}

#[derive(serde::Deserialize, Default, Clone, Copy)]
struct CompressorToml {
    #[serde(default)]
    threshold: i32, // dB, 0 = compressor off
    #[serde(default)]
    makeup_gain: i32, // 0 = off, 1 = auto
    #[serde(default)]
    ratio: i32, // index: 2:1, 4:1, 6:1, 10:1, limit
    #[serde(default)]
    knee: i32, // index: hard … soft
    #[serde(default)]
    release_time: i32, // ms
    #[serde(default)]
    attack_time: i32, // ms
}

#[derive(serde::Deserialize, Clone, Copy)]
struct EqBand {
    cutoff: i32, // Hz
    q: i32,      // Q × 10
    gain: i32,   // dB × 10
}

fn settings_path() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    Path::new(&home).join(".config/rockbox.org/settings.toml")
}

fn load_settings() -> Settings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match toml::from_str(&text) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("warning: failed to parse {}: {e}", path.display());
                Settings::default()
            }
        },
        Err(_) => {
            eprintln!("warning: {} not found, EQ off", path.display());
            Settings::default()
        }
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: play <audio-file>");
        std::process::exit(2);
    });
    let settings = load_settings();

    /* Output device */
    let device = cpal::default_host()
        .default_output_device()
        .expect("no output device");
    let config = device.default_output_config().expect("no output config");
    let out_rate = config.sample_rate().0;
    let out_channels = config.channels() as usize;
    println!(
        "output: {} @ {out_rate} Hz, {out_channels} ch, {:?}",
        device.name().unwrap_or_default(),
        config.sample_format()
    );

    /* Decoder + DSP thread → bounded channel of interleaved stereo i16 */
    let (tx, rx) = mpsc::sync_channel::<Vec<i16>>(16);
    let queued = Arc::new(AtomicUsize::new(0)); // samples in flight
    let done = Arc::new(AtomicBool::new(false));

    let decoder_thread = {
        let queued = Arc::clone(&queued);
        let done = Arc::clone(&done);
        let path = path.clone();
        std::thread::spawn(move || {
            decode_loop(&path, out_rate, &settings, tx, &queued);
            done.store(true, Ordering::Release);
        })
    };

    /* cpal stream pulling from the channel */
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config.into(), rx, &queued),
        cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config.into(), rx, &queued),
        cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config.into(), rx, &queued),
        f => panic!("unsupported sample format {f:?}"),
    };
    stream.play().expect("failed to start stream");

    /* Wait for decode to finish, then for the queue to drain */
    decoder_thread.join().unwrap();
    while queued.load(Ordering::Acquire) > 0 {
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(200)); // let the device drain
    println!("done.");
}

fn decode_loop(
    path: &str,
    out_rate: u32,
    settings: &Settings,
    tx: mpsc::SyncSender<Vec<i16>>,
    queued: &AtomicUsize,
) {
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("cannot open {path}: {e}");
        std::process::exit(1);
    });
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &Default::default(), &Default::default())
        .expect("unsupported format");
    let mut format = probed.format;

    let track = format.default_track().expect("no audio track").clone();
    let track_id = track.id;
    let track_rate = track.codec_params.sample_rate.expect("unknown sample rate");
    let track_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &Default::default())
        .expect("unsupported codec");

    println!("input : {path} @ {track_rate} Hz, {track_channels} ch");

    /* Rockbox DSP: input at track rate, output resampled to device rate */
    let mut dsp = Dsp::new(out_rate);
    dsp.set_input_frequency(track_rate);

    if settings.eq_enabled && !settings.eq_band_settings.is_empty() {
        println!(
            "eq    : enabled ({} bands)",
            settings.eq_band_settings.len()
        );
        for (i, band) in settings
            .eq_band_settings
            .iter()
            .take(EQ_NUM_BANDS)
            .enumerate()
        {
            println!(
                "        band {i}: {} Hz  Q {:.1}  {:+.1} dB",
                band.cutoff,
                band.q as f32 / 10.0,
                band.gain as f32 / 10.0
            );
            dsp.set_eq_band_raw(
                i,
                eq_band_setting {
                    cutoff: band.cutoff,
                    q: band.q,
                    gain: band.gain,
                },
            );
        }
        dsp.eq_enable(true);
    } else {
        println!("eq    : off");
    }

    if settings.bass != 0 || settings.treble != 0 {
        let bass_hz = if settings.bass_cutoff > 0 {
            settings.bass_cutoff
        } else {
            200
        };
        let treble_hz = if settings.treble_cutoff > 0 {
            settings.treble_cutoff
        } else {
            3500
        };
        println!(
            "tone  : bass {:+} dB @ {bass_hz} Hz, treble {:+} dB @ {treble_hz} Hz",
            settings.bass, settings.treble
        );
        dsp.set_tone_cutoffs(settings.bass_cutoff, settings.treble_cutoff);
        dsp.set_tone(settings.bass, settings.treble);
    } else {
        println!("tone  : off");
    }

    if settings.surround_enabled > 0 {
        println!(
            "srnd  : {} ms delay, balance {}%, fx1 {} Hz, fx2 {} Hz",
            settings.surround_enabled,
            settings.surround_balance,
            settings.surround_fx1,
            settings.surround_fx2
        );
        dsp.set_surround(
            settings.surround_enabled,
            settings.surround_balance,
            settings.surround_fx1,
            settings.surround_fx2,
        );
    } else {
        println!("srnd  : off");
    }

    /* stereo_width only takes effect with channel_config = custom (2) */
    dsp.set_stereo_width(settings.stereo_width);
    dsp.set_channel_config(settings.channel_config);
    println!(
        "chan  : config {} , stereo width {}%{}",
        settings.channel_config,
        settings.stereo_width,
        if settings.channel_config == 2 {
            ""
        } else {
            " (width needs config = 2/custom)"
        }
    );
    if settings.stereosw_mode != 0 {
        println!(
            "        stereosw_mode = {} (no DSP effect, ignored)",
            settings.stereosw_mode
        );
    }

    let comp = settings.compressor_settings;
    if comp.threshold != 0 {
        println!(
            "comp  : threshold {} dB, ratio idx {}, knee idx {}, attack {} ms, release {} ms, makeup {}",
            comp.threshold, comp.ratio, comp.knee, comp.attack_time, comp.release_time,
            if comp.makeup_gain == 1 { "auto" } else { "off" }
        );
        dsp.set_compressor(&compressor_settings {
            threshold: comp.threshold,
            makeup_gain: comp.makeup_gain,
            ratio: comp.ratio,
            knee: comp.knee,
            release_time: comp.release_time,
            attack_time: comp.attack_time,
        });
    } else {
        println!("comp  : off");
    }

    let mut sample_buf: Option<SampleBuffer<i16>> = None;
    let mut stereo: Vec<i16> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(_) => break, // EOF (symphonia reports it as IoError)
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymphoniaError::DecodeError(e)) => {
                eprintln!("decode error (skipping packet): {e}");
                continue;
            }
            Err(_) => break,
        };

        let spec = *decoded.spec();
        if sample_buf.as_ref().map_or(true, |b| {
            b.capacity() < decoded.capacity() * spec.channels.count()
        }) {
            sample_buf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        let ch = spec.channels.count();

        /* Fold to interleaved stereo */
        stereo.clear();
        match ch {
            1 => {
                for &s in samples {
                    stereo.push(s);
                    stereo.push(s);
                }
            }
            2 => stereo.extend_from_slice(samples),
            _ => {
                for frame in samples.chunks_exact(ch) {
                    stereo.push(frame[0]);
                    stereo.push(frame[1]);
                }
            }
        }

        /* Through the Rockbox pipeline */
        let mut out = Vec::new();
        dsp.process(&stereo, &mut out);
        if !out.is_empty() {
            queued.fetch_add(out.len(), Ordering::AcqRel);
            if tx.send(out).is_err() {
                break; // output stream gone
            }
        }
    }
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    rx: mpsc::Receiver<Vec<i16>>,
    queued: &Arc<AtomicUsize>,
) -> cpal::Stream
where
    T: cpal::SizedSample + cpal::FromSample<i16>,
{
    let channels = config.channels as usize;
    let queued = Arc::clone(queued);
    let mut pending: VecDeque<i16> = VecDeque::new();

    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels) {
                    while pending.len() < 2 {
                        match rx.try_recv() {
                            Ok(chunk) => pending.extend(chunk),
                            Err(_) => break,
                        }
                    }
                    let l = pending.pop_front().unwrap_or(0);
                    let r = pending.pop_front().unwrap_or(0);
                    queued
                        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |q| {
                            Some(q.saturating_sub(2))
                        })
                        .ok();
                    frame[0] = T::from_sample(l);
                    if channels > 1 {
                        frame[1] = T::from_sample(r);
                        for s in frame.iter_mut().skip(2) {
                            *s = T::from_sample(0i16);
                        }
                    }
                }
            },
            |e| eprintln!("stream error: {e}"),
            None,
        )
        .expect("failed to build output stream")
}
