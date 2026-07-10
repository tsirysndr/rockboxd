package rockbox

import (
	"errors"
	"math"
	"unsafe"
)

// Dsp is the Rockbox DSP pipeline: EQ, tone, surround, compressor, ReplayGain,
// and resampler.
//
// The underlying dsp_config is a process-wide singleton, so only one Dsp may
// exist at a time and it must be used from one goroutine. Call [Dsp.Close] when
// done.
type Dsp struct {
	ptr uintptr
}

// NewDsp creates a Dsp configured for the given output sample rate (Hz).
func NewDsp(sampleRate uint32) (*Dsp, error) {
	ptr := rbDspNew(sampleRate)
	if ptr == 0 {
		return nil, errors.New("rockbox: rb_dsp_new returned NULL")
	}
	return &Dsp{ptr: ptr}, nil
}

// Close frees the native DSP. It is safe to call more than once.
func (d *Dsp) Close() {
	if d.ptr == 0 {
		return
	}
	rbDspFree(d.ptr)
	d.ptr = 0
}

// SetInputFrequency sets the sample rate (Hz) of the audio fed to Process, so
// the resampler can convert it to the output rate.
func (d *Dsp) SetInputFrequency(hz uint32) { rbDspSetInputFrequency(d.ptr, hz) }

// Flush clears the internal resampler / filter state.
func (d *Dsp) Flush() { rbDspFlush(d.ptr) }

// EqEnable turns the parametric EQ on or off.
func (d *Dsp) EqEnable(enable bool) { rbDspEqEnable(d.ptr, enable) }

// SetEqBand configures one EQ band (0..=9; band 0 is the low shelf, 9 the high
// shelf). q is the Q factor and gainDb the gain in dB, both in plain units —
// the binding applies Rockbox's tenths scaling internally.
func (d *Dsp) SetEqBand(band int, cutoffHz int32, q, gainDb float32) {
	rbDspSetEqBand(d.ptr, uint64(band), cutoffHz, q, gainDb)
}

// SetEqPrecut sets the EQ pre-cut in dB.
func (d *Dsp) SetEqPrecut(db float32) { rbDspSetEqPrecut(d.ptr, db) }

// SetTone sets the bass and treble shelves in dB.
func (d *Dsp) SetTone(bassDb, trebleDb int32) { rbDspSetTone(d.ptr, bassDb, trebleDb) }

// SetToneCutoffs sets the bass and treble shelf cutoff frequencies (Hz).
func (d *Dsp) SetToneCutoffs(bassHz, trebleHz int32) { rbDspSetToneCutoffs(d.ptr, bassHz, trebleHz) }

// SetSurround configures the surround effect.
func (d *Dsp) SetSurround(delayMs, balance, fx1, fx2 int32) {
	rbDspSetSurround(d.ptr, delayMs, balance, fx1, fx2)
}

// SetChannelConfig selects the channel routing (see [ChannelConfig]).
func (d *Dsp) SetChannelConfig(mode ChannelConfig) { rbDspSetChannelConfig(d.ptr, int32(mode)) }

// SetStereoWidth sets the stereo width as a percentage.
func (d *Dsp) SetStereoWidth(percent int32) { rbDspSetStereoWidth(d.ptr, percent) }

// SetCompressor configures the dynamic-range compressor.
func (d *Dsp) SetCompressor(threshold, makeupGain, ratio, knee, releaseTime, attackTime int32) {
	rbDspSetCompressor(d.ptr, threshold, makeupGain, ratio, knee, releaseTime, attackTime)
}

// SetReplaygain selects the ReplayGain mode (see [DspReplayGainMode]). When
// noclip is true, gains that would clip are reduced; preampDb is applied on top.
func (d *Dsp) SetReplaygain(mode DspReplayGainMode, noclip bool, preampDb float32) {
	rbDspSetReplaygain(d.ptr, int32(mode), noclip, preampDb)
}

// SetReplaygainGains sets the per-track ReplayGain values: gains in plain dB
// and peaks as linear amplitude (1.0 = full scale). Pass a nil pointer for any
// absent tag (mapped to the ABI's NaN sentinel). Use [Opt] to build pointers.
func (d *Dsp) SetReplaygainGains(trackGainDb, albumGainDb, trackPeak, albumPeak *float32) {
	rbDspSetReplaygainGains(d.ptr, optF32(trackGainDb), optF32(albumGainDb), optF32(trackPeak), optF32(albumPeak))
}

// SetReplaygainGainsRaw sets the native Q7.24 linear factors (the Raw* fields
// of [ReplayGain] from [Metadata.Read]); 0 means "not tagged".
func (d *Dsp) SetReplaygainGainsRaw(trackGain, albumGain, trackPeak, albumPeak int64) {
	rbDspSetReplaygainGainsRaw(d.ptr, trackGain, albumGain, trackPeak, albumPeak)
}

// Process runs interleaved stereo S16 samples through the pipeline and returns
// the processed samples. The output length may differ from the input (the
// resampler buffers). input must have even length (L/R pairs).
func (d *Dsp) Process(input []int16) ([]int16, error) {
	if len(input)%2 != 0 {
		return nil, errors.New("rockbox: input must be interleaved stereo (even length)")
	}
	if len(input) == 0 {
		return nil, nil
	}
	var outLen uint64
	outPtr := rbDspProcess(d.ptr, unsafe.Pointer(&input[0]), uint64(len(input)), unsafe.Pointer(&outLen))
	if outPtr == 0 || outLen == 0 {
		return nil, nil
	}
	src := unsafe.Slice((*int16)(unsafe.Pointer(outPtr)), outLen)
	out := make([]int16, outLen)
	copy(out, src)
	rbBufferFree(outPtr, outLen)
	return out, nil
}

// Opt returns a pointer to v, for the optional *float32 arguments of
// [Dsp.SetReplaygainGains].
func Opt(v float32) *float32 { return &v }

// optF32 maps a nil pointer to NaN (the ABI's "tag absent" sentinel).
func optF32(v *float32) float32 {
	if v == nil {
		return float32(math.NaN())
	}
	return *v
}
