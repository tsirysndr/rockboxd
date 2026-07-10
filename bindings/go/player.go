package rockbox

import (
	"encoding/json"
	"errors"
	"fmt"
)

// Config holds the tunables for [NewPlayer]. Use [DefaultConfig] and override
// the fields you care about. SampleRate 0 means the output device default.
type Config struct {
	SampleRate                uint32
	BufferSeconds             float32
	Volume                    float32
	ReplayGainMode            ReplayGainMode
	ReplayGainPreampDb        float32
	ReplayGainPreventClipping bool
	CrossfadeMode             CrossfadeMode
	FadeOutDelayMs            uint32
	FadeOutDurationMs         uint32
	FadeInDelayMs             uint32
	FadeInDurationMs          uint32
	MixMode                   MixMode
}

// DefaultConfig returns the Rockbox default player configuration.
func DefaultConfig() Config {
	return Config{
		SampleRate:                0, // device default
		BufferSeconds:             4.0,
		Volume:                    1.0,
		ReplayGainMode:            ReplayGainOff,
		ReplayGainPreampDb:        0.0,
		ReplayGainPreventClipping: true,
		CrossfadeMode:             CrossfadeOff,
		FadeOutDelayMs:            0,
		FadeOutDurationMs:         2000,
		FadeInDelayMs:             0,
		FadeInDurationMs:          2000,
		MixMode:                   MixCrossfade,
	}
}

// Player is a queue-based audio player with native ReplayGain and Rockbox
// crossfade.
//
// A Player owns a live audio output device and a background engine thread —
// construct it only where an output device exists. Call [Player.Close] when
// done.
//
// The ReplayGain mode here uses the *player* encoding (see [ReplayGainMode],
// Off=0, Track=1, Album=2) — distinct from the DSP encoding.
type Player struct {
	ptr uintptr
}

// NewPlayer creates a player on the default device with the given
// configuration (start from [DefaultConfig]).
func NewPlayer(c Config) (*Player, error) {
	ptr := rbPlayerNewWithConfig(
		c.SampleRate, c.BufferSeconds, c.Volume, int32(c.ReplayGainMode),
		c.ReplayGainPreampDb, c.ReplayGainPreventClipping, int32(c.CrossfadeMode),
		c.FadeOutDelayMs, c.FadeOutDurationMs, c.FadeInDelayMs, c.FadeInDurationMs,
		int32(c.MixMode),
	)
	if ptr == 0 {
		return nil, errors.New("rockbox: failed to create Player (no output device?)")
	}
	return &Player{ptr: ptr}, nil
}

// NewDefaultPlayer creates a player on the default device with Rockbox default
// settings.
func NewDefaultPlayer() (*Player, error) {
	ptr := rbPlayerNew()
	if ptr == 0 {
		return nil, errors.New("rockbox: failed to create Player (no output device?)")
	}
	return &Player{ptr: ptr}, nil
}

// Close frees the native player and stops its engine thread. Safe to call more
// than once.
func (p *Player) Close() {
	if p.ptr == 0 {
		return
	}
	rbPlayerFree(p.ptr)
	p.ptr = 0
}

// SetQueue replaces the queue with the given file paths.
func (p *Player) SetQueue(paths []string) error {
	b, err := json.Marshal(paths)
	if err != nil {
		return fmt.Errorf("rockbox: marshaling queue: %w", err)
	}
	rbPlayerSetQueueJSON(p.ptr, string(b))
	return nil
}

// Enqueue appends a single file path to the queue.
func (p *Player) Enqueue(path string) { rbPlayerEnqueue(p.ptr, path) }

// Play starts (or resumes) playback.
func (p *Player) Play() { rbPlayerPlay(p.ptr) }

// Pause pauses playback.
func (p *Player) Pause() { rbPlayerPause(p.ptr) }

// Toggle flips between playing and paused.
func (p *Player) Toggle() { rbPlayerToggle(p.ptr) }

// Stop stops playback and resets the position.
func (p *Player) Stop() { rbPlayerStop(p.ptr) }

// Next skips to the next track.
func (p *Player) Next() { rbPlayerNext(p.ptr) }

// Previous skips to the previous track.
func (p *Player) Previous() { rbPlayerPrevious(p.ptr) }

// SkipTo jumps to the queue entry at index.
func (p *Player) SkipTo(index int) { rbPlayerSkipTo(p.ptr, uint64(index)) }

// SeekMs seeks within the current track to ms milliseconds.
func (p *Player) SeekMs(ms uint64) { rbPlayerSeekMs(p.ptr, ms) }

// SetVolume sets the linear output volume (0.0..=1.0).
func (p *Player) SetVolume(vol float32) { rbPlayerSetVolume(p.ptr, vol) }

// Volume reports the current linear output volume.
func (p *Player) Volume() float32 { return rbPlayerVolume(p.ptr) }

// SampleRate reports the output device sample rate (Hz).
func (p *Player) SampleRate() uint32 { return rbPlayerSampleRate(p.ptr) }

// SetCrossfade configures crossfading (see [CrossfadeMode], [MixMode]).
func (p *Player) SetCrossfade(mode CrossfadeMode, fadeOutDelayMs, fadeOutDurationMs, fadeInDelayMs, fadeInDurationMs uint32, mix MixMode) {
	rbPlayerSetCrossfade(p.ptr, int32(mode), fadeOutDelayMs, fadeOutDurationMs, fadeInDelayMs, fadeInDurationMs, int32(mix))
}

// SetReplaygain sets the ReplayGain mode (see [ReplayGainMode], Off=0, Track=1,
// Album=2), pre-amp in dB, and clipping prevention.
func (p *Player) SetReplaygain(mode ReplayGainMode, preampDb float32, preventClipping bool) {
	rbPlayerSetReplaygain(p.ptr, int32(mode), preampDb, preventClipping)
}

// Status is a snapshot of the player's playback state.
type Status struct {
	State      string `json:"state"` // "stopped" | "playing" | "paused"
	Index      *int   `json:"index"`
	PositionMs uint64 `json:"position_ms"`
	DurationMs uint64 `json:"duration_ms"`
	QueueLen   int    `json:"queue_len"`
	Metadata   *Meta  `json:"metadata"`
}

// Status returns a snapshot of the player's status.
func (p *Player) Status() (*Status, error) {
	s, ok := takeString(rbPlayerStatusJSON(p.ptr))
	if !ok {
		return nil, errors.New("rockbox: rb_player_status_json returned NULL")
	}
	var st Status
	if err := json.Unmarshal([]byte(s), &st); err != nil {
		return nil, fmt.Errorf("rockbox: decoding status json: %w", err)
	}
	return &st, nil
}
