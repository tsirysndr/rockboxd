package org.rockbox.ffi

import java.lang.foreign.Arena
import java.lang.foreign.MemorySegment
import java.lang.foreign.ValueLayout
import org.json.JSONObject

/**
 * Decode a local audio file to interleaved-stereo S16 PCM, one chunk at a time.
 *
 * Wraps the Rockbox codec engine. The codec state is process-wide, so only one
 * [Decoder] may decode at a time — constructing a second one blocks until the
 * first is closed. The handle is freed on [close]; the class is [AutoCloseable]
 * for use with `use { }`.
 */
class Decoder(path: String) : AutoCloseable {
    private var ptr: MemorySegment? =
        Arena.ofConfined().use { arena ->
            (Native.decoderOpen.invokeWithArguments(arena.allocateFrom(path)) as MemorySegment).let {
                if (it.address() == 0L) throw RockboxException("could not open decoder for $path") else it
            }
        }

    private fun handle(): MemorySegment =
        ptr ?: throw RockboxException("Decoder has been closed")

    /** Free the native handle. Safe to call more than once. */
    override fun close() {
        ptr?.let { Native.decoderFree.invokeWithArguments(it); ptr = null }
    }

    val isClosed: Boolean get() = ptr == null

    // ---- metadata -----------------------------------------------------

    /** Tags and stream properties (same shape as [Metadata.read]). */
    fun metadata(): Map<String, Any?> {
        val raw = Native.decoderMetadataJson.invokeWithArguments(handle()) as MemorySegment
        val json = Native.takeString(raw)
            ?: throw RockboxException("rb_decoder_metadata_json returned NULL")
        return JSONObject(json).toMap()
    }

    // ---- decoding -----------------------------------------------------

    /**
     * Next buffer of decoded PCM as `(samples, sampleRate)`, where `samples` is
     * an interleaved stereo int16 array. Returns null at end of track (or after
     * a codec error — see [finished]).
     */
    fun nextChunk(): Pair<ShortArray, Int>? {
        Arena.ofConfined().use { arena ->
            val outLen = arena.allocate(ValueLayout.JAVA_LONG)
            val outRate = arena.allocate(ValueLayout.JAVA_INT)
            val outPtr = Native.decoderNextChunk.invokeWithArguments(handle(), outLen, outRate) as MemorySegment
            val n = outLen.get(ValueLayout.JAVA_LONG, 0)
            if (outPtr.address() == 0L || n <= 0) return null

            val out = outPtr.reinterpret(n * 2)
            val result = ShortArray(n.toInt())
            MemorySegment.copy(out, ValueLayout.JAVA_SHORT, 0, result, 0, n.toInt())
            Native.bufferFree.invokeWithArguments(outPtr, n)
            return result to outRate.get(ValueLayout.JAVA_INT, 0)
        }
    }

    /** Request a seek to [ms] milliseconds. */
    fun seekMs(ms: Long) { Native.decoderSeekMs.invokeWithArguments(handle(), ms) }

    /** Playback position last reported by the codec, in milliseconds. */
    fun elapsedMs(): Long = Native.decoderElapsedMs.invokeWithArguments(handle()) as Long

    /**
     * `(done, code)`: `done` is true once the track has ended; `code` is the
     * exit status (0 = clean end, negative = codec error), valid only when
     * `done`.
     */
    fun finished(): Pair<Boolean, Int> {
        Arena.ofConfined().use { arena ->
            val outCode = arena.allocate(ValueLayout.JAVA_INT)
            val done = Native.decoderFinished.invokeWithArguments(handle(), outCode) as Boolean
            return done to outCode.get(ValueLayout.JAVA_INT, 0)
        }
    }
}
