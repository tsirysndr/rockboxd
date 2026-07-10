import Foundation

/// The DSP pipeline: EQ, tone, surround, compressor, ReplayGain, resampler.
///
/// The underlying dsp_config is a process-wide singleton, so only one Dsp may
/// exist at a time and it must be used from one thread. The handle is freed on
/// `close()` or `deinit`.
public final class Dsp {
    private var ptr: OpaquePointer?
    private let lib = Lib.shared

    public init(sampleRate: UInt32) throws {
        guard let p = lib.dspNew(sampleRate) else {
            throw RockboxError.nullReturn("rb_dsp_new")
        }
        ptr = p
    }

    deinit { close() }

    /// Free the native handle. Safe to call more than once.
    public func close() {
        if let p = ptr { lib.dspFree(p); ptr = nil }
    }

    public var isClosed: Bool { ptr == nil }

    // MARK: - configuration

    public func setInputFrequency(_ hz: UInt32) { lib.dspSetInputFrequency(ptr, hz) }
    public func flush() { lib.dspFlush(ptr) }
    public func eqEnable(_ enable: Bool) { lib.dspEqEnable(ptr, enable) }

    /// Configure one EQ band (0...9). Band 0 is a low shelf, 9 a high shelf.
    public func setEqBand(_ band: Int, cutoffHz: Int32, q: Float, gainDb: Float) {
        lib.dspSetEqBand(ptr, band, cutoffHz, q, gainDb)
    }

    public func setEqPrecut(_ db: Float) { lib.dspSetEqPrecut(ptr, db) }
    public func setTone(bassDb: Int32, trebleDb: Int32) { lib.dspSetTone(ptr, bassDb, trebleDb) }
    public func setToneCutoffs(bassHz: Int32, trebleHz: Int32) { lib.dspSetToneCutoffs(ptr, bassHz, trebleHz) }

    public func setSurround(delayMs: Int32, balance: Int32, fx1: Int32, fx2: Int32) {
        lib.dspSetSurround(ptr, delayMs, balance, fx1, fx2)
    }

    public func setChannelConfig(_ mode: ChannelConfig) { lib.dspSetChannelConfig(ptr, mode.rawValue) }
    public func setStereoWidth(percent: Int32) { lib.dspSetStereoWidth(ptr, percent) }

    public func setCompressor(threshold: Int32, makeupGain: Int32, ratio: Int32,
                              knee: Int32, releaseTime: Int32, attackTime: Int32) {
        lib.dspSetCompressor(ptr, threshold, makeupGain, ratio, knee, releaseTime, attackTime)
    }

    /// `mode`: `DspReplayGainMode` (TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3).
    public func setReplaygain(_ mode: DspReplayGainMode, noclip: Bool, preampDb: Float) {
        lib.dspSetReplaygain(ptr, mode.rawValue, noclip, preampDb)
    }

    /// Per-track gains in plain dB / peaks as linear amplitude (1.0 = full
    /// scale). `nil` for any absent tag (mapped to the ABI's NaN sentinel).
    public func setReplaygainGains(trackGainDb: Float? = nil, albumGainDb: Float? = nil,
                                   trackPeak: Float? = nil, albumPeak: Float? = nil) {
        lib.dspSetReplaygainGains(ptr, trackGainDb ?? .nan, albumGainDb ?? .nan,
                                  trackPeak ?? .nan, albumPeak ?? .nan)
    }

    /// Native Q7.24 linear factors (the `raw_*` fields from Metadata.read),
    /// 0 = not tagged.
    public func setReplaygainGainsRaw(trackGain: Int64, albumGain: Int64,
                                      trackPeak: Int64, albumPeak: Int64) {
        lib.dspSetReplaygainGainsRaw(ptr, trackGain, albumGain, trackPeak, albumPeak)
    }

    // MARK: - processing

    /// Run interleaved stereo S16 `samples` through the pipeline. Returns a
    /// new array of processed samples (length may differ from the input — the
    /// resampler buffers).
    public func process(_ samples: [Int16]) throws -> [Int16] {
        guard samples.count % 2 == 0 else {
            throw RockboxError.invalidInput("input must be interleaved stereo (even length)")
        }
        var outLen: Int = 0
        let outPtr: UnsafeMutablePointer<Int16>? = samples.withUnsafeBufferPointer { buf in
            lib.dspProcess(ptr, buf.baseAddress, samples.count, &outLen)
        }
        guard let out = outPtr, outLen > 0 else { return [] }
        defer { lib.bufferFree(out, outLen) }
        return Array(UnsafeBufferPointer(start: out, count: outLen))
    }
}

/// Generate `seconds` of a sine as interleaved stereo int16.
public func sineStereo(freqHz: Double, seconds: Double, rate: UInt32,
                       amplitude: Double = 16_000) -> [Int16] {
    let n = Int(seconds * Double(rate))
    var buf = [Int16](repeating: 0, count: n * 2)
    for i in 0..<n {
        let v = sin(Double(i) * 2.0 * Double.pi * freqHz / Double(rate)) * amplitude
        let s = Int16(max(-32_768, min(32_767, Int(v))))
        buf[i * 2] = s
        buf[i * 2 + 1] = s
    }
    return buf
}
