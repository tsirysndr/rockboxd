import Foundation

/// Queue-based player with native ReplayGain and Rockbox crossfade.
///
/// A Player owns a live audio output device and a background engine thread —
/// construct it only where an output device exists. The handle is freed on
/// `close()` or `deinit`.
///
/// ReplayGain `mode` here uses the *player* values: OFF=0, TRACK=1, ALBUM=2
/// (`ReplayGainMode`) — distinct from the DSP encoding.
public final class Player {
    /// Overridable construction parameters (see `rb_player_new_with_config`).
    public struct Config {
        public var sampleRate: UInt32 = 0 // 0 => device default
        public var bufferSeconds: Float = 4.0
        public var volume: Float = 1.0
        public var replaygainMode: ReplayGainMode = .off
        public var replaygainPreampDb: Float = 0.0
        public var replaygainPreventClipping: Bool = true
        public var crossfadeMode: CrossfadeMode = .off
        public var fadeOutDelayMs: UInt32 = 0
        public var fadeOutDurationMs: UInt32 = 2000
        public var fadeInDelayMs: UInt32 = 0
        public var fadeInDurationMs: UInt32 = 2000
        public var mixMode: MixMode = .crossfade

        public init() {}
    }

    private var ptr: OpaquePointer?
    private let lib = Lib.shared

    /// Create a player with configuration overrides.
    public init(config: Config = Config()) throws {
        guard let p = lib.playerNewWithConfig(
            config.sampleRate, config.bufferSeconds, config.volume,
            config.replaygainMode.rawValue, config.replaygainPreampDb,
            config.replaygainPreventClipping, config.crossfadeMode.rawValue,
            config.fadeOutDelayMs, config.fadeOutDurationMs,
            config.fadeInDelayMs, config.fadeInDurationMs, config.mixMode.rawValue
        ) else {
            throw RockboxError.nullReturn("rb_player_new_with_config (no output device?)")
        }
        ptr = p
    }

    /// Player on the default device with Rockbox default settings.
    public static func makeDefault() throws -> Player {
        let lib = Lib.shared
        guard let p = lib.playerNew() else {
            throw RockboxError.nullReturn("rb_player_new (no output device?)")
        }
        return Player(adopting: p)
    }

    private init(adopting p: OpaquePointer) { ptr = p }

    deinit { close() }

    /// Free the native handle. Safe to call more than once.
    public func close() {
        if let p = ptr { lib.playerFree(p); ptr = nil }
    }

    public var isClosed: Bool { ptr == nil }

    // MARK: - queue

    public func setQueue(_ paths: [String]) throws {
        let json = String(data: try JSONSerialization.data(withJSONObject: paths), encoding: .utf8)!
        json.withCString { lib.playerSetQueueJson(ptr, $0) }
    }

    public func enqueue(_ path: String) {
        path.withCString { lib.playerEnqueue(ptr, $0) }
    }

    // MARK: - transport

    public func play() { lib.playerPlay(ptr) }
    public func pause() { lib.playerPause(ptr) }
    public func toggle() { lib.playerToggle(ptr) }
    public func stop() { lib.playerStop(ptr) }
    public func next() { lib.playerNext(ptr) }
    public func previous() { lib.playerPrevious(ptr) }
    public func skip(to index: Int) { lib.playerSkipTo(ptr, index) }
    public func seek(ms: UInt64) { lib.playerSeekMs(ptr, ms) }

    // MARK: - settings

    public func setVolume(_ vol: Float) { lib.playerSetVolume(ptr, vol) }
    public var volume: Float { lib.playerVolume(ptr) }
    public var sampleRate: UInt32 { lib.playerSampleRate(ptr) }

    public func setCrossfade(_ mode: CrossfadeMode, fadeOutDelayMs: UInt32 = 0,
                             fadeOutDurationMs: UInt32 = 2000, fadeInDelayMs: UInt32 = 0,
                             fadeInDurationMs: UInt32 = 2000, mixMode: MixMode = .crossfade) {
        lib.playerSetCrossfade(ptr, mode.rawValue, fadeOutDelayMs, fadeOutDurationMs,
                               fadeInDelayMs, fadeInDurationMs, mixMode.rawValue)
    }

    /// `mode`: `ReplayGainMode` (OFF=0, TRACK=1, ALBUM=2).
    public func setReplaygain(_ mode: ReplayGainMode, preampDb: Float, preventClipping: Bool) {
        lib.playerSetReplaygain(ptr, mode.rawValue, preampDb, preventClipping)
    }

    // MARK: - status

    /// A snapshot of the player's status as a dictionary.
    public func status() throws -> [String: Any] {
        guard let json = lib.takeString(lib.playerStatusJson(ptr)) else {
            throw RockboxError.nullReturn("rb_player_status_json")
        }
        return try parseObject(json, context: "player status")
    }
}
