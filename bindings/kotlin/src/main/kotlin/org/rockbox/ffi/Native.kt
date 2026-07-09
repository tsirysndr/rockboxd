package org.rockbox.ffi

import java.io.File
import java.lang.foreign.Arena
import java.lang.foreign.FunctionDescriptor
import java.lang.foreign.Linker
import java.lang.foreign.MemorySegment
import java.lang.foreign.SymbolLookup
import java.lang.foreign.ValueLayout
import java.lang.invoke.MethodHandle

/** Errors thrown by the bindings. */
class RockboxException(message: String) : RuntimeException(message)

/**
 * Runtime loader for the prebuilt librockbox_ffi shared library, built on the
 * Java Foreign Function & Memory API (JEP 454, stable since JDK 22).
 *
 * Nothing is linked at build time: the library is located at runtime and every
 * function is bound to a [MethodHandle] downcall. This mirrors
 * include/rockbox_ffi.h — keep the descriptors in sync with the C ABI.
 */
internal object Native {
    private val linker: Linker = Linker.nativeLinker()

    // The library outlives every call, so it lives in the global arena (never
    // unloaded). The lookup is created once, process-wide.
    private val lookup: SymbolLookup = run {
        val path = locateLibrary()
        SymbolLookup.libraryLookup(path, Arena.global())
    }

    // Layout shorthands (LP64: size_t / uint64_t / int64_t are all 8 bytes).
    private val PTR = ValueLayout.ADDRESS
    private val I32 = ValueLayout.JAVA_INT       // int32_t / uint32_t
    private val I64 = ValueLayout.JAVA_LONG      // int64_t / uint64_t / size_t
    private val F32 = ValueLayout.JAVA_FLOAT
    private val I16 = ValueLayout.JAVA_SHORT
    private val BOOL = ValueLayout.JAVA_BOOLEAN

    private fun h(name: String, desc: FunctionDescriptor): MethodHandle {
        val sym = lookup.find(name).orElseThrow {
            RockboxException("symbol $name not found in librockbox_ffi")
        }
        return linker.downcallHandle(sym, desc)
    }

    // ---- deallocators / version ---------------------------------------
    val abiVersion = h("rb_ffi_abi_version", FunctionDescriptor.of(I32))
    private val stringFree = h("rb_string_free", FunctionDescriptor.ofVoid(PTR))
    val bufferFree = h("rb_buffer_free", FunctionDescriptor.ofVoid(PTR, I64))

