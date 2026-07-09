package org.rockbox.ffi

import java.lang.foreign.Arena
import java.lang.foreign.MemorySegment
import java.lang.foreign.ValueLayout
import kotlin.math.PI
import kotlin.math.sin

/**
 * The DSP pipeline: EQ, tone, surround, compressor, ReplayGain, resampler.
 *
 * The underlying dsp_config is a process-wide singleton, so only one Dsp may
 * exist at a time and it must be used from one thread. The handle is freed on
 * [close]; the class is [AutoCloseable] for use with `use { }`.
 */
class Dsp(sampleRate: Long) : AutoCloseable {
    private var ptr: MemorySegment? =
        (Native.dspNew.invokeWithArguments(sampleRate.toInt()) as MemorySegment).let {
            if (it.address() == 0L) throw RockboxException("rb_dsp_new returned NULL") else it
        }

    private fun handle(): MemorySegment =
        ptr ?: throw RockboxException("Dsp has been closed")

    /** Free the native handle. Safe to call more than once. */
    override fun close() {
        ptr?.let { Native.dspFree.invokeWithArguments(it); ptr = null }
    }

    val isClosed: Boolean get() = ptr == null

    // ---- configuration ------------------------------------------------

    fun setInputFrequency(hz: Long) = Native.dspSetInputFrequency.invokeWithArguments(handle(), hz.toInt()).unit()
    fun flush() = Native.dspFlush.invokeWithArguments(handle()).unit()
    fun eqEnable(enable: Boolean) = Native.dspEqEnable.invokeWithArguments(handle(), enable).unit()

    /** Configure one EQ band (0..9). Band 0 is a low shelf, 9 a high shelf. */
    fun setEqBand(band: Long, cutoffHz: Int, q: Float, gainDb: Float) =
        Native.dspSetEqBand.invokeWithArguments(handle(), band, cutoffHz, q, gainDb).unit()

    fun setEqPrecut(db: Float) = Native.dspSetEqPrecut.invokeWithArguments(handle(), db).unit()
    fun setTone(bassDb: Int, trebleDb: Int) = Native.dspSetTone.invokeWithArguments(handle(), bassDb, trebleDb).unit()
    fun setToneCutoffs(bassHz: Int, trebleHz: Int) =
        Native.dspSetToneCutoffs.invokeWithArguments(handle(), bassHz, trebleHz).unit()

    fun setSurround(delayMs: Int, balance: Int, fx1: Int, fx2: Int) =
        Native.dspSetSurround.invokeWithArguments(handle(), delayMs, balance, fx1, fx2).unit()

    fun setChannelConfig(mode: ChannelConfig) =
        Native.dspSetChannelConfig.invokeWithArguments(handle(), mode.value).unit()

    fun setStereoWidth(percent: Int) = Native.dspSetStereoWidth.invokeWithArguments(handle(), percent).unit()

    fun setCompressor(threshold: Int, makeupGain: Int, ratio: Int, knee: Int, releaseTime: Int, attackTime: Int) =
        Native.dspSetCompressor.invokeWithArguments(
            handle(), threshold, makeupGain, ratio, knee, releaseTime, attackTime,
        ).unit()

    /** [mode]: [DspReplayGainMode] (TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3). */
    fun setReplaygain(mode: DspReplayGainMode, noclip: Boolean, preampDb: Float) =
        Native.dspSetReplaygain.invokeWithArguments(handle(), mode.value, noclip, preampDb).unit()

    /**
     * Per-track gains in plain dB / peaks as linear amplitude (1.0 = full
     * scale). Pass `Float.NaN` for any absent tag (the ABI's sentinel).
     */
    fun setReplaygainGains(
        trackGainDb: Float = Float.NaN,
        albumGainDb: Float = Float.NaN,
        trackPeak: Float = Float.NaN,
        albumPeak: Float = Float.NaN,
    ) = Native.dspSetReplaygainGains.invokeWithArguments(handle(), trackGainDb, albumGainDb, trackPeak, albumPeak).unit()

    /** Native Q7.24 linear factors (the `raw_*` fields from metadata), 0 = untagged. */
    fun setReplaygainGainsRaw(trackGain: Long, albumGain: Long, trackPeak: Long, albumPeak: Long) =
        Native.dspSetReplaygainGainsRaw.invokeWithArguments(handle(), trackGain, albumGain, trackPeak, albumPeak).unit()

    // ---- processing ---------------------------------------------------

    /**
     * Run interleaved stereo S16 [samples] through the pipeline. Returns a new
     * array of processed samples (length may differ — the resampler buffers).
     */
    fun process(samples: ShortArray): ShortArray {
        require(samples.size % 2 == 0) { "input must be interleaved stereo (even length)" }
        Arena.ofConfined().use { arena ->
            val input = arena.allocate(ValueLayout.JAVA_SHORT, samples.size.toLong())
            MemorySegment.copy(samples, 0, input, ValueLayout.JAVA_SHORT, 0, samples.size)
            val outLen = arena.allocate(ValueLayout.JAVA_LONG)

            val outPtr = Native.dspProcess.invokeWithArguments(
                handle(), input, samples.size.toLong(), outLen,
            ) as MemorySegment
            val n = outLen.get(ValueLayout.JAVA_LONG, 0)
            if (outPtr.address() == 0L || n <= 0) return ShortArray(0)

            val out = outPtr.reinterpret(n * 2)
            val result = ShortArray(n.toInt())
            MemorySegment.copy(out, ValueLayout.JAVA_SHORT, 0, result, 0, n.toInt())
            Native.bufferFree.invokeWithArguments(outPtr, n)
            return result
        }
    }
}

// invokeWithArguments returns Any?; the setters are logically void.
private fun Any?.unit(): Unit = Unit

/** Generate [seconds] of a sine as interleaved stereo int16. */
fun sineStereo(freqHz: Double, seconds: Double, rate: Long, amplitude: Double = 16_000.0): ShortArray {
    val n = (seconds * rate).toInt()
    val buf = ShortArray(n * 2)
    for (i in 0 until n) {
        val v = sin(i * 2.0 * PI * freqHz / rate) * amplitude
        val s = v.toInt().coerceIn(-32_768, 32_767).toShort()
        buf[i * 2] = s
        buf[i * 2 + 1] = s
    }
    return buf
}
