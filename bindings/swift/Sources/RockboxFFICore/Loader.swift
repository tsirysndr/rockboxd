// Runtime loader for the rockbox-ffi C ABI.
//
// Every function is resolved with `dlsym` into a typed `@convention(c)`
// closure. This mirrors include/rockbox_ffi.h — keep the signatures in sync
// with the C ABI. Symbols are located one of two ways, tried in order:
//
//   1. Statically linked (the `RockboxFFI` product `-force_load`s
//      librockbox_ffi.a): the `rb_*` symbols are already in the process
//      image, so they resolve through `RTLD_DEFAULT` with no file lookup.
//   2. Dynamically loaded (the `RockboxFFIDynamic` product): the shared
//      library is `dlopen`ed from ROCKBOX_FFI_LIB / target/release / a
//      bundled `Libs` dir, exactly like the Ruby / Python bindings.
//
// Both products share this one code path; which branch runs is decided at
// runtime by whether the symbols were linked in.

import Foundation

#if canImport(Darwin)
import Darwin
#elseif canImport(Glibc)
import Glibc
#endif

// dlfcn's RTLD_DEFAULT is a `#define` (a magic pseudo-handle), so it doesn't
// import into Swift — spell it out per platform. dlsym with this handle
// searches every image already loaded in the process, which is where the
// `rb_*` symbols live when librockbox_ffi.a is statically linked in.
#if canImport(Darwin)
private let RTLD_DEFAULT = UnsafeMutableRawPointer(bitPattern: -2)
#else
private let RTLD_DEFAULT: UnsafeMutableRawPointer? = nil
#endif

/// Errors thrown by the bindings.
public enum RockboxError: Error, CustomStringConvertible {
    case libraryNotFound(tried: [String])
    case symbolNotFound(String)
    case nullReturn(String)
    case invalidInput(String)

    public var description: String {
        switch self {
        case .libraryNotFound(let tried):
            return "could not locate librockbox_ffi shared library. Set "
                + "ROCKBOX_FFI_LIB or run `cargo build --release -p rockbox-ffi`. "
                + "Tried:\n  " + tried.joined(separator: "\n  ")
        case .symbolNotFound(let name):
            return "symbol \(name) not found in librockbox_ffi"
        case .nullReturn(let fn):
            return "\(fn) returned NULL"
        case .invalidInput(let msg):
            return msg
        }
    }
}

// C ABI function-pointer types (see include/rockbox_ffi.h).
typealias FnAbiVersion = @convention(c) () -> UInt32
typealias FnStringFree = @convention(c) (UnsafeMutablePointer<CChar>?) -> Void
typealias FnBufferFree = @convention(c) (UnsafeMutablePointer<Int16>?, Int) -> Void

typealias FnDspNew = @convention(c) (UInt32) -> OpaquePointer?
typealias FnDspFree = @convention(c) (OpaquePointer?) -> Void
typealias FnDspSetU32 = @convention(c) (OpaquePointer?, UInt32) -> Void
typealias FnDspVoid = @convention(c) (OpaquePointer?) -> Void
typealias FnDspBool = @convention(c) (OpaquePointer?, Bool) -> Void
typealias FnDspI2 = @convention(c) (OpaquePointer?, Int32, Int32) -> Void
typealias FnDspI4 = @convention(c) (OpaquePointer?, Int32, Int32, Int32, Int32) -> Void
typealias FnDspI1 = @convention(c) (OpaquePointer?, Int32) -> Void
typealias FnDspI6 = @convention(c) (OpaquePointer?, Int32, Int32, Int32, Int32, Int32, Int32) -> Void
typealias FnDspRg = @convention(c) (OpaquePointer?, Int32, Bool, Float) -> Void
typealias FnDspRgGains = @convention(c) (OpaquePointer?, Float, Float, Float, Float) -> Void
typealias FnDspRgGainsRaw = @convention(c) (OpaquePointer?, Int64, Int64, Int64, Int64) -> Void
typealias FnDspEqBand = @convention(c) (OpaquePointer?, Int, Int32, Float, Float) -> Void
typealias FnDspEqPrecut = @convention(c) (OpaquePointer?, Float) -> Void
typealias FnDspProcess = @convention(c) (OpaquePointer?, UnsafePointer<Int16>?, Int, UnsafeMutablePointer<Int>?) -> UnsafeMutablePointer<Int16>?