    // ---- DSP ----------------------------------------------------------
    val dspNew = h("rb_dsp_new", FunctionDescriptor.of(PTR, I32))
    val dspFree = h("rb_dsp_free", FunctionDescriptor.ofVoid(PTR))
    val dspSetInputFrequency = h("rb_dsp_set_input_frequency", FunctionDescriptor.ofVoid(PTR, I32))
    val dspFlush = h("rb_dsp_flush", FunctionDescriptor.ofVoid(PTR))
    val dspEqEnable = h("rb_dsp_eq_enable", FunctionDescriptor.ofVoid(PTR, BOOL))
    val dspSetTone = h("rb_dsp_set_tone", FunctionDescriptor.ofVoid(PTR, I32, I32))
    val dspSetToneCutoffs = h("rb_dsp_set_tone_cutoffs", FunctionDescriptor.ofVoid(PTR, I32, I32))
    val dspSetSurround = h("rb_dsp_set_surround", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32))
    val dspSetChannelConfig = h("rb_dsp_set_channel_config", FunctionDescriptor.ofVoid(PTR, I32))
    val dspSetStereoWidth = h("rb_dsp_set_stereo_width", FunctionDescriptor.ofVoid(PTR, I32))
    val dspSetCompressor =
        h("rb_dsp_set_compressor", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32, I32, I32))
    val dspSetReplaygain = h("rb_dsp_set_replaygain", FunctionDescriptor.ofVoid(PTR, I32, BOOL, F32))
    val dspSetReplaygainGains =
        h("rb_dsp_set_replaygain_gains", FunctionDescriptor.ofVoid(PTR, F32, F32, F32, F32))
    val dspSetReplaygainGainsRaw =
        h("rb_dsp_set_replaygain_gains_raw", FunctionDescriptor.ofVoid(PTR, I64, I64, I64, I64))
    val dspSetEqBand = h("rb_dsp_set_eq_band", FunctionDescriptor.ofVoid(PTR, I64, I32, F32, F32))
    val dspSetEqPrecut = h("rb_dsp_set_eq_precut", FunctionDescriptor.ofVoid(PTR, F32))
    val dspProcess = h("rb_dsp_process", FunctionDescriptor.of(PTR, PTR, PTR, I64, PTR))

    // ---- metadata -----------------------------------------------------
    val metaReadJson = h("rb_meta_read_json", FunctionDescriptor.of(PTR, PTR))
    val metaProbe = h("rb_meta_probe", FunctionDescriptor.of(PTR, PTR))

    // ---- player -------------------------------------------------------
    val playerNew = h("rb_player_new", FunctionDescriptor.of(PTR))
    val playerNewWithConfig = h(
        "rb_player_new_with_config",
        FunctionDescriptor.of(PTR, I32, F32, F32, I32, F32, BOOL, I32, I32, I32, I32, I32, I32),
    )
    val playerFree = h("rb_player_free", FunctionDescriptor.ofVoid(PTR))
    val playerSetQueueJson = h("rb_player_set_queue_json", FunctionDescriptor.ofVoid(PTR, PTR))
    val playerEnqueue = h("rb_player_enqueue", FunctionDescriptor.ofVoid(PTR, PTR))
    val playerPlay = h("rb_player_play", FunctionDescriptor.ofVoid(PTR))
    val playerPause = h("rb_player_pause", FunctionDescriptor.ofVoid(PTR))
    val playerToggle = h("rb_player_toggle", FunctionDescriptor.ofVoid(PTR))
    val playerStop = h("rb_player_stop", FunctionDescriptor.ofVoid(PTR))
    val playerNext = h("rb_player_next", FunctionDescriptor.ofVoid(PTR))
    val playerPrevious = h("rb_player_previous", FunctionDescriptor.ofVoid(PTR))
    val playerSkipTo = h("rb_player_skip_to", FunctionDescriptor.ofVoid(PTR, I64))
    val playerSeekMs = h("rb_player_seek_ms", FunctionDescriptor.ofVoid(PTR, I64))
    val playerSetVolume = h("rb_player_set_volume", FunctionDescriptor.ofVoid(PTR, F32))
    val playerSetCrossfade =
        h("rb_player_set_crossfade", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32, I32, I32))
    val playerSetReplaygain =
        h("rb_player_set_replaygain", FunctionDescriptor.ofVoid(PTR, I32, F32, BOOL))
    val playerVolume = h("rb_player_volume", FunctionDescriptor.of(F32, PTR))
    val playerSampleRate = h("rb_player_sample_rate", FunctionDescriptor.of(I32, PTR))
    val playerStatusJson = h("rb_player_status_json", FunctionDescriptor.of(PTR, PTR))

    /**
     * Copy a heap C string returned by the ABI into a String, then free it.
     * Returns null for a NULL pointer (the ABI's error/absent signal).
     */
    fun takeString(seg: MemorySegment?): String? {
        if (seg == null || seg.address() == 0L) return null
        val s = seg.reinterpret(Long.MAX_VALUE).getString(0)
        stringFree.invokeWithArguments(seg)
        return s
    }

    // ---- library location ---------------------------------------------

    /**
     * Locate the shared library. Precedence:
     *   1. ROCKBOX_FFI_LIB env var (explicit override)
     *   2. a `Libs/` dir bundled next to the working directory (distributions)
     *   3. target/release, walking up from the working directory (repo checkout)
     */
    private fun locateLibrary(): String {
        val names = listOf("librockbox_ffi.dylib", "librockbox_ffi.so", "rockbox_ffi.dll")
        val tried = mutableListOf<String>()

        fun attempt(path: String): String? {
            tried += path
            return if (File(path).exists()) path else null
        }

        System.getenv("ROCKBOX_FFI_LIB")?.let { env -> attempt(env)?.let { return it } }

        val start = File(System.getProperty("user.dir")).absoluteFile
        var dir: File? = start
        while (dir != null) {
            for (sub in listOf("target/release", "Libs")) {
                for (name in names) {
                    attempt(File(dir, "$sub/$name").path)?.let { return it }
                }
            }
            dir = dir.parentFile
        }

        throw RockboxException(
            "could not locate librockbox_ffi shared library. Set ROCKBOX_FFI_LIB or run " +
                "`cargo build --release -p rockbox-ffi`. Tried:\n  " + tried.joinToString("\n  "),
        )
    }
}

/** ABI major version of the loaded library (bumped on breaking changes). */
fun abiVersion(): Long = (Native.abiVersion.invokeWithArguments() as Int).toLong() and 0xFFFFFFFFL
