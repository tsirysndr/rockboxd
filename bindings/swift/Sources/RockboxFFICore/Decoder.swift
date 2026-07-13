import Foundation

/// Streaming decoder for one local audio file: interleaved-stereo S16 PCM,
/// one chunk at a time.
///
/// Wraps the Rockbox codec engine, whose state is process-wide, so only one
/// Decoder may decode at a time — constructing a second one blocks until the
/// first is closed. The handle is freed on `close()` or `deinit`.
public final class Decoder {
    private var ptr: OpaquePointer?
    private let lib = Lib.shared

    /// Open the audio file at `path` for decoding. Throws if the codec engine
    /// could not open it.
    public init(path: String) throws {
        guard let p = path.withCString({ lib.decoderOpen($0) }) else {
            throw RockboxError.nullReturn("rb_decoder_open(\(path))")
        }
        ptr = p
    }

    deinit { close() }

    /// Free the native handle. Safe to call more than once.
    public func close() {
        if let p = ptr { lib.decoderFree(p); ptr = nil }
    }

    public var isClosed: Bool { ptr == nil }

    // MARK: - metadata

    /// Tags and stream properties (same shape as `Metadata.read`). Throws on
    /// failure.
    public func metadata() throws -> [String: Any] {
        guard let json = lib.takeString(lib.decoderMetadataJson(ptr)) else {
            throw RockboxError.nullReturn("rb_decoder_metadata_json")
        }
        return try parseObject(json, context: "decoder metadata")
    }

    // MARK: - decoding

    /// Next buffer of decoded PCM as `(samples, sampleRate)`. `samples` is
    /// interleaved stereo int16. Returns nil at end of track (or after a codec
    /// error — see `finished()`).
    public func nextChunk() -> (samples: [Int16], sampleRate: UInt32)? {
        var outLen: Int = 0
        var outRate: UInt32 = 0
        let ptr = lib.decoderNextChunk(self.ptr, &outLen, &outRate)
        guard let out = ptr, outLen > 0 else { return nil }
        defer { lib.bufferFree(out, outLen) }
        return (Array(UnsafeBufferPointer(start: out, count: outLen)), outRate)
    }

    /// Request a seek to `ms` milliseconds.
    public func seek(ms: UInt64) { lib.decoderSeekMs(ptr, ms) }

    /// Playback position last reported by the codec, in milliseconds.
    public var elapsedMs: UInt64 { lib.decoderElapsedMs(ptr) }

    /// `(done, code)`: `done` is true once the track has ended; `code` is the
    /// exit status (0 = clean end, negative = codec error), valid only when
    /// `done`.
    public func finished() -> (done: Bool, code: Int32) {
        var code: Int32 = 0
        let done = lib.decoderFinished(ptr, &code)
        return (done, code)
    }
}