typealias FnMetaFn = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?

typealias FnPlayerNew = @convention(c) () -> OpaquePointer?
typealias FnPlayerNewCfg = @convention(c) (UInt32, Float, Float, Int32, Float, Bool, Int32, UInt32, UInt32, UInt32, UInt32, Int32) -> OpaquePointer?
typealias FnPlayerFree = @convention(c) (OpaquePointer?) -> Void
typealias FnPlayerStr = @convention(c) (OpaquePointer?, UnsafePointer<CChar>?) -> Void
typealias FnPlayerVoid = @convention(c) (OpaquePointer?) -> Void
typealias FnPlayerSize = @convention(c) (OpaquePointer?, Int) -> Void
typealias FnPlayerU64 = @convention(c) (OpaquePointer?, UInt64) -> Void
typealias FnPlayerF = @convention(c) (OpaquePointer?, Float) -> Void
typealias FnPlayerXfade = @convention(c) (OpaquePointer?, Int32, UInt32, UInt32, UInt32, UInt32, Int32) -> Void
typealias FnPlayerRg = @convention(c) (OpaquePointer?, Int32, Float, Bool) -> Void
typealias FnPlayerVolume = @convention(c) (OpaquePointer?) -> Float
typealias FnPlayerRate = @convention(c) (OpaquePointer?) -> UInt32
typealias FnPlayerStatus = @convention(c) (OpaquePointer?) -> UnsafeMutablePointer<CChar>?

