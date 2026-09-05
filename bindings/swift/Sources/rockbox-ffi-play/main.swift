// Play an audio source through the real output device.
//
// The queue entry can be a local file, a remote http(s) file, an
// internet-radio stream, or an HLS (.m3u8) / MPEG-DASH (.mpd) manifest —
// the engine detects each kind automatically.
//
// Run: swift run rockbox-ffi-play [path-or-URL]
//      swift run rockbox-ffi-play hls    // public HLS test stream
//      swift run rockbox-ffi-play dash   // public MPEG-DASH test stream

import Foundation
import RockboxFFIDynamic

let repo = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent()
let fixture = repo
    .appendingPathComponent("crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a")
    .path

// Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
// for more).
let hlsDefault = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"
let dashDefault = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd"

let arg = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : fixture
let file: String
switch arg {
case "hls": file = hlsDefault
case "dash": file = dashDefault
default: file = arg
}

do {
    var cfg = Player.Config()
    cfg.volume = 0.8
    let player = try Player(config: cfg)
    // DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift, then play — fluent chain.
    try player.setQueue([file])
        .setEqPreset(.bassBoost)
        .setBass(7)
        .setTreble(4)
        .play()
    print("▶ playing \(file)")
    print("eq: BassBoost preset, bass +7 dB, treble +4 dB")

    // The native audio engine installs its own SIGINT handler while starting
    // the output device; reinstall ours so Ctrl-C exits promptly. We _exit
    // straight away rather than calling stop/close (blocking native calls
    // that can deadlock against the engine thread).
    signal(SIGINT) { _ in
        print("\nstopped")
        _exit(130)
    }

    // A live stream reports duration 0 and plays until Ctrl-C.
    while true {
        let st = try player.status()
        let pos = Double(st["position_ms"] as? Int ?? 0) / 1000.0
        let dur = Double(st["duration_ms"] as? Int ?? 0) / 1000.0
        let state = st["state"] as? String ?? "?"
        let clock = dur == 0
            ? String(format: "%.1fs / LIVE", pos)
            : String(format: "%.1fs / %.1fs", pos, dur)
        print(String(format: "\r[%@] %@   ", state, clock), terminator: "")
        fflush(stdout)

        if state == "stopped" && (st["position_ms"] as? Int ?? 0) > 0 {
            print("\n✔ done")
            break
        }
        Thread.sleep(forTimeInterval: 0.5)
    }
    _exit(0)
} catch {
    FileHandle.standardError.write(Data("error: \(error)\n".utf8))
    exit(1)
}
