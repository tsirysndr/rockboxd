//! Push a 1 kHz sine through the Rockbox DSP and show the EQ working:
//! a −12 dB peaking filter centered on the tone should attenuate it ~4×.
//!
//!     cargo run --release -p rockbox-dsp --example eq_sine

use rockbox_dsp::Dsp;

const SAMPLE_RATE: u32 = 44100;
const FREQ_HZ: f64 = 1000.0;
const FRAMES: usize = SAMPLE_RATE as usize; // 1 second

fn sine_stereo(frames: usize) -> Vec<i16> {
    let mut pcm = Vec::with_capacity(frames * 2);
    for n in 0..frames {
        let t = n as f64 / SAMPLE_RATE as f64;
        let s = (0.25 * (2.0 * std::f64::consts::PI * FREQ_HZ * t).sin() * 32767.0) as i16;
        pcm.push(s); // L
        pcm.push(s); // R
    }
    pcm
}

fn rms(pcm: &[i16]) -> f64 {
    let sum: f64 = pcm.iter().map(|&s| (s as f64) * (s as f64)).sum();
    (sum / pcm.len() as f64).sqrt()
}

fn main() {
    let input = sine_stereo(FRAMES);
    let mut dsp = Dsp::new(SAMPLE_RATE);

    // Pass 1: EQ off
    let mut flat = Vec::new();
    let produced = dsp.process(&input, &mut flat);
    println!(
        "EQ off : {produced} frames out, RMS {:.1} (input RMS {:.1})",
        rms(&flat),
        rms(&input)
    );

    // Pass 2: −12 dB peaking filter at 1 kHz, Q 0.7
    dsp.flush();
    dsp.set_eq_band(5, 1000, 0.7, -12.0);
    dsp.eq_enable(true);

    let mut cut = Vec::new();
    let produced = dsp.process(&input, &mut cut);
    let ratio = rms(&flat) / rms(&cut);
    println!(
        "EQ -12dB@1kHz : {produced} frames out, RMS {:.1} ({:.1}x quieter, {:.1} dB)",
        rms(&cut),
        ratio,
        20.0 * ratio.log10()
    );

    assert!(produced > 0, "DSP produced no output");
    assert!(
        ratio > 2.5 && ratio < 6.0,
        "expected ~4x attenuation, got {ratio:.2}x"
    );
    println!("OK — Rockbox EQ is running standalone.");
}
