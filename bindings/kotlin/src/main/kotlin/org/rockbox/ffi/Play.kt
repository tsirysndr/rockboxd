package org.rockbox.ffi

import kotlin.system.exitProcess

// Play an audio source through the real output device.
//
// The queue entry can be a local file, a remote http(s) file, an
// internet-radio stream, or an HLS (.m3u8) / MPEG-DASH (.mpd) manifest —
// the engine detects each kind automatically.
//
// Run: mise exec -- gradle play --args="/path/to/audio-or-URL"
//      mise exec -- gradle play --args="hls"    // public HLS test stream
//      mise exec -- gradle play --args="dash"   // public MPEG-DASH test stream

// Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
// for more).
private const val HLS_DEFAULT = "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8"
private const val DASH_DEFAULT = "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd"

fun main(args: Array<String>) {
    val arg = if (args.isNotEmpty()) args[0] else Fixtures.sample
    val file = when (arg) {
        "hls" -> HLS_DEFAULT
        "dash" -> DASH_DEFAULT
        else -> arg
    }

    try {
        val player = Player(Player.Config().apply { volume = 0.8f })
        player.setQueue(listOf(file))
        // DSP: Bass Boost preset + a +7 dB bass / +4 dB treble lift.
        player.setEqPreset(EqPreset.BASS_BOOST)
        player.setBass(7)
        player.setTreble(4)
        player.play()
        println("▶ playing $file")
        println("eq: BassBoost preset, bass +7 dB, treble +4 dB")

        // A live stream reports duration 0 and plays until Ctrl-C.
        while (true) {
            val st = player.status()
            val pos = ((st["position_ms"] as? Number)?.toLong() ?: 0L) / 1000.0
            val dur = ((st["duration_ms"] as? Number)?.toLong() ?: 0L) / 1000.0
            val state = st["state"] as? String ?: "?"
            val clock = if (dur == 0.0) "%.1fs / LIVE".format(pos) else "%.1fs / %.1fs".format(pos, dur)
            print("\r[%s] %s   ".format(state, clock))
            System.out.flush()

            if (state == "stopped" && ((st["position_ms"] as? Number)?.toLong() ?: 0L) > 0) {
                println("\n✔ done")
                break
            }
            Thread.sleep(500)
        }
        exitProcess(0)
    } catch (e: Exception) {
        System.err.println("error: $e")
        exitProcess(1)
    }
}
