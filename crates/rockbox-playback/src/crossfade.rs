//! Rockbox's crossfade, ported to Rust.
//!
//! Faithful to `apps/pcmbuf.c`: the fade gain is a Q16 factor
//! (`MIXFADE_UNITY = 1 << 16`), each sample is scaled with
//! `(factor * s + MIXFADE_UNITY/2) >> 16` and the mixed result is
//! saturated to 16 bits. Fade-in and fade-out are independent linear
//! ramps (Rockbox uses a Bresenham stepper — reproduced here as
//! [`MixFader`]). Settings, defaults, ranges and units mirror
//! `apps/settings_list.c`.

use std::time::Duration;

const MIXFADE_UNITY_BITS: i32 = 16;
const MIXFADE_UNITY: i32 = 1 << MIXFADE_UNITY_BITS; // 65536

/// When to crossfade between tracks — mirrors Rockbox's `crossfade`
/// setting (`apps/settings.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CrossfadeMode {
    /// No crossfade; tracks play gapless.
    #[default]
    Off,
    /// Only when a track ends and the next begins automatically.
    AutoSkip,
    /// Only on a manual skip (next / previous / skip-to).
    ManualSkip,
    /// Only while shuffle is on (treated as "always" here — this crate
    /// has no shuffle setting of its own).
    Shuffle,
    /// On shuffle or manual skip.
    ShuffleOrManualSkip,
    /// Always crossfade.
    Always,
}

impl CrossfadeMode {
    /// Does a transition crossfade under this mode? `auto` is true when
    /// the previous track ended on its own (vs a user skip).
    pub(crate) fn applies(self, auto: bool) -> bool {
        match self {
            CrossfadeMode::Off => false,
            CrossfadeMode::AutoSkip => auto,
            CrossfadeMode::ManualSkip => !auto,
            // No shuffle state here; honour the user's intent to fade.
            CrossfadeMode::Shuffle => true,
            CrossfadeMode::ShuffleOrManualSkip => true,
            CrossfadeMode::Always => true,
        }
    }
}

/// How the outgoing track behaves during the overlap
/// (`crossfade_fade_out_mixmode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MixMode {
    /// Both tracks fade — outgoing ramps to silence as incoming ramps up.
    #[default]
    Crossfade,
    /// Outgoing stays at full volume; only the incoming track fades in
    /// and is mixed on top.
    Mix,
}

/// Crossfade configuration. Field names, defaults, ranges and units
/// mirror Rockbox's settings (`apps/settings_list.c`).
#[derive(Debug, Clone, Copy)]
pub struct CrossfadeSettings {
    pub mode: CrossfadeMode,
    /// Full-volume outgoing audio kept before the fade-out ramp (0–7 s).
    pub fade_out_delay: Duration,
    /// Length of the outgoing fade-out ramp (0–15 s).
    pub fade_out_duration: Duration,
    /// Silence at the start of the incoming track before it fades in
    /// (0–7 s).
    pub fade_in_delay: Duration,
    /// Length of the incoming fade-in ramp (0–15 s).
    pub fade_in_duration: Duration,
    pub mix_mode: MixMode,
}

impl Default for CrossfadeSettings {
    fn default() -> Self {
        // Rockbox defaults: 2 s fade in/out, no delays, crossfade mode.
        CrossfadeSettings {
            mode: CrossfadeMode::Off,
            fade_out_delay: Duration::ZERO,
            fade_out_duration: Duration::from_secs(2),
            fade_in_delay: Duration::ZERO,
            fade_in_duration: Duration::from_secs(2),
            mix_mode: MixMode::Crossfade,
        }
    }
}

impl CrossfadeSettings {
    /// A ready-made 2 s crossfade on every track change.
    pub fn always() -> Self {
        CrossfadeSettings {
            mode: CrossfadeMode::Always,
            ..Default::default()
        }
    }

    fn frames(dur: Duration, rate: u32) -> usize {
        (dur.as_secs_f64() * rate as f64).round() as usize
    }

    /// Total overlap length in frames at `rate`, the number of frames the
    /// outgoing tail and incoming head share.
    pub(crate) fn region_frames(&self, rate: u32) -> usize {
        let out =
            Self::frames(self.fade_out_delay, rate) + Self::frames(self.fade_out_duration, rate);
        let in_ =
            Self::frames(self.fade_in_delay, rate) + Self::frames(self.fade_in_duration, rate);
        out.max(in_)
    }
}

/// Linear fade gain stepper — the Bresenham ramp from `apps/pcmbuf.c`
/// (`mixfader_init` / `mixfader_step`). `factor` walks from `start` to
/// `end` (Q16) over `nframes`, one [`MixFader::step`] per stereo frame.
struct MixFader {
    factor: i32,
    endfac: i32,
    nsamp2: i32,
    ferr: i32,
    dfquo: i32,
    dfrem: i32,
    dfinc: i32,
}

impl MixFader {
    fn new(start: i32, end: i32, nframes: usize) -> Self {
        let nsamp2 = nframes as i32 * 2;
        if nsamp2 == 0 {
            return MixFader {
                factor: end,
                endfac: end,
                nsamp2: 0,
                ferr: 0,
                dfquo: 0,
                dfrem: 0,
                dfinc: 0,
            };
        }
        let dfact2 = 2 * (end - start).abs();
        let dfinc = if end < start { -1 } else { 1 };
        MixFader {
            factor: start,
            endfac: end,
            nsamp2,
            ferr: dfact2 / 2,
            dfquo: (dfact2 / nsamp2) * dfinc,
            dfrem: dfact2 - (dfact2 / nsamp2) * nsamp2,
            dfinc,
        }
    }

