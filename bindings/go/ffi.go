// Package rockbox provides Go bindings for the Rockbox DSP / metadata /
// playback engine.
//
// It sits on top of the flat C ABI librockbox_ffi (crates/rockbox-ffi, header
// include/rockbox_ffi.h) and loads the prebuilt shared library at runtime with
// purego — no cgo and no C toolchain required. The library is located via the
// ROCKBOX_FFI_LIB env var, a bundled lib/ directory, or by walking up to the
// repo's target/release directory (see libraryPath).
//
// Three surfaces mirror the other language bindings:
//
//   - [Metadata.Read] / [Metadata.Probe] — audio file tags & codec probing.
//   - [Dsp]    — EQ / tone / surround / compressor / ReplayGain over int16 PCM.
//   - [Player] — queue + transport with crossfade and ReplayGain.
package rockbox

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"unsafe"

	"github.com/ebitengine/purego"
)

// Raw C ABI bindings. Each variable is bound to a symbol in librockbox_ffi by
// purego.RegisterLibFunc in load(). Opaque handles (RbDsp*, RbPlayer*) and
// heap char*/int16* returns cross as uintptr; scalars as their fixed-width Go
// type. Keep in sync with include/rockbox_ffi.h.
var (
	rbFFIABIVersion func() uint32

	rbStringFree func(uintptr)
	rbBufferFree func(uintptr, uint64)

	// ---- DSP ----------------------------------------------------------
	rbDspNew                   func(uint32) uintptr
	rbDspFree                  func(uintptr)
	rbDspSetInputFrequency     func(uintptr, uint32)
	rbDspFlush                 func(uintptr)
	rbDspEqEnable              func(uintptr, bool)
	rbDspSetTone               func(uintptr, int32, int32)
	rbDspSetToneCutoffs        func(uintptr, int32, int32)
	rbDspSetSurround           func(uintptr, int32, int32, int32, int32)
	rbDspSetChannelConfig      func(uintptr, int32)
	rbDspSetStereoWidth        func(uintptr, int32)
	rbDspSetCompressor         func(uintptr, int32, int32, int32, int32, int32, int32)
	rbDspSetReplaygain         func(uintptr, int32, bool, float32)
	rbDspSetReplaygainGains    func(uintptr, float32, float32, float32, float32)
	rbDspSetReplaygainGainsRaw func(uintptr, int64, int64, int64, int64)
	rbDspSetEqBand             func(uintptr, uint64, int32, float32, float32)
	rbDspSetEqPrecut           func(uintptr, float32)
	rbDspProcess               func(uintptr, unsafe.Pointer, uint64, unsafe.Pointer) uintptr

	// ---- metadata -----------------------------------------------------
	rbMetaReadJSON func(string) uintptr
	rbMetaProbe    func(string) uintptr

	// ---- player -------------------------------------------------------
	rbPlayerNew             func() uintptr
	rbPlayerNewWithConfig   func(uint32, float32, float32, int32, float32, bool, int32, uint32, uint32, uint32, uint32, int32) uintptr
	rbPlayerNewWithConfigEx func(uint32, float32, float32, int32, float32, bool, int32, uint32, uint32, uint32, uint32, int32, string, uint32) uintptr
	rbPlayerFree            func(uintptr)
	rbPlayerSetQueueJSON    func(uintptr, string)
	rbPlayerEnqueue         func(uintptr, string)
	rbPlayerInsertJSON      func(uintptr, string, int32, uint64)
	rbPlayerQueueJSON       func(uintptr) uintptr
	rbPlayerPlay            func(uintptr)
	rbPlayerPause           func(uintptr)
	rbPlayerToggle          func(uintptr)
	rbPlayerStop            func(uintptr)
	rbPlayerNext            func(uintptr)
	rbPlayerPrevious        func(uintptr)
	rbPlayerSkipTo          func(uintptr, uint64)
	rbPlayerSeekMs          func(uintptr, uint64)
	rbPlayerSetVolume       func(uintptr, float32)
	rbPlayerSetCrossfade    func(uintptr, int32, uint32, uint32, uint32, uint32, int32)
	rbPlayerSetReplaygain   func(uintptr, int32, float32, bool)
	rbPlayerVolume          func(uintptr) float32
	rbPlayerSampleRate      func(uintptr) uint32
	rbPlayerStatusJSON      func(uintptr) uintptr

	rbPlayerSetShuffle       func(uintptr, bool)
	rbPlayerIsShuffleEnabled func(uintptr) bool
	rbPlayerSetRepeat        func(uintptr, int32)
	rbPlayerRepeat           func(uintptr) int32

	// ---- player DSP ---------------------------------------------------
	rbPlayerSetEqEnabled        func(uintptr, bool)
	rbPlayerIsEqEnabled         func(uintptr) bool
	rbPlayerSetEqBand           func(uintptr, uint64, int32, float32, float32)
	rbPlayerSetEqPrecut         func(uintptr, float32)
	rbPlayerSetEqPreset         func(uintptr, int32)
	rbPlayerSetTone             func(uintptr, int32, int32, int32, int32)
	rbPlayerSetBass             func(uintptr, int32)
	rbPlayerSetTreble           func(uintptr, int32)
	rbPlayerSetSurround         func(uintptr, int32, int32, int32, int32)
	rbPlayerSetChannelMode      func(uintptr, int32)
	rbPlayerSetStereoWidth      func(uintptr, int32)
	rbPlayerSetCompressor       func(uintptr, int32, int32, int32, int32, int32, int32)
	rbPlayerSetDither           func(uintptr, bool)
	rbPlayerSetPitch            func(uintptr, int32)
	rbPlayerSetBassCutoff       func(uintptr, int32)
	rbPlayerSetTrebleCutoff     func(uintptr, int32)
	rbPlayerSetCrossfeed        func(uintptr, int32, int32, int32, int32, int32)
	rbPlayerSetBassEnhancement  func(uintptr, int32, int32)
	rbPlayerSetFatigueReduction func(uintptr, int32)
	rbPlayerDspSettingsJSON     func(uintptr) uintptr

	// ---- resume -------------------------------------------------------
	rbPlayerResume      func(uintptr) uintptr
	rbPlayerSaveResume  func(uintptr)
	rbPlayerClearResume func(uintptr)
	rbLoadResumeJSON    func(string) uintptr

	// ---- m3u / m3u8 ---------------------------------------------------
	rbPlayerImportM3u func(uintptr, string, int32, uint64) uintptr
	rbPlayerLoadM3u   func(uintptr, string) uintptr
	rbPlayerExportM3u func(uintptr, string) int32
	rbM3uReadJSON     func(string) uintptr
	rbM3uWriteJSON    func(string, string) int32
	rbIsURL           func(string) bool
)