typealias FnPlayerNewCfgEx = @convention(c) (UInt32, Float, Float, Int32, Float, Bool, Int32, UInt32, UInt32, UInt32, UInt32, Int32, UnsafePointer<CChar>?, UInt32) -> OpaquePointer?
typealias FnPlayerInsertJson = @convention(c) (OpaquePointer?, UnsafePointer<CChar>?, Int32, Int) -> Void
typealias FnPlayerQueueJson = @convention(c) (OpaquePointer?) -> UnsafeMutablePointer<CChar>?
typealias FnPlayerResume = @convention(c) (OpaquePointer?) -> UnsafeMutablePointer<CChar>?
typealias FnPlayerSaveResume = @convention(c) (OpaquePointer?) -> Void
typealias FnPlayerClearResume = @convention(c) (OpaquePointer?) -> Void
typealias FnLoadResumeJson = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
typealias FnPlayerImportM3u = @convention(c) (OpaquePointer?, UnsafePointer<CChar>?, Int32, Int) -> UnsafeMutablePointer<CChar>?
typealias FnPlayerLoadM3u = @convention(c) (OpaquePointer?, UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
typealias FnPlayerExportM3u = @convention(c) (OpaquePointer?, UnsafePointer<CChar>?) -> Int32
typealias FnM3uReadJson = @convention(c) (UnsafePointer<CChar>?) -> UnsafeMutablePointer<CChar>?
typealias FnM3uWriteJson = @convention(c) (UnsafePointer<CChar>?, UnsafePointer<CChar>?) -> Int32
typealias FnIsUrl = @convention(c) (UnsafePointer<CChar>?) -> Bool

// player DSP
typealias FnPlayerBool = @convention(c) (OpaquePointer?, Bool) -> Void
typealias FnPlayerIsBool = @convention(c) (OpaquePointer?) -> Bool
typealias FnPlayerEqBand = @convention(c) (OpaquePointer?, Int, Int32, Float, Float) -> Void
typealias FnPlayerF1 = @convention(c) (OpaquePointer?, Float) -> Void
typealias FnPlayerI1 = @convention(c) (OpaquePointer?, Int32) -> Void
typealias FnPlayerI4 = @convention(c) (OpaquePointer?, Int32, Int32, Int32, Int32) -> Void
typealias FnPlayerI6 = @convention(c) (OpaquePointer?, Int32, Int32, Int32, Int32, Int32, Int32) -> Void
typealias FnPlayerDspJson = @convention(c) (OpaquePointer?) -> UnsafeMutablePointer<CChar>?

/// Loaded once, shared process-wide. A missing native library is a fatal
/// setup error (matching how the other bindings abort on load failure).
final class Lib {
    static let shared: Lib = {
        do { return try Lib() }
        catch { fatalError("\(error)") }
    }()

    // nil when the symbols are statically linked into the process image
    // (resolved through RTLD_DEFAULT); otherwise the dlopen'd library handle.
    private let handle: UnsafeMutableRawPointer?

    // Deallocators
    let abiVersion: FnAbiVersion
    let stringFree: FnStringFree
    let bufferFree: FnBufferFree

    // DSP
    let dspNew: FnDspNew
    let dspFree: FnDspFree
    let dspSetInputFrequency: FnDspSetU32
    let dspFlush: FnDspVoid
    let dspEqEnable: FnDspBool
    let dspSetTone: FnDspI2
    let dspSetToneCutoffs: FnDspI2
    let dspSetSurround: FnDspI4
    let dspSetChannelConfig: FnDspI1
    let dspSetStereoWidth: FnDspI1
    let dspSetCompressor: FnDspI6
    let dspSetReplaygain: FnDspRg
    let dspSetReplaygainGains: FnDspRgGains
    let dspSetReplaygainGainsRaw: FnDspRgGainsRaw
    let dspSetEqBand: FnDspEqBand
    let dspSetEqPrecut: FnDspEqPrecut
    let dspProcess: FnDspProcess

    // metadata
    let metaReadJson: FnMetaFn
    let metaProbe: FnMetaFn

    // player
    let playerNew: FnPlayerNew
    let playerNewWithConfig: FnPlayerNewCfg
    let playerFree: FnPlayerFree
    let playerSetQueueJson: FnPlayerStr
    let playerEnqueue: FnPlayerStr
    let playerPlay: FnPlayerVoid
    let playerPause: FnPlayerVoid
    let playerToggle: FnPlayerVoid
    let playerStop: FnPlayerVoid
    let playerNext: FnPlayerVoid
    let playerPrevious: FnPlayerVoid
    let playerSkipTo: FnPlayerSize
    let playerSeekMs: FnPlayerU64
    let playerSetVolume: FnPlayerF
    let playerSetCrossfade: FnPlayerXfade
    let playerSetReplaygain: FnPlayerRg
    let playerVolume: FnPlayerVolume
    let playerSampleRate: FnPlayerRate
    let playerStatusJson: FnPlayerStatus
    let playerNewWithConfigEx: FnPlayerNewCfgEx
    let playerInsertJson: FnPlayerInsertJson
    let playerQueueJson: FnPlayerQueueJson
    let playerResume: FnPlayerResume
    let playerSaveResume: FnPlayerSaveResume
    let playerClearResume: FnPlayerClearResume
    let loadResumeJson: FnLoadResumeJson
    let playerImportM3u: FnPlayerImportM3u
    let playerLoadM3u: FnPlayerLoadM3u
    let playerExportM3u: FnPlayerExportM3u
    let m3uReadJson: FnM3uReadJson
    let m3uWriteJson: FnM3uWriteJson
    let isUrl: FnIsUrl

    // player DSP
    let playerSetEqEnabled: FnPlayerBool
    let playerIsEqEnabled: FnPlayerIsBool
    let playerSetEqBand: FnPlayerEqBand
    let playerSetEqPrecut: FnPlayerF1
    let playerSetEqPreset: FnPlayerI1
    let playerSetTone: FnPlayerI4
    let playerSetBass: FnPlayerI1
    let playerSetTreble: FnPlayerI1
    let playerSetSurround: FnPlayerI4
    let playerSetChannelMode: FnPlayerI1
    let playerSetStereoWidth: FnPlayerI1
    let playerSetCompressor: FnPlayerI6
    let playerSetDither: FnPlayerBool
    let playerSetPitch: FnPlayerI1
    let playerDspSettingsJson: FnPlayerDspJson

    private init() throws {
        let (h, _) = try Lib.open()
        handle = h

        // Capture `h` locally so the loader doesn't touch `self` before every
        // stored property is initialized (Swift definite-initialization).
        // `h == nil` means the symbols are statically linked in — look them up
        // through RTLD_DEFAULT (the whole-process image) instead of a handle.
        let lookup = h ?? RTLD_DEFAULT
        func sym<T>(_ name: String, _ type: T.Type) throws -> T {
            guard let s = dlsym(lookup, name) else { throw RockboxError.symbolNotFound(name) }
            return unsafeBitCast(s, to: T.self)
        }

        abiVersion = try sym("rb_ffi_abi_version", FnAbiVersion.self)
        stringFree = try sym("rb_string_free", FnStringFree.self)
        bufferFree = try sym("rb_buffer_free", FnBufferFree.self)

        dspNew = try sym("rb_dsp_new", FnDspNew.self)
        dspFree = try sym("rb_dsp_free", FnDspFree.self)
        dspSetInputFrequency = try sym("rb_dsp_set_input_frequency", FnDspSetU32.self)
        dspFlush = try sym("rb_dsp_flush", FnDspVoid.self)
        dspEqEnable = try sym("rb_dsp_eq_enable", FnDspBool.self)
        dspSetTone = try sym("rb_dsp_set_tone", FnDspI2.self)
        dspSetToneCutoffs = try sym("rb_dsp_set_tone_cutoffs", FnDspI2.self)
        dspSetSurround = try sym("rb_dsp_set_surround", FnDspI4.self)
        dspSetChannelConfig = try sym("rb_dsp_set_channel_config", FnDspI1.self)
        dspSetStereoWidth = try sym("rb_dsp_set_stereo_width", FnDspI1.self)
        dspSetCompressor = try sym("rb_dsp_set_compressor", FnDspI6.self)
        dspSetReplaygain = try sym("rb_dsp_set_replaygain", FnDspRg.self)
        dspSetReplaygainGains = try sym("rb_dsp_set_replaygain_gains", FnDspRgGains.self)
        dspSetReplaygainGainsRaw = try sym("rb_dsp_set_replaygain_gains_raw", FnDspRgGainsRaw.self)
        dspSetEqBand = try sym("rb_dsp_set_eq_band", FnDspEqBand.self)
        dspSetEqPrecut = try sym("rb_dsp_set_eq_precut", FnDspEqPrecut.self)
        dspProcess = try sym("rb_dsp_process", FnDspProcess.self)

        metaReadJson = try sym("rb_meta_read_json", FnMetaFn.self)
        metaProbe = try sym("rb_meta_probe", FnMetaFn.self)

        playerNew = try sym("rb_player_new", FnPlayerNew.self)
        playerNewWithConfig = try sym("rb_player_new_with_config", FnPlayerNewCfg.self)
        playerFree = try sym("rb_player_free", FnPlayerFree.self)
        playerSetQueueJson = try sym("rb_player_set_queue_json", FnPlayerStr.self)
        playerEnqueue = try sym("rb_player_enqueue", FnPlayerStr.self)
        playerPlay = try sym("rb_player_play", FnPlayerVoid.self)
        playerPause = try sym("rb_player_pause", FnPlayerVoid.self)
        playerToggle = try sym("rb_player_toggle", FnPlayerVoid.self)
        playerStop = try sym("rb_player_stop", FnPlayerVoid.self)
        playerNext = try sym("rb_player_next", FnPlayerVoid.self)
        playerPrevious = try sym("rb_player_previous", FnPlayerVoid.self)
        playerSkipTo = try sym("rb_player_skip_to", FnPlayerSize.self)
        playerSeekMs = try sym("rb_player_seek_ms", FnPlayerU64.self)
        playerSetVolume = try sym("rb_player_set_volume", FnPlayerF.self)
        playerSetCrossfade = try sym("rb_player_set_crossfade", FnPlayerXfade.self)
        playerSetReplaygain = try sym("rb_player_set_replaygain", FnPlayerRg.self)
        playerVolume = try sym("rb_player_volume", FnPlayerVolume.self)
        playerSampleRate = try sym("rb_player_sample_rate", FnPlayerRate.self)
        playerStatusJson = try sym("rb_player_status_json", FnPlayerStatus.self)
        playerNewWithConfigEx = try sym("rb_player_new_with_config_ex", FnPlayerNewCfgEx.self)
        playerInsertJson = try sym("rb_player_insert_json", FnPlayerInsertJson.self)
        playerQueueJson = try sym("rb_player_queue_json", FnPlayerQueueJson.self)
        playerResume = try sym("rb_player_resume", FnPlayerResume.self)
        playerSaveResume = try sym("rb_player_save_resume", FnPlayerSaveResume.self)
        playerClearResume = try sym("rb_player_clear_resume", FnPlayerClearResume.self)
        loadResumeJson = try sym("rb_load_resume_json", FnLoadResumeJson.self)
        playerImportM3u = try sym("rb_player_import_m3u", FnPlayerImportM3u.self)
        playerLoadM3u = try sym("rb_player_load_m3u", FnPlayerLoadM3u.self)
        playerExportM3u = try sym("rb_player_export_m3u", FnPlayerExportM3u.self)
        m3uReadJson = try sym("rb_m3u_read_json", FnM3uReadJson.self)
        m3uWriteJson = try sym("rb_m3u_write_json", FnM3uWriteJson.self)
        isUrl = try sym("rb_is_url", FnIsUrl.self)

        playerSetEqEnabled = try sym("rb_player_set_eq_enabled", FnPlayerBool.self)
        playerIsEqEnabled = try sym("rb_player_is_eq_enabled", FnPlayerIsBool.self)
        playerSetEqBand = try sym("rb_player_set_eq_band", FnPlayerEqBand.self)
        playerSetEqPrecut = try sym("rb_player_set_eq_precut", FnPlayerF1.self)
        playerSetEqPreset = try sym("rb_player_set_eq_preset", FnPlayerI1.self)
        playerSetTone = try sym("rb_player_set_tone", FnPlayerI4.self)
        playerSetBass = try sym("rb_player_set_bass", FnPlayerI1.self)
        playerSetTreble = try sym("rb_player_set_treble", FnPlayerI1.self)
        playerSetSurround = try sym("rb_player_set_surround", FnPlayerI4.self)
        playerSetChannelMode = try sym("rb_player_set_channel_mode", FnPlayerI1.self)
        playerSetStereoWidth = try sym("rb_player_set_stereo_width", FnPlayerI1.self)
        playerSetCompressor = try sym("rb_player_set_compressor", FnPlayerI6.self)
        playerSetDither = try sym("rb_player_set_dither", FnPlayerBool.self)
        playerSetPitch = try sym("rb_player_set_pitch", FnPlayerI1.self)
        playerDspSettingsJson = try sym("rb_player_dsp_settings_json", FnPlayerDspJson.self)
    }

    /// Copy a heap C string returned by the ABI into a String, then free it.
    /// Returns nil for a NULL pointer (the ABI's error/absent signal).
    func takeString(_ ptr: UnsafeMutablePointer<CChar>?) -> String? {
        guard let ptr = ptr else { return nil }
        defer { stringFree(ptr) }
        return String(cString: ptr)
    }

    // MARK: - library location

    /// Resolve the native library. Precedence:
    ///   0. already in the process image (statically linked) → nil handle
    ///   1. ROCKBOX_FFI_LIB env var (explicit override)
    ///   2. bundled next to the package (published distributions)
    ///   3. target/release, walking up from this source file / cwd
    private static func open() throws -> (UnsafeMutableRawPointer?, String) {
        // 0. Static link: the RockboxFFI product force_loads librockbox_ffi.a,
        //    so the symbols are already resolvable. Probe one; if present,
        //    skip all file lookup and resolve everything via RTLD_DEFAULT.
        if dlsym(RTLD_DEFAULT, "rb_ffi_abi_version") != nil {
            return (nil, "<statically linked>")
        }

        var tried: [String] = []
        let names = ["librockbox_ffi.dylib", "librockbox_ffi.so"]

        func attempt(_ path: String) -> UnsafeMutableRawPointer? {
            tried.append(path)
            guard FileManager.default.fileExists(atPath: path) else { return nil }
            return dlopen(path, RTLD_NOW)
        }

        if let env = ProcessInfo.processInfo.environment["ROCKBOX_FFI_LIB"] {
            if let h = attempt(env) { return (h, env) }
        }

        var dirs: [String] = []
        // Walk up from this source file (repo checkout) and from the cwd.
        for base in [URL(fileURLWithPath: #filePath).deletingLastPathComponent().path,
                     FileManager.default.currentDirectoryPath] {
            var dir = URL(fileURLWithPath: base)
            while true {
                dirs.append(dir.appendingPathComponent("target/release").path)
                dirs.append(dir.appendingPathComponent("Libs").path) // bundled
                let parent = dir.deletingLastPathComponent()
                if parent.path == dir.path { break }
                dir = parent
            }
        }
        for d in dirs {
            for name in names {
                if let h = attempt("\(d)/\(name)") { return (h, "\(d)/\(name)") }
            }
        }

        throw RockboxError.libraryNotFound(tried: tried)
    }
}
