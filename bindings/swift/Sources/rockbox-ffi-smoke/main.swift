// End-to-end smoke test for the Swift bindings.
//
// Run: swift run rockbox-ffi-smoke

import Foundation
import RockboxFFI

func fail(_ msg: String) -> Never {
    FileHandle.standardError.write(Data("SMOKE FAILED: \(msg)\n".utf8))
    exit(1)
}

// bindings/swift/Sources/rockbox-ffi-smoke/main.swift -> repo root is 5 up.
let repo = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent()
let fixture = repo
    .appendingPathComponent("crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a")
    .path

print("ABI version: \(abiVersion())")

// -- metadata -----------------------------------------------------------
do {
    let meta = try Metadata.read(fixture)
    let codec = meta["codec"] as? String ?? ""
    print("codec=\(codec) title=\(meta["title"] ?? "nil") "
        + "artist=\(meta["artist"] ?? "nil") duration_ms=\(meta["duration_ms"] ?? "nil") "
        + "sample_rate=\(meta["sample_rate"] ?? "nil")")
    if codec.isEmpty { fail("codec label should not be empty") }

    let probe = Metadata.probe("song.flac")
    print("probe('song.flac') = \(probe ?? "nil")")
    if probe != "FLAC" { fail("expected FLAC, got \(probe ?? "nil")") }
} catch {
    fail("metadata: \(error)")
}

// -- DSP: -6.0206 dB track gain should HALVE amplitude ------------------
do {
    let rate: UInt32 = 44_100
    let dsp = try Dsp(sampleRate: rate)
    defer { dsp.close() }
    dsp.setReplaygain(.track, noclip: false, preampDb: 0.0)
    dsp.setReplaygainGains(trackGainDb: -6.0206) // x0.5

    let sine = sineStereo(freqHz: 1000, seconds: 1.0, rate: rate, amplitude: 16_000)
    let out = try dsp.process(sine)
    let peak = out.map { Int(abs(Int32($0))) }.max() ?? 0
    print("DSP peak after -6.02 dB track gain: \(peak) (expected ~8000)")
    if !(7_600...8_400).contains(peak) { fail("peak \(peak) out of range") }
} catch {
    fail("dsp: \(error)")
}

// -- Player (construct only, no audible playback) -----------------------
do {
    var cfg = Player.Config()
    cfg.volume = 0.0
    let player = try Player(config: cfg)
    defer { player.close() }

    let sr = player.sampleRate
    print("Player sample_rate=\(sr)")
    if sr == 0 { fail("sample_rate should be > 0") }

    player.setVolume(0.0)
    try player.setQueue([fixture])
    Thread.sleep(forTimeInterval: 0.1) // queue command applied asynchronously

    let st = try player.status()
    let state = st["state"] as? String ?? ""
    let queueLen = st["queue_len"] as? Int ?? -1
    print("Player status: state=\(state) queue_len=\(queueLen)")
    if state != "stopped" { fail("expected stopped, got \(state)") }
    if queueLen != 1 { fail("expected queue_len 1, got \(queueLen)") }
} catch {
    fail("player: \(error)")
}

print("\nALL CHECKS PASSED ✔")
