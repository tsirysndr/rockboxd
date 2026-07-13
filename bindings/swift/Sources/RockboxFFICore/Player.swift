import Foundation

/// Queue-based player with native ReplayGain and Rockbox crossfade.
///
/// A Player owns a live audio output device and a background engine thread —
/// construct it only where an output device exists. The handle is freed on
/// `close()` or `deinit`.
///
/// ReplayGain `mode` here uses the *player* values: OFF=0, TRACK=1, ALBUM=2
/// (`ReplayGainMode`) — distinct from the DSP encoding.
///
/// Mutating setters are `@discardableResult` and return `self`, so they can be
/// chained fluently — e.g.
/// `try player.setQueue([file]).setShuffle(true).setRepeat(.all).play()` —
/// while existing statement-style calls still compile without warnings.
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
        /// Auto-persist the queue + exact position to this `.m3u8` file.
        /// nil/empty disables resume.
        public var resumeFile: String? = nil
        /// Save interval for the resume file; 0 uses the ABI default (5 s).
        public var resumeSaveIntervalMs: UInt32 = 0

        public init() {}
    }

    private var ptr: OpaquePointer?
    private let lib = Lib.shared

    /// Create a player with configuration overrides.
    ///
    /// When `config.resumeFile` is set the queue and exact position are
    /// auto-persisted to it (see `rb_player_new_with_config_ex`).
    public init(config: Config = Config()) throws {
        let make: (UnsafePointer<CChar>?) -> OpaquePointer? = { resume in
            self.lib.playerNewWithConfigEx(
                config.sampleRate, config.bufferSeconds, config.volume,
                config.replaygainMode.rawValue, config.replaygainPreampDb,
                config.replaygainPreventClipping, config.crossfadeMode.rawValue,
                config.fadeOutDelayMs, config.fadeOutDurationMs,
                config.fadeInDelayMs, config.fadeInDurationMs, config.mixMode.rawValue,
                resume, config.resumeSaveIntervalMs
            )
        }
        let created: OpaquePointer?
        if let resumeFile = config.resumeFile {
            created = resumeFile.withCString { make($0) }
        } else {
            created = make(nil)
        }
        guard let p = created else {
            throw RockboxError.nullReturn("rb_player_new_with_config_ex (no output device?)")
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

    /// Replace the queue. Each entry may be a local file path, an
    /// `http(s)://` URL to a finite remote file, or a live-radio /
    /// streaming URL.
    @discardableResult
    public func setQueue(_ paths: [String]) throws -> Self {
        let json = String(data: try JSONSerialization.data(withJSONObject: paths), encoding: .utf8)!
        json.withCString { lib.playerSetQueueJson(ptr, $0) }
        return self
    }

    /// Append one track to the queue. `path` may be a local file path, an
    /// `http(s)://` URL to a finite remote file, or a live-radio / streaming
    /// URL.
    @discardableResult
    public func enqueue(_ path: String) -> Self {
        path.withCString { lib.playerEnqueue(ptr, $0) }
        return self
    }

    /// Insert paths/URLs into the queue at `position` (`index` used for `.index`).
    @discardableResult
    public func insert(_ paths: [String], position: InsertPosition, index: Int = 0) throws -> Self {
        let json = String(data: try JSONSerialization.data(withJSONObject: paths), encoding: .utf8)!
        json.withCString { lib.playerInsertJson(ptr, $0, position.rawValue, index) }
        return self
    }

    /// The current queue as an array of paths/URLs.
    public func queue() -> [String] {
        guard let json = lib.takeString(lib.playerQueueJson(ptr)),
              let data = json.data(using: .utf8),
              let arr = try? JSONSerialization.jsonObject(with: data) as? [String]
        else { return [] }
        return arr
    }

    // MARK: - transport

    @discardableResult public func play() -> Self { lib.playerPlay(ptr); return self }
    @discardableResult public func pause() -> Self { lib.playerPause(ptr); return self }
    @discardableResult public func toggle() -> Self { lib.playerToggle(ptr); return self }
    @discardableResult public func stop() -> Self { lib.playerStop(ptr); return self }
    @discardableResult public func next() -> Self { lib.playerNext(ptr); return self }
    @discardableResult public func previous() -> Self { lib.playerPrevious(ptr); return self }
    @discardableResult public func skip(to index: Int) -> Self { lib.playerSkipTo(ptr, index); return self }
    @discardableResult public func seek(ms: UInt64) -> Self { lib.playerSeekMs(ptr, ms); return self }

    // MARK: - settings

    @discardableResult public func setVolume(_ vol: Float) -> Self { lib.playerSetVolume(ptr, vol); return self }
    /// Stereo balance, -100 (full left) to +100 (full right); 0 = centre.
    @discardableResult public func setBalance(_ balance: Int32) -> Self { lib.playerSetBalance(ptr, balance); return self }
    public var volume: Float { lib.playerVolume(ptr) }
    /// Current stereo balance, -100 (full left) to +100 (full right).
    public var balance: Int32 { lib.playerBalance(ptr) }
    public var sampleRate: UInt32 { lib.playerSampleRate(ptr) }

    @discardableResult
    public func setCrossfade(_ mode: CrossfadeMode, fadeOutDelayMs: UInt32 = 0,
                             fadeOutDurationMs: UInt32 = 2000, fadeInDelayMs: UInt32 = 0,
                             fadeInDurationMs: UInt32 = 2000, mixMode: MixMode = .crossfade) -> Self {
        lib.playerSetCrossfade(ptr, mode.rawValue, fadeOutDelayMs, fadeOutDurationMs,
                               fadeInDelayMs, fadeInDurationMs, mixMode.rawValue)
        return self
    }

    /// `mode`: `ReplayGainMode` (OFF=0, TRACK=1, ALBUM=2).
    @discardableResult
    public func setReplaygain(_ mode: ReplayGainMode, preampDb: Float, preventClipping: Bool) -> Self {
        lib.playerSetReplaygain(ptr, mode.rawValue, preampDb, preventClipping)
        return self
    }

    // MARK: - shuffle / repeat

    /// Enable or disable shuffle.
    @discardableResult
    public func setShuffle(_ enabled: Bool) -> Self { lib.playerSetShuffle(ptr, enabled); return self }

    /// Whether shuffle is currently enabled.
    public func isShuffleEnabled() -> Bool { lib.playerIsShuffleEnabled(ptr) }

    /// Set the repeat mode (`RepeatMode`: off=0, one=1, all=2).
    @discardableResult
    public func setRepeat(_ mode: RepeatMode) -> Self { lib.playerSetRepeat(ptr, mode.rawValue); return self }

    /// The current repeat mode. Falls back to `.off` for an unknown value.
    public func `repeat`() -> RepeatMode { RepeatMode(rawValue: lib.playerRepeat(ptr)) ?? .off }

    // MARK: - status

    /// A snapshot of the player's status as a dictionary.
    public func status() throws -> [String: Any] {
        guard let json = lib.takeString(lib.playerStatusJson(ptr)) else {
            throw RockboxError.nullReturn("rb_player_status_json")
        }
        return try parseObject(json, context: "player status")
    }

    // MARK: - DSP

    /// Enable or disable the parametric EQ.
    @discardableResult
    public func setEqEnabled(_ enabled: Bool) -> Self { lib.playerSetEqEnabled(ptr, enabled); return self }

    /// Whether the parametric EQ is currently enabled.
    public func isEqEnabled() -> Bool { lib.playerIsEqEnabled(ptr) }

    /// Configure one EQ band. `gainDb` is plain dB, `q` a plain Q factor.
    @discardableResult
    public func setEqBand(_ band: Int, cutoffHz: Int32, q: Float, gainDb: Float) -> Self {
        lib.playerSetEqBand(ptr, band, cutoffHz, q, gainDb)
        return self
    }

    /// EQ pre-cut (headroom) in plain dB.
    @discardableResult
    public func setEqPrecut(_ db: Float) -> Self { lib.playerSetEqPrecut(ptr, db); return self }

    /// Apply a built-in EQ preset.
    @discardableResult
    public func setEqPreset(_ preset: EqPreset) -> Self { lib.playerSetEqPreset(ptr, preset.rawValue); return self }

    /// Bass/treble tone control with explicit cutoffs (all in dB / Hz).
    @discardableResult
    public func setTone(bassDb: Int32, trebleDb: Int32, bassCutoffHz: Int32, trebleCutoffHz: Int32) -> Self {
        lib.playerSetTone(ptr, bassDb, trebleDb, bassCutoffHz, trebleCutoffHz)
        return self
    }

    /// Bass gain in dB.
    @discardableResult
    public func setBass(_ bassDb: Int32) -> Self { lib.playerSetBass(ptr, bassDb); return self }

    /// Treble gain in dB.
    @discardableResult
    public func setTreble(_ trebleDb: Int32) -> Self { lib.playerSetTreble(ptr, trebleDb); return self }

    /// Surround effect (delay in ms, balance, and low/high cutoffs in Hz).
    @discardableResult
    public func setSurround(delayMs: Int32, balance: Int32, cutoffLowHz: Int32, cutoffHighHz: Int32) -> Self {
        lib.playerSetSurround(ptr, delayMs, balance, cutoffLowHz, cutoffHighHz)
        return self
    }

    /// Channel mixing mode.
    @discardableResult
    public func setChannelMode(_ mode: ChannelMode) -> Self { lib.playerSetChannelMode(ptr, mode.rawValue); return self }

    /// Stereo width as a percentage.
    @discardableResult
    public func setStereoWidth(_ percent: Int32) -> Self { lib.playerSetStereoWidth(ptr, percent); return self }

    /// Dynamic-range compressor (threshold/makeup in dB, times in ms).
    @discardableResult
    public func setCompressor(thresholdDb: Int32, makeupGain: Int32, ratio: Int32,
                              knee: Int32, attackMs: Int32, releaseMs: Int32) -> Self {
        lib.playerSetCompressor(ptr, thresholdDb, makeupGain, ratio, knee, attackMs, releaseMs)
        return self
    }

    /// Enable or disable output dithering.
    @discardableResult
    public func setDither(_ enabled: Bool) -> Self { lib.playerSetDither(ptr, enabled); return self }

    /// Playback pitch as a ratio (native Rockbox units).
    @discardableResult
    public func setPitch(_ ratio: Int32) -> Self { lib.playerSetPitch(ptr, ratio); return self }

    /// Bass tone-control cutoff frequency in Hz.
    @discardableResult
    public func setBassCutoff(_ hz: Int32) -> Self { lib.playerSetBassCutoff(ptr, hz); return self }

    /// Treble tone-control cutoff frequency in Hz.
    @discardableResult
    public func setTrebleCutoff(_ hz: Int32) -> Self { lib.playerSetTrebleCutoff(ptr, hz); return self }

    /// Crossfeed (headphone stereo narrowing). Gains and cutoff use native
    /// Rockbox units; `directGain` / `crossGain` / `hfGain` / `hfCutoff` are
    /// only consulted in `.custom` mode.
    @discardableResult
    public func setCrossfeed(_ mode: CrossfeedMode, directGain: Int32, crossGain: Int32,
                             hfGain: Int32, hfCutoff: Int32) -> Self {
        lib.playerSetCrossfeed(ptr, mode.rawValue, directGain, crossGain, hfGain, hfCutoff)
        return self
    }

    /// Bass enhancement (strength and precut, in native Rockbox units).
    @discardableResult
    public func setBassEnhancement(strength: Int32, precut: Int32) -> Self {
        lib.playerSetBassEnhancement(ptr, strength, precut)
        return self
    }

    /// Listening-fatigue reduction (treble roll-off) strength.
    @discardableResult
    public func setFatigueReduction(_ strength: Int32) -> Self {
        lib.playerSetFatigueReduction(ptr, strength)
        return self
    }

    /// A snapshot of the current DSP settings as a dictionary.
    public func dspSettings() throws -> [String: Any] {
        guard let json = lib.takeString(lib.playerDspSettingsJson(ptr)) else {
            throw RockboxError.nullReturn("rb_player_dsp_settings_json")
        }
        return try parseObject(json, context: "player dsp settings")
    }

    // MARK: - resume

    /// Restore the queue + exact position from the resume file (does NOT
    /// auto-play). Returns nil when there's nothing to resume.
    public func resume() -> ResumeState? {
        guard let json = lib.takeString(lib.playerResume(ptr)) else { return nil }
        return try? decodeJSON(ResumeState.self, from: json)
    }

    /// Persist the current queue + position to the resume file now.
    public func saveResume() { lib.playerSaveResume(ptr) }

    /// Delete the resume file.
    public func clearResume() { lib.playerClearResume(ptr) }

    // MARK: - m3u / m3u8 playlists

    /// Import a playlist file into the queue at `position`; returns the
    /// imported paths, or nil on error.
    public func importM3u(_ path: String, position: InsertPosition, index: Int = 0) -> [String]? {
        let raw = path.withCString { lib.playerImportM3u(ptr, $0, position.rawValue, index) }
        return decodeStringArray(lib.takeString(raw))
    }

    /// Replace the queue with a playlist file; returns the loaded paths, or nil on error.
    public func loadM3u(_ path: String) -> [String]? {
        let raw = path.withCString { lib.playerLoadM3u(ptr, $0) }
        return decodeStringArray(lib.takeString(raw))
    }

    /// Export the current queue to an `.m3u8` (atomic). Returns true on success.
    public func exportM3u(_ path: String) -> Bool {
        path.withCString { lib.playerExportM3u(ptr, $0) == 0 }
    }
}

// MARK: - Codable payloads

/// A restorable playback position (see `rb_player_resume`).
public struct ResumeState: Decodable {
    public let tracks: [String]
    public let index: Int
    public let elapsedMs: UInt64

    enum CodingKeys: String, CodingKey {
        case tracks
        case index
        case elapsedMs = "elapsed_ms"
    }
}

/// A single entry of a parsed playlist file (see `rb_m3u_read_json`).
public struct M3uEntry: Decodable {
    public let path: String
    public let durationMs: Int?
    public let title: String?

    enum CodingKeys: String, CodingKey {
        case path
        case durationMs = "duration_ms"
        case title
    }
}

// MARK: - standalone helpers (no player required)

/// Peek at a resume file without a player. Returns nil when absent/invalid.
public func loadResume(_ path: String) -> ResumeState? {
    let lib = Lib.shared
    let raw = path.withCString { lib.loadResumeJson($0) }
    guard let json = lib.takeString(raw) else { return nil }
    return try? decodeJSON(ResumeState.self, from: json)
}

/// Parse a playlist file into its entries. Returns nil on error.
public func m3uRead(_ path: String) -> [M3uEntry]? {
    let lib = Lib.shared
    let raw = path.withCString { lib.m3uReadJson($0) }
    guard let json = lib.takeString(raw) else { return nil }
    return try? decodeJSON([M3uEntry].self, from: json)
}

/// Write an array of paths as an `.m3u8`. Returns true on success.
public func m3uWrite(_ path: String, _ paths: [String]) -> Bool {
    let lib = Lib.shared
    guard let json = String(data: (try? JSONSerialization.data(withJSONObject: paths)) ?? Data(),
                            encoding: .utf8) else { return false }
    return path.withCString { p in
        json.withCString { j in lib.m3uWriteJson(p, j) == 0 }
    }
}

/// Whether a string looks like an http(s):// URL.
public func isURL(_ s: String) -> Bool {
    let lib = Lib.shared
    return s.withCString { lib.isUrl($0) }
}

// MARK: - JSON decode helpers

private func decodeJSON<T: Decodable>(_ type: T.Type, from json: String) throws -> T {
    guard let data = json.data(using: .utf8) else {
        throw RockboxError.invalidInput("could not decode JSON")
    }
    return try JSONDecoder().decode(T.self, from: data)
}

private func decodeStringArray(_ json: String?) -> [String]? {
    guard let json = json, let data = json.data(using: .utf8),
          let arr = try? JSONSerialization.jsonObject(with: data) as? [String]
    else { return nil }
    return arr
}
