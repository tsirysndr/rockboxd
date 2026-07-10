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
