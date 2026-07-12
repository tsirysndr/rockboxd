package rockbox

// Note the two *different* ReplayGain encodings in the C ABI.

// DspReplayGainMode selects the ReplayGain mode for [Dsp.SetReplaygain]
// (native Rockbox encoding).
type DspReplayGainMode int32

const (
	DspReplayGainTrack   DspReplayGainMode = 0
	DspReplayGainAlbum   DspReplayGainMode = 1
	DspReplayGainShuffle DspReplayGainMode = 2
	DspReplayGainOff     DspReplayGainMode = 3
)

// ReplayGainMode selects the ReplayGain mode for [Player.SetReplaygain] and
// [Config.ReplayGainMode] (player encoding).
type ReplayGainMode int32

const (
	ReplayGainOff   ReplayGainMode = 0
	ReplayGainTrack ReplayGainMode = 1
	ReplayGainAlbum ReplayGainMode = 2
)

// RepeatMode selects the repeat behaviour for [Player.SetRepeat] and
// [Player.Repeat].
type RepeatMode int32

const (
	RepeatOff RepeatMode = 0
	RepeatOne RepeatMode = 1
	RepeatAll RepeatMode = 2
)

// CrossfadeMode selects the crossfade behaviour for [Player.SetCrossfade] and
// [Config.CrossfadeMode].
type CrossfadeMode int32

const (
	CrossfadeOff             CrossfadeMode = 0
	CrossfadeAutoSkip        CrossfadeMode = 1
	CrossfadeManualSkip      CrossfadeMode = 2
	CrossfadeShuffle         CrossfadeMode = 3
	CrossfadeShuffleOrManual CrossfadeMode = 4
	CrossfadeAlways          CrossfadeMode = 5
)

// MixMode selects how crossfading tracks are combined.
type MixMode int32

const (
	MixCrossfade MixMode = 0
	MixMix       MixMode = 1
)

// InsertPosition selects where [Player.Insert] / [Player.ImportM3u] place the
// new tracks in the queue.
type InsertPosition int32

const (
	InsertPrepend      InsertPosition = 0 // before the current track
	InsertAppend       InsertPosition = 1 // at the very end
	InsertNext         InsertPosition = 2 // right after the current track
	InsertLast         InsertPosition = 3 // at the end (last)
	InsertShuffled     InsertPosition = 4 // random slot
	InsertLastShuffled InsertPosition = 5 // shuffled but after current
	InsertReplace      InsertPosition = 6 // clear then insert
	InsertAtIndex      InsertPosition = 7 // at the explicit index argument
)

// ChannelConfig selects the DSP channel routing for [Dsp.SetChannelConfig].
type ChannelConfig int32

const (
	ChannelStereo    ChannelConfig = 0
	ChannelMono      ChannelConfig = 1
	ChannelCustom    ChannelConfig = 2
	ChannelMonoLeft  ChannelConfig = 3
	ChannelMonoRight ChannelConfig = 4
	ChannelKaraoke   ChannelConfig = 5
	ChannelSwap      ChannelConfig = 6
)

// EqPreset selects a built-in equalizer preset for [Player.SetEqPreset].
type EqPreset int32

const (
	EqPresetFlat          EqPreset = 0
	EqPresetAcoustic      EqPreset = 1
	EqPresetBassBoost     EqPreset = 2
	EqPresetBassReducer   EqPreset = 3
	EqPresetClassical     EqPreset = 4
	EqPresetDance         EqPreset = 5
	EqPresetDeep          EqPreset = 6
	EqPresetElectronic    EqPreset = 7
	EqPresetHipHop        EqPreset = 8
	EqPresetJazz          EqPreset = 9
	EqPresetLatin         EqPreset = 10
	EqPresetLoudness      EqPreset = 11
	EqPresetLounge        EqPreset = 12
	EqPresetPiano         EqPreset = 13
	EqPresetPop           EqPreset = 14
	EqPresetRnB           EqPreset = 15
	EqPresetRock          EqPreset = 16
	EqPresetSmallSpeakers EqPreset = 17
	EqPresetTrebleBoost   EqPreset = 18
	EqPresetTrebleReducer EqPreset = 19
	EqPresetVocalBoost    EqPreset = 20
)

// ChannelMode selects the channel routing for [Player.SetChannelMode].
type ChannelMode int32

const (
	ChannelModeStereo    ChannelMode = 0
	ChannelModeMono      ChannelMode = 1
	ChannelModeCustom    ChannelMode = 2
	ChannelModeMonoLeft  ChannelMode = 3
	ChannelModeMonoRight ChannelMode = 4
	ChannelModeKaraoke   ChannelMode = 5
	ChannelModeSwap      ChannelMode = 6
)
