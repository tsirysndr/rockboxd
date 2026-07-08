//! Engine integration tests. These need an output audio device; on a
//! headless machine (`Player::new()` → NoOutputDevice) they skip.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use rockbox_playback::{CrossfadeMode, CrossfadeSettings, PlaybackState, Player, ReplayGainMode};

const RATE: u32 = 44100;

/// The player uses process-global singletons (the codec decode gate and
/// the DSP instance) and one output stream, so only one may exist at a
/// time. Serialize the tests so `cargo test` (parallel by default) works.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

struct TempFile(PathBuf);
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Write a `secs`-long sine WAV; returns its path.
fn wav(name: &str, secs: f32, freq: f32) -> TempFile {
    let frames = (RATE as f32 * secs) as usize;
    let mut data = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        let t = i as f32 / RATE as f32;
        let s = (10000.0 * (2.0 * std::f32::consts::PI * freq * t).sin()) as i16;
        data.extend_from_slice(&s.to_le_bytes());
        data.extend_from_slice(&s.to_le_bytes());
    }
    let n = data.len() as u32;
    let mut v = Vec::new();
    v.extend(b"RIFF");
    v.extend(&(36 + n).to_le_bytes());
    v.extend(b"WAVEfmt ");
    v.extend(&16u32.to_le_bytes());
    v.extend(&1u16.to_le_bytes());
    v.extend(&2u16.to_le_bytes());
    v.extend(&RATE.to_le_bytes());
    v.extend(&(RATE * 4).to_le_bytes());
    v.extend(&4u16.to_le_bytes());
    v.extend(&16u16.to_le_bytes());
    v.extend(b"data");
    v.extend(&n.to_le_bytes());
    v.extend(&data);

    let path = std::env::temp_dir().join(format!("rbplay_{}_{}", std::process::id(), name));
    std::fs::File::create(&path).unwrap().write_all(&v).unwrap();
    TempFile(path)
}

/// Build a player at low volume, skipping the test if there's no device.
fn player_or_skip() -> Option<Player> {
    match Player::new() {
        Ok(p) => {
            p.set_volume(0.05); // audible tests are rude; keep it quiet
            Some(p)
        }
        Err(_) => {
            eprintln!("no output device — skipping");
            None
        }
    }
}

fn wait_until<F: Fn(&Player) -> bool>(p: &Player, timeout: Duration, cond: F) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if cond(p) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    false
}

#[test]
fn plays_and_advances_through_queue() {
    let _serial = serial();
    let Some(player) = player_or_skip() else {
        return;
    };
    let a = wav("a.wav", 0.6, 330.0);
    let b = wav("b.wav", 0.6, 440.0);

    player.set_queue(vec![a.0.clone(), b.0.clone()]);
    player.play();

    assert!(
        wait_until(&player, Duration::from_secs(3), |p| {
            p.status().state == PlaybackState::Playing
        }),
        "should reach Playing"
    );

    // Should advance to the second track on auto-skip.
    assert!(
        wait_until(&player, Duration::from_secs(4), |p| {
            p.status().index == Some(1)
        }),
        "should advance to track 2"
    );

    // And eventually stop after the last track drains.
    assert!(
        wait_until(&player, Duration::from_secs(5), |p| {
            p.status().state == PlaybackState::Stopped
        }),
        "should stop at end of queue"
    );
}

#[test]
fn pause_and_resume() {
    let _serial = serial();
    let Some(player) = player_or_skip() else {
        return;
    };
    let a = wav("pause.wav", 3.0, 330.0);
    player.set_queue(vec![a.0.clone()]);
    player.play();

    assert!(wait_until(&player, Duration::from_secs(3), |p| {
        p.status().state == PlaybackState::Playing
    }));

    player.pause();
    assert!(wait_until(&player, Duration::from_secs(1), |p| {
        p.status().state == PlaybackState::Paused
    }));
    // Let the ~1/3 s pause fade-out settle, then position must be frozen.
    std::thread::sleep(Duration::from_millis(500));
    let pos1 = player.status().position;
    std::thread::sleep(Duration::from_millis(300));
    let pos2 = player.status().position;
    assert_eq!(pos1, pos2, "position must not advance once paused");

    player.toggle(); // resume
    assert!(wait_until(&player, Duration::from_secs(1), |p| {
        p.status().state == PlaybackState::Playing
    }));
}

#[test]
fn manual_next_skips_immediately() {
    let _serial = serial();
    let Some(player) = player_or_skip() else {
        return;
    };
    let a = wav("mn_a.wav", 5.0, 330.0);
    let b = wav("mn_b.wav", 1.0, 440.0);
    player.set_queue(vec![a.0.clone(), b.0.clone()]);
    player.play();

    assert!(wait_until(&player, Duration::from_secs(3), |p| {
        p.status().index == Some(0) && p.status().state == PlaybackState::Playing
    }));

    player.next();
    assert!(
        wait_until(&player, Duration::from_secs(2), |p| {
            p.status().index == Some(1)
        }),
        "manual next should reach track 2 quickly"
    );
}

#[test]
fn crossfade_transition_runs() {
    let _serial = serial();
    let Some(player) = player_or_skip() else {
        return;
    };
    // Tracks longer than the crossfade so the overlap actually engages.
    let a = wav("cf_a.wav", 1.5, 330.0);
    let b = wav("cf_b.wav", 1.5, 440.0);
    player.set_crossfade(CrossfadeSettings {
        mode: CrossfadeMode::Always,
        fade_in_duration: Duration::from_millis(500),
        fade_out_duration: Duration::from_millis(500),
        ..Default::default()
    });
    player.set_replaygain(ReplayGainMode::Track, 0.0, true);
    player.set_queue(vec![a.0.clone(), b.0.clone()]);
    player.play();

    // The crossfade path must carry us into track 2 and then stop
    // cleanly (no hang / panic).
    assert!(
        wait_until(&player, Duration::from_secs(5), |p| {
            p.status().index == Some(1)
        }),
        "crossfade should transition to track 2"
    );
    assert!(
        wait_until(&player, Duration::from_secs(5), |p| {
            p.status().state == PlaybackState::Stopped
        }),
        "should stop after crossfaded queue ends"
    );
}
