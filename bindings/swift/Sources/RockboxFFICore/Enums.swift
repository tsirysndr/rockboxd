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

public enum ChannelConfig: Int32 {
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
