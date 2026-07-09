package org.rockbox.ffi

import kotlin.system.exitProcess

// Play an audio file through the real output device.
//
// Run: mise exec -- gradle play --args="/path/to/audio"

fun main(args: Array<String>) {
    val file = if (args.isNotEmpty()) args[0] else Fixtures.sample

    try {
        val player = Player(Player.Config().apply { volume = 0.8f })
        player.setQueue(listOf(file))
        player.play()
        println("▶ playing $file")

        while (true) {
            val st = player.status()
            val pos = ((st["position_ms"] as? Number)?.toLong() ?: 0L) / 1000.0
            val dur = ((st["duration_ms"] as? Number)?.toLong() ?: 0L) / 1000.0
            val state = st["state"] as? String ?: "?"
            print("\r[%s] %.1fs / %.1fs   ".format(state, pos, dur))
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
