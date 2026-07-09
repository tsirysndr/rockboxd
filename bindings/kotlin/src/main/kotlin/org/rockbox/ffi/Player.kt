package org.rockbox.ffi

import java.lang.foreign.Arena
import java.lang.foreign.MemorySegment
import org.json.JSONArray
import org.json.JSONObject

/**
 * Queue-based player with native ReplayGain and Rockbox crossfade.
 *
 * A Player owns a live audio output device and a background engine thread —
 * construct it only where an output device exists. The handle is freed on
 * [close]; the class is [AutoCloseable].
 *
 * ReplayGain `mode` here uses the *player* values ([ReplayGainMode]: OFF=0,
 * TRACK=1, ALBUM=2) — distinct from the DSP encoding.
 */
class Player private constructor(private var ptr: MemorySegment?) : AutoCloseable {

    /** Overridable construction parameters (see `rb_player_new_with_config`). */
    data class Config(
        var sampleRate: Long = 0,          // 0 => device default
        var bufferSeconds: Float = 4.0f,
        var volume: Float = 1.0f,
        var replaygainMode: ReplayGainMode = ReplayGainMode.OFF,
        var replaygainPreampDb: Float = 0.0f,
        var replaygainPreventClipping: Boolean = true,
        var crossfadeMode: CrossfadeMode = CrossfadeMode.OFF,
        var fadeOutDelayMs: Long = 0,
        var fadeOutDurationMs: Long = 2000,
        var fadeInDelayMs: Long = 0,
        var fadeInDurationMs: Long = 2000,
        var mixMode: MixMode = MixMode.CROSSFADE,
    )

    companion object {
        /** Create a player with configuration overrides. */
        operator fun invoke(config: Config = Config()): Player {
            val p = Native.playerNewWithConfig.invokeWithArguments(
                config.sampleRate.toInt(), config.bufferSeconds, config.volume,
                config.replaygainMode.value, config.replaygainPreampDb,
                config.replaygainPreventClipping, config.crossfadeMode.value,
                config.fadeOutDelayMs.toInt(), config.fadeOutDurationMs.toInt(),
                config.fadeInDelayMs.toInt(), config.fadeInDurationMs.toInt(), config.mixMode.value,
            ) as MemorySegment
            if (p.address() == 0L) {
                throw RockboxException("rb_player_new_with_config returned NULL (no output device?)")
            }
            return Player(p)
        }

        /** Player on the default device with Rockbox default settings. */
        fun makeDefault(): Player {
            val p = Native.playerNew.invokeWithArguments() as MemorySegment
            if (p.address() == 0L) {
                throw RockboxException("rb_player_new returned NULL (no output device?)")
            }
            return Player(p)
        }
    }

    private fun handle(): MemorySegment = ptr ?: throw RockboxException("Player has been closed")

    /** Free the native handle. Safe to call more than once. */
    override fun close() {
        ptr?.let { Native.playerFree.invokeWithArguments(it); ptr = null }
    }

    val isClosed: Boolean get() = ptr == null

    // ---- queue --------------------------------------------------------

    fun setQueue(paths: List<String>) {
        val json = JSONArray(paths).toString()
        Arena.ofConfined().use { arena ->
            Native.playerSetQueueJson.invokeWithArguments(handle(), arena.allocateFrom(json))
        }
    }

    fun enqueue(path: String) {
        Arena.ofConfined().use { arena ->
            Native.playerEnqueue.invokeWithArguments(handle(), arena.allocateFrom(path))
        }
    }

    // ---- transport ----------------------------------------------------

    fun play() { Native.playerPlay.invokeWithArguments(handle()) }
    fun pause() { Native.playerPause.invokeWithArguments(handle()) }
    fun toggle() { Native.playerToggle.invokeWithArguments(handle()) }
    fun stop() { Native.playerStop.invokeWithArguments(handle()) }
    fun next() { Native.playerNext.invokeWithArguments(handle()) }
    fun previous() { Native.playerPrevious.invokeWithArguments(handle()) }
    fun skipTo(index: Long) { Native.playerSkipTo.invokeWithArguments(handle(), index) }
    fun seekMs(ms: Long) { Native.playerSeekMs.invokeWithArguments(handle(), ms) }

    // ---- settings -----------------------------------------------------

    fun setVolume(vol: Float) { Native.playerSetVolume.invokeWithArguments(handle(), vol) }
    val volume: Float get() = Native.playerVolume.invokeWithArguments(handle()) as Float
    val sampleRate: Long
        get() = (Native.playerSampleRate.invokeWithArguments(handle()) as Int).toLong() and 0xFFFFFFFFL

    fun setCrossfade(
        mode: CrossfadeMode,
        fadeOutDelayMs: Long = 0,
        fadeOutDurationMs: Long = 2000,
        fadeInDelayMs: Long = 0,
        fadeInDurationMs: Long = 2000,
        mixMode: MixMode = MixMode.CROSSFADE,
    ) = Native.playerSetCrossfade.invokeWithArguments(
        handle(), mode.value, fadeOutDelayMs.toInt(), fadeOutDurationMs.toInt(),
        fadeInDelayMs.toInt(), fadeInDurationMs.toInt(), mixMode.value,
    ).let { }

    /** [mode]: [ReplayGainMode] (OFF=0, TRACK=1, ALBUM=2). */
    fun setReplaygain(mode: ReplayGainMode, preampDb: Float, preventClipping: Boolean) =
        Native.playerSetReplaygain.invokeWithArguments(handle(), mode.value, preampDb, preventClipping).let { }

    // ---- status -------------------------------------------------------

    /** A snapshot of the player's status as a map. */
    fun status(): Map<String, Any?> {
        val json = Native.takeString(Native.playerStatusJson.invokeWithArguments(handle()) as MemorySegment)
            ?: throw RockboxException("rb_player_status_json returned NULL")
        return JSONObject(json).toMap()
    }
}
