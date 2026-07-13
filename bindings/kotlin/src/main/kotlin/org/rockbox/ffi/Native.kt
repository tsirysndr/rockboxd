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

    // ---- decoder ------------------------------------------------------
    val decoderOpen = h("rb_decoder_open", FunctionDescriptor.of(PTR, PTR))
    val decoderFree = h("rb_decoder_free", FunctionDescriptor.ofVoid(PTR))
    val decoderMetadataJson = h("rb_decoder_metadata_json", FunctionDescriptor.of(PTR, PTR))
    val decoderNextChunk = h("rb_decoder_next_chunk", FunctionDescriptor.of(PTR, PTR, PTR, PTR))
    val decoderSeekMs = h("rb_decoder_seek_ms", FunctionDescriptor.ofVoid(PTR, I64))
    val decoderElapsedMs = h("rb_decoder_elapsed_ms", FunctionDescriptor.of(I64, PTR))
    val decoderFinished = h("rb_decoder_finished", FunctionDescriptor.of(BOOL, PTR, PTR))

    // ---- player -------------------------------------------------------
    val playerNew = h("rb_player_new", FunctionDescriptor.of(PTR))
    val playerNewWithConfig = h(
        "rb_player_new_with_config",
        FunctionDescriptor.of(PTR, I32, F32, F32, I32, F32, BOOL, I32, I32, I32, I32, I32, I32),
    )
    val playerNewWithConfigEx = h(
        "rb_player_new_with_config_ex",
        FunctionDescriptor.of(
            PTR, I32, F32, F32, I32, F32, BOOL, I32, I32, I32, I32, I32, I32, PTR, I32,
        ),
    )
    val playerFree = h("rb_player_free", FunctionDescriptor.ofVoid(PTR))
    val playerSetQueueJson = h("rb_player_set_queue_json", FunctionDescriptor.ofVoid(PTR, PTR))
    val playerEnqueue = h("rb_player_enqueue", FunctionDescriptor.ofVoid(PTR, PTR))
    val playerInsertJson = h("rb_player_insert_json", FunctionDescriptor.ofVoid(PTR, PTR, I32, I64))
    val playerRemove = h("rb_player_remove", FunctionDescriptor.ofVoid(PTR, I64))
    val playerClearQueue = h("rb_player_clear_queue", FunctionDescriptor.ofVoid(PTR))
    val playerQueueJson = h("rb_player_queue_json", FunctionDescriptor.of(PTR, PTR))
    val playerPlay = h("rb_player_play", FunctionDescriptor.ofVoid(PTR))
    val playerPause = h("rb_player_pause", FunctionDescriptor.ofVoid(PTR))
    val playerToggle = h("rb_player_toggle", FunctionDescriptor.ofVoid(PTR))
    val playerStop = h("rb_player_stop", FunctionDescriptor.ofVoid(PTR))
    val playerNext = h("rb_player_next", FunctionDescriptor.ofVoid(PTR))
    val playerPrevious = h("rb_player_previous", FunctionDescriptor.ofVoid(PTR))
    val playerSkipTo = h("rb_player_skip_to", FunctionDescriptor.ofVoid(PTR, I64))
    val playerSeekMs = h("rb_player_seek_ms", FunctionDescriptor.ofVoid(PTR, I64))
    val playerSetVolume = h("rb_player_set_volume", FunctionDescriptor.ofVoid(PTR, F32))
    val playerSetBalance = h("rb_player_set_balance", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetCrossfade =
        h("rb_player_set_crossfade", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32, I32, I32))
    val playerSetReplaygain =
        h("rb_player_set_replaygain", FunctionDescriptor.ofVoid(PTR, I32, F32, BOOL))
    val playerVolume = h("rb_player_volume", FunctionDescriptor.of(F32, PTR))
    val playerBalance = h("rb_player_balance", FunctionDescriptor.of(I32, PTR))
    val playerSampleRate = h("rb_player_sample_rate", FunctionDescriptor.of(I32, PTR))
    val playerStatusJson = h("rb_player_status_json", FunctionDescriptor.of(PTR, PTR))
    val playerSetShuffle = h("rb_player_set_shuffle", FunctionDescriptor.ofVoid(PTR, BOOL))
    val playerIsShuffleEnabled = h("rb_player_is_shuffle_enabled", FunctionDescriptor.of(BOOL, PTR))
    val playerSetRepeat = h("rb_player_set_repeat", FunctionDescriptor.ofVoid(PTR, I32))
    val playerRepeat = h("rb_player_repeat", FunctionDescriptor.of(I32, PTR))

    // ---- player DSP ---------------------------------------------------
    val playerSetEqEnabled = h("rb_player_set_eq_enabled", FunctionDescriptor.ofVoid(PTR, BOOL))
    val playerIsEqEnabled = h("rb_player_is_eq_enabled", FunctionDescriptor.of(BOOL, PTR))
    val playerSetEqBand = h("rb_player_set_eq_band", FunctionDescriptor.ofVoid(PTR, I64, I32, F32, F32))
    val playerSetEqPrecut = h("rb_player_set_eq_precut", FunctionDescriptor.ofVoid(PTR, F32))
    val playerSetEqPreset = h("rb_player_set_eq_preset", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetTone = h("rb_player_set_tone", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32))
    val playerSetBass = h("rb_player_set_bass", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetTreble = h("rb_player_set_treble", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetSurround = h("rb_player_set_surround", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32))
    val playerSetChannelMode = h("rb_player_set_channel_mode", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetStereoWidth = h("rb_player_set_stereo_width", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetCompressor =
        h("rb_player_set_compressor", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32, I32, I32))
    val playerSetDither = h("rb_player_set_dither", FunctionDescriptor.ofVoid(PTR, BOOL))
    val playerSetPitch = h("rb_player_set_pitch", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetBassCutoff = h("rb_player_set_bass_cutoff", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetTrebleCutoff = h("rb_player_set_treble_cutoff", FunctionDescriptor.ofVoid(PTR, I32))
    val playerSetCrossfeed =
        h("rb_player_set_crossfeed", FunctionDescriptor.ofVoid(PTR, I32, I32, I32, I32, I32))
    val playerSetBassEnhancement =
        h("rb_player_set_bass_enhancement", FunctionDescriptor.ofVoid(PTR, I32, I32))
    val playerSetFatigueReduction =
        h("rb_player_set_fatigue_reduction", FunctionDescriptor.ofVoid(PTR, I32))
    val playerDspSettingsJson = h("rb_player_dsp_settings_json", FunctionDescriptor.of(PTR, PTR))

    // ---- resume -------------------------------------------------------
    val playerResume = h("rb_player_resume", FunctionDescriptor.of(PTR, PTR))
    val playerSaveResume = h("rb_player_save_resume", FunctionDescriptor.ofVoid(PTR))
    val playerClearResume = h("rb_player_clear_resume", FunctionDescriptor.ofVoid(PTR))
    val loadResumeJson = h("rb_load_resume_json", FunctionDescriptor.of(PTR, PTR))

    // ---- m3u / m3u8 ---------------------------------------------------
    val playerImportM3u = h("rb_player_import_m3u", FunctionDescriptor.of(PTR, PTR, PTR, I32, I64))
    val playerLoadM3u = h("rb_player_load_m3u", FunctionDescriptor.of(PTR, PTR, PTR))
    val playerExportM3u = h("rb_player_export_m3u", FunctionDescriptor.of(I32, PTR, PTR))
    val m3uReadJson = h("rb_m3u_read_json", FunctionDescriptor.of(PTR, PTR))
    val m3uWriteJson = h("rb_m3u_write_json", FunctionDescriptor.of(I32, PTR, PTR))
    val isUrl = h("rb_is_url", FunctionDescriptor.of(BOOL, PTR))

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
     *   2. the native lib bundled in the jar for this OS/arch (published pkg)
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

        extractBundled(tried)?.let { return it }

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

    /** The `<os>-<arch>` release target for the running JVM, or null if unsupported. */
    private fun nativeTarget(): String? {
        val os = System.getProperty("os.name").lowercase()
        val arch = System.getProperty("os.arch").lowercase()
        val o = when {
            os.contains("mac") || os.contains("darwin") -> "darwin"
            os.contains("linux") -> "linux"
            os.contains("freebsd") -> "freebsd"
            os.contains("netbsd") -> "netbsd"
            else -> return null
        }
        val a = when (arch) {
            "aarch64", "arm64" -> "arm64"
            "amd64", "x86_64", "x64" -> "x64"
            else -> return null
        }
        return "$o-$a"
    }

    /**
     * Extract the platform's bundled `native/<target>/librockbox_ffi.<ext>` jar
     * resource to a temp file and return its path, or null if none is bundled.
     */
    private fun extractBundled(tried: MutableList<String>): String? {
        val target = nativeTarget() ?: return null
        val ext = if (target.startsWith("darwin")) "dylib" else "so"
        val resource = "native/$target/librockbox_ffi.$ext"
        tried += "classpath:$resource"
        val stream = Native::class.java.classLoader.getResourceAsStream(resource) ?: return null
        return stream.use { s ->
            val tmp = File.createTempFile("librockbox_ffi", ".$ext")
            tmp.deleteOnExit()
            java.io.FileOutputStream(tmp).use { out -> s.copyTo(out) }
            tmp.absolutePath
        }
    }
}

/** ABI major version of the loaded library (bumped on breaking changes). */
fun abiVersion(): Long = (Native.abiVersion.invokeWithArguments() as Int).toLong() and 0xFFFFFFFFL