func init() {
	handle, err := purego.Dlopen(libraryPath(), purego.RTLD_NOW|purego.RTLD_GLOBAL)
	if err != nil {
		panic(fmt.Sprintf("rockbox: %v", err))
	}

	purego.RegisterLibFunc(&rbFFIABIVersion, handle, "rb_ffi_abi_version")
	purego.RegisterLibFunc(&rbStringFree, handle, "rb_string_free")
	purego.RegisterLibFunc(&rbBufferFree, handle, "rb_buffer_free")

	purego.RegisterLibFunc(&rbDspNew, handle, "rb_dsp_new")
	purego.RegisterLibFunc(&rbDspFree, handle, "rb_dsp_free")
	purego.RegisterLibFunc(&rbDspSetInputFrequency, handle, "rb_dsp_set_input_frequency")
	purego.RegisterLibFunc(&rbDspFlush, handle, "rb_dsp_flush")
	purego.RegisterLibFunc(&rbDspEqEnable, handle, "rb_dsp_eq_enable")
	purego.RegisterLibFunc(&rbDspSetTone, handle, "rb_dsp_set_tone")
	purego.RegisterLibFunc(&rbDspSetToneCutoffs, handle, "rb_dsp_set_tone_cutoffs")
	purego.RegisterLibFunc(&rbDspSetSurround, handle, "rb_dsp_set_surround")
	purego.RegisterLibFunc(&rbDspSetChannelConfig, handle, "rb_dsp_set_channel_config")
	purego.RegisterLibFunc(&rbDspSetStereoWidth, handle, "rb_dsp_set_stereo_width")
	purego.RegisterLibFunc(&rbDspSetCompressor, handle, "rb_dsp_set_compressor")
	purego.RegisterLibFunc(&rbDspSetReplaygain, handle, "rb_dsp_set_replaygain")
	purego.RegisterLibFunc(&rbDspSetReplaygainGains, handle, "rb_dsp_set_replaygain_gains")
	purego.RegisterLibFunc(&rbDspSetReplaygainGainsRaw, handle, "rb_dsp_set_replaygain_gains_raw")
	purego.RegisterLibFunc(&rbDspSetEqBand, handle, "rb_dsp_set_eq_band")
	purego.RegisterLibFunc(&rbDspSetEqPrecut, handle, "rb_dsp_set_eq_precut")
	purego.RegisterLibFunc(&rbDspProcess, handle, "rb_dsp_process")

	purego.RegisterLibFunc(&rbMetaReadJSON, handle, "rb_meta_read_json")
	purego.RegisterLibFunc(&rbMetaProbe, handle, "rb_meta_probe")

	purego.RegisterLibFunc(&rbPlayerNew, handle, "rb_player_new")
	purego.RegisterLibFunc(&rbPlayerNewWithConfig, handle, "rb_player_new_with_config")
	purego.RegisterLibFunc(&rbPlayerNewWithConfigEx, handle, "rb_player_new_with_config_ex")
	purego.RegisterLibFunc(&rbPlayerFree, handle, "rb_player_free")
	purego.RegisterLibFunc(&rbPlayerSetQueueJSON, handle, "rb_player_set_queue_json")
	purego.RegisterLibFunc(&rbPlayerEnqueue, handle, "rb_player_enqueue")
	purego.RegisterLibFunc(&rbPlayerInsertJSON, handle, "rb_player_insert_json")
	purego.RegisterLibFunc(&rbPlayerQueueJSON, handle, "rb_player_queue_json")
	purego.RegisterLibFunc(&rbPlayerPlay, handle, "rb_player_play")
	purego.RegisterLibFunc(&rbPlayerPause, handle, "rb_player_pause")
	purego.RegisterLibFunc(&rbPlayerToggle, handle, "rb_player_toggle")
	purego.RegisterLibFunc(&rbPlayerStop, handle, "rb_player_stop")
	purego.RegisterLibFunc(&rbPlayerNext, handle, "rb_player_next")
	purego.RegisterLibFunc(&rbPlayerPrevious, handle, "rb_player_previous")
	purego.RegisterLibFunc(&rbPlayerSkipTo, handle, "rb_player_skip_to")
	purego.RegisterLibFunc(&rbPlayerSeekMs, handle, "rb_player_seek_ms")
	purego.RegisterLibFunc(&rbPlayerSetVolume, handle, "rb_player_set_volume")
	purego.RegisterLibFunc(&rbPlayerSetCrossfade, handle, "rb_player_set_crossfade")
	purego.RegisterLibFunc(&rbPlayerSetReplaygain, handle, "rb_player_set_replaygain")
	purego.RegisterLibFunc(&rbPlayerVolume, handle, "rb_player_volume")
	purego.RegisterLibFunc(&rbPlayerSampleRate, handle, "rb_player_sample_rate")
	purego.RegisterLibFunc(&rbPlayerStatusJSON, handle, "rb_player_status_json")

	purego.RegisterLibFunc(&rbPlayerSetShuffle, handle, "rb_player_set_shuffle")
	purego.RegisterLibFunc(&rbPlayerIsShuffleEnabled, handle, "rb_player_is_shuffle_enabled")
	purego.RegisterLibFunc(&rbPlayerSetRepeat, handle, "rb_player_set_repeat")
	purego.RegisterLibFunc(&rbPlayerRepeat, handle, "rb_player_repeat")

	purego.RegisterLibFunc(&rbPlayerSetEqEnabled, handle, "rb_player_set_eq_enabled")
	purego.RegisterLibFunc(&rbPlayerIsEqEnabled, handle, "rb_player_is_eq_enabled")
	purego.RegisterLibFunc(&rbPlayerSetEqBand, handle, "rb_player_set_eq_band")
	purego.RegisterLibFunc(&rbPlayerSetEqPrecut, handle, "rb_player_set_eq_precut")
	purego.RegisterLibFunc(&rbPlayerSetEqPreset, handle, "rb_player_set_eq_preset")
	purego.RegisterLibFunc(&rbPlayerSetTone, handle, "rb_player_set_tone")
	purego.RegisterLibFunc(&rbPlayerSetBass, handle, "rb_player_set_bass")
	purego.RegisterLibFunc(&rbPlayerSetTreble, handle, "rb_player_set_treble")
	purego.RegisterLibFunc(&rbPlayerSetSurround, handle, "rb_player_set_surround")
	purego.RegisterLibFunc(&rbPlayerSetChannelMode, handle, "rb_player_set_channel_mode")
	purego.RegisterLibFunc(&rbPlayerSetStereoWidth, handle, "rb_player_set_stereo_width")
	purego.RegisterLibFunc(&rbPlayerSetCompressor, handle, "rb_player_set_compressor")
	purego.RegisterLibFunc(&rbPlayerSetDither, handle, "rb_player_set_dither")
	purego.RegisterLibFunc(&rbPlayerSetPitch, handle, "rb_player_set_pitch")
	purego.RegisterLibFunc(&rbPlayerSetBassCutoff, handle, "rb_player_set_bass_cutoff")
	purego.RegisterLibFunc(&rbPlayerSetTrebleCutoff, handle, "rb_player_set_treble_cutoff")
	purego.RegisterLibFunc(&rbPlayerSetCrossfeed, handle, "rb_player_set_crossfeed")
	purego.RegisterLibFunc(&rbPlayerSetBassEnhancement, handle, "rb_player_set_bass_enhancement")
	purego.RegisterLibFunc(&rbPlayerSetFatigueReduction, handle, "rb_player_set_fatigue_reduction")
	purego.RegisterLibFunc(&rbPlayerDspSettingsJSON, handle, "rb_player_dsp_settings_json")

	purego.RegisterLibFunc(&rbPlayerResume, handle, "rb_player_resume")
	purego.RegisterLibFunc(&rbPlayerSaveResume, handle, "rb_player_save_resume")
	purego.RegisterLibFunc(&rbPlayerClearResume, handle, "rb_player_clear_resume")
	purego.RegisterLibFunc(&rbLoadResumeJSON, handle, "rb_load_resume_json")

	purego.RegisterLibFunc(&rbPlayerImportM3u, handle, "rb_player_import_m3u")
	purego.RegisterLibFunc(&rbPlayerLoadM3u, handle, "rb_player_load_m3u")
	purego.RegisterLibFunc(&rbPlayerExportM3u, handle, "rb_player_export_m3u")
	purego.RegisterLibFunc(&rbM3uReadJSON, handle, "rb_m3u_read_json")
	purego.RegisterLibFunc(&rbM3uWriteJSON, handle, "rb_m3u_write_json")
	purego.RegisterLibFunc(&rbIsURL, handle, "rb_is_url")
}

