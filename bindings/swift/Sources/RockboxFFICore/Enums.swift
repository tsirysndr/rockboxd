// Note the two *different* ReplayGain encodings in the C ABI.

/// Values for `Dsp.setReplaygain` (native Rockbox).
public enum DspReplayGainMode: Int32 {
    case track = 0
    case album = 1
    case shuffle = 2
    case off = 3
}

/// Values for `Player.setReplaygain`.
public enum ReplayGainMode: Int32 {
    case off = 0
    case track = 1
    case album = 2
}

public enum CrossfadeMode: Int32 {
    case off = 0
    case autoSkip = 1
    case manualSkip = 2
    case shuffle = 3
    case shuffleOrManual = 4
    case always = 5
}

public enum MixMode: Int32 {
    case crossfade = 0
    case mix = 1
}

/// Where to place inserted / imported tracks in the queue.
public enum InsertPosition: Int32 {
    case prepend = 0
    case insert = 1
    case insertNext = 2
    case insertLast = 3
    case insertShuffled = 4
    case insertLastShuffled = 5
    case replace = 6
    /// Explicit index — uses the `index` argument.
    case index = 7
}

public enum ChannelConfig: Int32 {
    case stereo = 0
    case mono = 1
    case custom = 2
    case monoLeft = 3
    case monoRight = 4
    case karaoke = 5
    case swap = 6
}

/// Built-in EQ presets for `Player.setEqPreset`.
public enum EqPreset: Int32 {
    case flat = 0
    case acoustic = 1
    case bassBoost = 2
    case bassReducer = 3
    case classical = 4
    case dance = 5
    case deep = 6
    case electronic = 7
    case hipHop = 8
    case jazz = 9
    case latin = 10
    case loudness = 11
    case lounge = 12
    case piano = 13
    case pop = 14
    case rnb = 15
    case rock = 16
    case smallSpeakers = 17
    case trebleBoost = 18
    case trebleReducer = 19
    case vocalBoost = 20
}

/// Channel mixing modes for `Player.setChannelMode`.
public enum ChannelMode: Int32 {
    case stereo = 0
    case mono = 1
    case custom = 2
    case monoLeft = 3
    case monoRight = 4
    case karaoke = 5
    case swap = 6
}

/// ABI major version of the loaded library (bumped on breaking changes).
public func abiVersion() -> UInt32 {
    Lib.shared.abiVersion()
}
