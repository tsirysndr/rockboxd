// Play an audio file through the real output device.
//
// Run: swift run rockbox-ffi-play [path-to-audio]

import Foundation
import RockboxFFIDynamic

let repo = URL(fileURLWithPath: #filePath)
    .deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent().deletingLastPathComponent()
    .deletingLastPathComponent()
let fixture = repo
    .appendingPathComponent("crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a")
    .path

let file = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : fixture

do {
    var cfg = Player.Config()
    cfg.volume = 0.8
    let player = try Player(config: cfg)
    try player.setQueue([file])
    // DSP: Bass Boost preset + a +3 dB bass/treble lift.
    player.setEqPreset(.bassBoost)
    player.setBass(3)
    player.setTreble(3)
    player.play()
    print("▶ playing \(file)")
    print("eq: BassBoost preset, bass +3 dB, treble +3 dB")

    // The native audio engine installs its own SIGINT handler while starting
    // the output device; reinstall ours so Ctrl-C exits promptly. We _exit
    // straight away rather than calling stop/close (blocking native calls
    // that can deadlock against the engine thread).
    signal(SIGINT) { _ in
        print("\nstopped")
        _exit(130)
    }

    while true {
        let st = try player.status()
        let pos = Double(st["position_ms"] as? Int ?? 0) / 1000.0
        let dur = Double(st["duration_ms"] as? Int ?? 0) / 1000.0
        let state = st["state"] as? String ?? "?"
        print(String(format: "\r[%@] %.1fs / %.1fs   ", state, pos, dur), terminator: "")
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
