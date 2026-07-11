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
        /**
         * Auto-persist the queue + exact position to this `.m3u8` file; null or
         * empty disables resume. Requires [Player.invoke] (not [makeDefault]).
         */
        var resumeFile: String? = null,
        /** Resume save interval in ms; 0 uses the ABI default (5 s). */
        var resumeSaveIntervalMs: Long = 0,
    )

    companion object {
        /** Create a player with configuration overrides. */
        operator fun invoke(config: Config = Config()): Player {
            val p = Arena.ofConfined().use { arena ->
                val resume = config.resumeFile?.let { arena.allocateFrom(it) } ?: MemorySegment.NULL
                Native.playerNewWithConfigEx.invokeWithArguments(
                    config.sampleRate.toInt(), config.bufferSeconds, config.volume,
                    config.replaygainMode.value, config.replaygainPreampDb,
                    config.replaygainPreventClipping, config.crossfadeMode.value,
                    config.fadeOutDelayMs.toInt(), config.fadeOutDurationMs.toInt(),
                    config.fadeInDelayMs.toInt(), config.fadeInDurationMs.toInt(),
                    config.mixMode.value, resume, config.resumeSaveIntervalMs.toInt(),
                ) as MemorySegment
            }
            if (p.address() == 0L) {
                throw RockboxException("rb_player_new_with_config_ex returned NULL (no output device?)")
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

        /** Peek at a resume file without a player, or null if absent/invalid. */
        fun loadResume(path: String): ResumeState? {
            val json = Arena.ofConfined().use { arena ->
                Native.takeString(
                    Native.loadResumeJson.invokeWithArguments(arena.allocateFrom(path)) as MemorySegment,
                )
            } ?: return null
            return ResumeState.fromJson(JSONObject(json))
        }

        /** Parse a playlist file into its entries, or null on error. */
        fun m3uRead(path: String): List<M3uEntry>? {
            val json = Arena.ofConfined().use { arena ->
                Native.takeString(
                    Native.m3uReadJson.invokeWithArguments(arena.allocateFrom(path)) as MemorySegment,
                )
            } ?: return null
            val arr = JSONArray(json)
            return (0 until arr.length()).map { M3uEntry.fromJson(arr.getJSONObject(it)) }
        }

        /** Write a list of paths as an `.m3u8`. Returns true on success. */
        fun m3uWrite(path: String, paths: List<String>): Boolean {
            val json = JSONArray(paths).toString()
            val rc = Arena.ofConfined().use { arena ->
                Native.m3uWriteJson.invokeWithArguments(
                    arena.allocateFrom(path), arena.allocateFrom(json),
                ) as Int
            }
            return rc == 0
        }

        /** Whether a string looks like an `http(s)://` URL. */
        fun isUrl(s: String): Boolean = Arena.ofConfined().use { arena ->
            Native.isUrl.invokeWithArguments(arena.allocateFrom(s)) as Boolean
        }
    }

    /** Restored playback state: the queue [tracks], the current [index], and [elapsedMs]. */
    data class ResumeState(val tracks: List<String>, val index: Int, val elapsedMs: Long) {
        internal companion object {
            fun fromJson(o: JSONObject): ResumeState {
                val arr = o.optJSONArray("tracks")
                val tracks = if (arr == null) emptyList() else
                    (0 until arr.length()).map { arr.getString(it) }
                return ResumeState(tracks, o.optInt("index", 0), o.optLong("elapsed_ms", 0))
            }
        }
    }

    /** One playlist entry parsed from an m3u/m3u8 file. */
    data class M3uEntry(val path: String, val durationMs: Long?, val title: String?) {
        internal companion object {
            fun fromJson(o: JSONObject): M3uEntry = M3uEntry(
                o.getString("path"),
                if (o.isNull("duration_ms")) null else o.optLong("duration_ms"),
                if (o.isNull("title")) null else o.optString("title", null),
            )
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

    /**
     * Insert [paths] (local paths or http(s):// URLs) into the queue at
     * [position]. [index] is only used when [position] is [InsertPosition.INDEX].
     */
    fun insert(paths: List<String>, position: InsertPosition, index: Int = 0) {
        val json = JSONArray(paths).toString()
        Arena.ofConfined().use { arena ->
            Native.playerInsertJson.invokeWithArguments(
                handle(), arena.allocateFrom(json), position.value, index.toLong(),
            )
        }
    }

    /** The current queue as a list of paths/URLs. */
    fun queue(): List<String> {
        val json = Native.takeString(Native.playerQueueJson.invokeWithArguments(handle()) as MemorySegment)
            ?: return emptyList()
        val arr = JSONArray(json)
        return (0 until arr.length()).map { arr.getString(it) }
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

    // ---- resume -------------------------------------------------------

    /**
     * Restore queue + exact position from the configured resume file (does NOT
     * auto-play). Returns the restored state, or null if there is nothing to
     * resume.
     */
    fun resume(): ResumeState? {
        val json = Native.takeString(Native.playerResume.invokeWithArguments(handle()) as MemorySegment)
            ?: return null
        return ResumeState.fromJson(JSONObject(json))
    }

    /** Force-persist the current queue + position to the resume file. */
    fun saveResume() { Native.playerSaveResume.invokeWithArguments(handle()) }

    /** Delete the resume file. */
    fun clearResume() { Native.playerClearResume.invokeWithArguments(handle()) }

    // ---- m3u / m3u8 ---------------------------------------------------

    /**
     * Import a playlist file into the queue at [position] ([index] only used
     * for [InsertPosition.INDEX]). Returns the imported paths, or null on error.
     */
    fun importM3u(path: String, position: InsertPosition, index: Int = 0): List<String>? {
        val json = Arena.ofConfined().use { arena ->
            Native.takeString(
                Native.playerImportM3u.invokeWithArguments(
                    handle(), arena.allocateFrom(path), position.value, index.toLong(),
                ) as MemorySegment,
            )
        } ?: return null
        val arr = JSONArray(json)
        return (0 until arr.length()).map { arr.getString(it) }
    }

    /** Replace the queue with a playlist file. Returns loaded paths, or null on error. */
    fun loadM3u(path: String): List<String>? {
        val json = Arena.ofConfined().use { arena ->
            Native.takeString(
                Native.playerLoadM3u.invokeWithArguments(handle(), arena.allocateFrom(path)) as MemorySegment,
            )
        } ?: return null
        val arr = JSONArray(json)
        return (0 until arr.length()).map { arr.getString(it) }
    }

    /** Export the current queue to an `.m3u8`. Returns true on success. */
    fun exportM3u(path: String): Boolean {
        val rc = Arena.ofConfined().use { arena ->
            Native.playerExportM3u.invokeWithArguments(handle(), arena.allocateFrom(path)) as Int
        }
        return rc == 0
    }
}
