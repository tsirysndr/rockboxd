package org.rockbox.ffi

import kotlin.math.abs
import kotlin.system.exitProcess

// End-to-end smoke test for the Kotlin bindings.
//
// Run: mise exec -- gradle smoke   (or ./gradlew smoke)

private fun fail(msg: String): Nothing {
    System.err.println("SMOKE FAILED: $msg")
    exitProcess(1)
}

fun main() {
    val fixture = Fixtures.sample
    println("ABI version: ${abiVersion()}")

    // -- metadata -------------------------------------------------------
    try {
        val meta = Metadata.read(fixture)
        val codec = meta["codec"] as? String ?: ""
        println(
            "codec=$codec title=${meta["title"]} artist=${meta["artist"]} " +
                "duration_ms=${meta["duration_ms"]} sample_rate=${meta["sample_rate"]}",
        )
        if (codec.isEmpty()) fail("codec label should not be empty")

        val probe = Metadata.probe("song.flac")
        println("probe('song.flac') = $probe")
        if (probe != "FLAC") fail("expected FLAC, got $probe")
    } catch (e: Exception) {
        fail("metadata: $e")
    }

    // -- DSP: -6.0206 dB track gain should HALVE amplitude --------------
    try {
        val rate = 44_100L
        Dsp(rate).use { dsp ->
            dsp.setReplaygain(DspReplayGainMode.TRACK, noclip = false, preampDb = 0.0f)
            dsp.setReplaygainGains(trackGainDb = -6.0206f) // x0.5

            val sine = sineStereo(freqHz = 1000.0, seconds = 1.0, rate = rate, amplitude = 16_000.0)
            val out = dsp.process(sine)
            val peak = out.maxOfOrNull { abs(it.toInt()) } ?: 0
            println("DSP peak after -6.02 dB track gain: $peak (expected ~8000)")
            if (peak !in 7_600..8_400) fail("peak $peak out of range")
        }
    } catch (e: Exception) {
        fail("dsp: $e")
    }

    // -- Player (construct only, no audible playback) -------------------
    try {
        val config = Player.Config().apply { volume = 0.0f }
        Player(config).use { player ->
            val sr = player.sampleRate
            println("Player sample_rate=$sr")
            if (sr == 0L) fail("sample_rate should be > 0")

            player.setVolume(0.0f)
            player.setQueue(listOf(fixture))
            Thread.sleep(100) // queue command applied asynchronously

            val st = player.status()
            val state = st["state"] as? String ?: ""
            val queueLen = (st["queue_len"] as? Number)?.toInt() ?: -1
            println("Player status: state=$state queue_len=$queueLen")
            if (state != "stopped") fail("expected stopped, got $state")
            if (queueLen != 1) fail("expected queue_len 1, got $queueLen")
        }
    } catch (e: Exception) {
        fail("player: $e")
    }

    println("\nALL CHECKS PASSED ✔")
}