    fn step(&mut self) {
        if self.factor == self.endfac {
            return;
        }
        self.factor += self.dfquo;
        self.ferr += self.dfrem;
        if self.ferr >= self.nsamp2 {
            self.factor += self.dfinc;
            self.ferr -= self.nsamp2;
        }
    }
}

/// `mixfade_sample()` — apply a Q16 gain factor with rounding.
#[inline]
fn mixfade_sample(factor: i32, s: i32) -> i32 {
    (factor * s + MIXFADE_UNITY / 2) >> MIXFADE_UNITY_BITS
}

/// `clip_sample_16()` — saturate to signed 16-bit.
#[inline]
pub(crate) fn clip16(s: i32) -> i16 {
    if s > 32767 {
        32767
    } else if s < -32768 {
        -32768
    } else {
        s as i16
    }
}

/// Per-frame outgoing gain over the overlap: hold at unity for the
/// fade-out delay, ramp unity→0 over the fade-out duration, then silence.
/// In [`MixMode::Mix`] the outgoing track stays at unity throughout.
fn out_gains(s: &CrossfadeSettings, region: usize, rate: u32) -> Vec<i32> {
    if s.mix_mode == MixMode::Mix {
        return vec![MIXFADE_UNITY; region];
    }
    let delay = CrossfadeSettings::frames(s.fade_out_delay, rate);
    let dur = CrossfadeSettings::frames(s.fade_out_duration, rate);
    let mut fader = MixFader::new(MIXFADE_UNITY, 0, dur);
    (0..region)
        .map(|i| {
            if i < delay {
                MIXFADE_UNITY
            } else if i < delay + dur {
                let g = fader.factor;
                fader.step();
                g
            } else {
                0
            }
        })
        .collect()
}

/// Per-frame incoming gain: silence for the fade-in delay, ramp 0→unity
/// over the fade-in duration, then full volume.
fn in_gains(s: &CrossfadeSettings, region: usize, rate: u32) -> Vec<i32> {
    let delay = CrossfadeSettings::frames(s.fade_in_delay, rate);
    let dur = CrossfadeSettings::frames(s.fade_in_duration, rate);
    let mut fader = MixFader::new(0, MIXFADE_UNITY, dur);
    (0..region)
        .map(|i| {
            if i < delay {
                0
            } else if i < delay + dur {
                let g = fader.factor;
                fader.step();
                g
            } else {
                MIXFADE_UNITY
            }
        })
        .collect()
}

/// Mix an outgoing `tail` and incoming `head` (both interleaved stereo
/// i16 at `rate`) into the crossfaded overlap. Either slice may be
/// shorter than the overlap — missing frames count as silence (the
/// outgoing track having ended, or the incoming being very short).
pub(crate) fn mix(tail: &[i16], head: &[i16], settings: &CrossfadeSettings, rate: u32) -> Vec<i16> {
    let region = settings
        .region_frames(rate)
        .max(tail.len() / 2)
        .max(head.len() / 2);
    let og = out_gains(settings, region, rate);
    let ig = in_gains(settings, region, rate);

    let frame = |buf: &[i16], i: usize| -> (i32, i32) {
        let base = i * 2;
        if base + 1 < buf.len() {
            (buf[base] as i32, buf[base + 1] as i32)
        } else {
            (0, 0)
        }
    };

    let mut out = Vec::with_capacity(region * 2);
    for i in 0..region {
        let (tl, tr) = frame(tail, i);
        let (hl, hr) = frame(head, i);
        let l = mixfade_sample(og[i], tl) + mixfade_sample(ig[i], hl);
        let r = mixfade_sample(og[i], tr) + mixfade_sample(ig[i], hr);
        out.push(clip16(l));
        out.push(clip16(r));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_gain_is_transparent() {
        assert_eq!(mixfade_sample(MIXFADE_UNITY, 12345), 12345);
        assert_eq!(mixfade_sample(0, 12345), 0);
        assert_eq!(mixfade_sample(MIXFADE_UNITY / 2, 10000), 5000);
    }

    #[test]
    fn clip_saturates() {
        assert_eq!(clip16(40000), 32767);
        assert_eq!(clip16(-40000), -32768);
        assert_eq!(clip16(100), 100);
    }

    #[test]
    fn fade_out_ramps_unity_to_zero() {
        let g = out_gains(
            &CrossfadeSettings {
                fade_out_duration: Duration::from_secs(1),
                ..CrossfadeSettings::always()
            },
            44100,
            44100,
        );
        assert_eq!(g.len(), 44100);
        assert!(g[0] >= MIXFADE_UNITY - 4, "starts near unity: {}", g[0]);
        assert!(
            *g.last().unwrap() <= 8,
            "ends near zero: {}",
            g.last().unwrap()
        );
        // monotonically non-increasing
        assert!(g.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn constant_power_ish_midpoint_is_attenuated() {
        // At the crossfade midpoint both tracks are ~half gain, so equal
        // full-scale signals sum below full scale (no gross clipping for
        // uncorrelated material).
        let s = CrossfadeSettings {
            fade_in_duration: Duration::from_secs(1),
            fade_out_duration: Duration::from_secs(1),
            ..CrossfadeSettings::always()
        };
        let n = 44100;
        let tail: Vec<i16> = std::iter::repeat(16000).take(n * 2).collect();
        let head: Vec<i16> = std::iter::repeat(-16000).take(n * 2).collect();
        let out = mix(&tail, &head, &s, 44100);
        let mid = out[n]; // middle frame, left channel
                          // 16000*0.5 + (-16000)*0.5 ≈ 0 for these opposed signals
        assert!(mid.abs() < 200, "midpoint {mid}");
        // first frame ≈ all outgoing, last ≈ all incoming
        assert!((out[0] - 16000).abs() < 200);
        assert!((out[out.len() - 2] + 16000).abs() < 200);
    }
}
