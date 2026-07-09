package org.rockbox.ffi

import java.lang.foreign.Arena
import java.lang.foreign.MemorySegment
import org.json.JSONObject

/** Audio file metadata parsing. */
object Metadata {
    /**
     * Parse the metadata of the audio file at [path].
     *
     * Returns a map with tag strings, `duration_ms`, `bitrate`, `sample_rate`,
     * a nested `replaygain` map (including the raw Q7.24 `raw_*` fields), and
     * optional `album_art` / `cuesheet` locations. Throws on failure.
     */
    fun read(path: String): Map<String, Any?> {
        val raw = Arena.ofConfined().use { arena ->
            Native.metaReadJson.invokeWithArguments(arena.allocateFrom(path)) as MemorySegment
        }
        val json = Native.takeString(raw)
            ?: throw RockboxException("rb_meta_read_json($path) returned NULL")
        return JSONObject(json).toMap()
    }

    /**
     * Guess the codec label (e.g. "FLAC") from a filename's extension without
     * opening the file. Returns null for an unknown extension.
     */
    fun probe(filename: String): String? {
        val raw = Arena.ofConfined().use { arena ->
            Native.metaProbe.invokeWithArguments(arena.allocateFrom(filename)) as MemorySegment
        }
        return Native.takeString(raw)
    }
}