// libNames returns the platform's shared-library filenames, most-specific
// first.
func libNames() []string {
	switch runtime.GOOS {
	case "darwin":
		return []string{"librockbox_ffi.dylib"}
	case "windows":
		return []string{"rockbox_ffi.dll"}
	default:
		return []string{"librockbox_ffi.so"}
	}
}

// libraryPath locates the prebuilt shared library. Precedence:
//
//  1. ROCKBOX_FFI_LIB env var (explicit override).
//  2. a lib/ dir next to this source file (bundled in a checkout / release).
//  3. target/release, by walking up from this source file (repo checkout).
//  4. target/release, by walking up from the executable and the cwd.
//
// It returns the first existing candidate, or panics with the list it tried.
func libraryPath() string {
	names := libNames()
	var tried []string

	try := func(cand string) (string, bool) {
		tried = append(tried, cand)
		if _, err := os.Stat(cand); err == nil {
			return cand, true
		}
		return "", false
	}

	// 1. Explicit override.
	if env := os.Getenv("ROCKBOX_FFI_LIB"); env != "" {
		if p, ok := try(env); ok {
			return p
		}
	}

	// Source-file directory (present when built from a checkout).
	var srcDir string
	if _, file, _, ok := runtime.Caller(0); ok {
		srcDir = filepath.Dir(file)
	}

	// 2. Bundled binary next to the source (lib/<name>).
	if srcDir != "" {
		for _, name := range names {
			if p, ok := try(filepath.Join(srcDir, "lib", name)); ok {
				return p
			}
		}
	}

	// 3 & 4. Walk up from the source dir, the executable dir, and the cwd
	// looking for target/release.
	var roots []string
	if srcDir != "" {
		roots = append(roots, srcDir)
	}
	if exe, err := os.Executable(); err == nil {
		roots = append(roots, filepath.Dir(exe))
	}
	if cwd, err := os.Getwd(); err == nil {
		roots = append(roots, cwd)
	}
	for _, root := range roots {
		for dir := root; ; {
			rel := filepath.Join(dir, "target", "release")
			if info, err := os.Stat(rel); err == nil && info.IsDir() {
				for _, name := range names {
					if p, ok := try(filepath.Join(rel, name)); ok {
						return p
					}
				}
			}
			parent := filepath.Dir(dir)
			if parent == dir {
				break
			}
			dir = parent
		}
	}

	panic(fmt.Sprintf("rockbox: could not locate librockbox_ffi shared "+
		"library. Set ROCKBOX_FFI_LIB or run `cargo build --release -p "+
		"rockbox-ffi`. Tried:\n  %s", filepathListJoin(tried)))
}

func filepathListJoin(paths []string) string {
	out := ""
	for i, p := range paths {
		if i > 0 {
			out += "\n  "
		}
		out += p
	}
	return out
}

// goString copies a NUL-terminated C string at p into a Go string. Returns ""
// for a NULL pointer.
func goString(p uintptr) string {
	if p == 0 {
		return ""
	}
	var n int
	for {
		if *(*byte)(unsafe.Pointer(p + uintptr(n))) == 0 {
			break
		}
		n++
	}
	return string(unsafe.Slice((*byte)(unsafe.Pointer(p)), n))
}

// takeString copies a heap C string returned by the ABI into a Go string, then
// frees it. The bool is false for a NULL pointer (the ABI's error/absent
// signal).
func takeString(p uintptr) (string, bool) {
	if p == 0 {
		return "", false
	}
	s := goString(p)
	rbStringFree(p)
	return s, true
}
