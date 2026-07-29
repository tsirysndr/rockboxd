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

	// Output selects the audio output backend. Empty (the default) uses the
	// local audio device via cpal. Recognized spec strings:
	//
	//	cpal                    local audio device (default; also "" )
	//	stdout | -              raw S16LE stereo PCM on fd 1 (stdout)
	//	fifo:/path              write PCM to a named FIFO
	//	unix:/path              listen on a Unix socket, stream PCM to the client
	//	unix-connect:/path      connect to a Unix socket and stream PCM
	//	tcp:host:port           listen on TCP, stream PCM to the client
	//	tcp-connect:host:port   connect to a TCP endpoint and stream PCM
	//
	// The listening variants (unix:, tcp:) block until a client connects. In
	// stdout mode fd 1 carries the raw PCM stream, so the host process MUST
	// keep stdout clean (no logging/printing to it) — e.g. pipe it to
	// `ffplay -f s16le -ar 44100 -ac 2 -`.
	Output string

	// ResumeFile, when non-empty, is an .m3u8 path the player auto-persists
	// the queue and exact playback position to, so a later Player can
	// [Player.Resume]. Empty disables resume persistence.
	ResumeFile string
	// ResumeSaveIntervalMs is how often the resume file is rewritten while
	// playing. 0 uses the Rockbox default (5 s). Ignored when ResumeFile is
	// empty.
	ResumeSaveIntervalMs uint32
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
	ptr := rbPlayerNewWithOutput(
		c.Output, c.SampleRate, c.BufferSeconds, c.Volume, int32(c.ReplayGainMode),
		c.ReplayGainPreampDb, c.ReplayGainPreventClipping, int32(c.CrossfadeMode),
		c.FadeOutDelayMs, c.FadeOutDurationMs, c.FadeInDelayMs, c.FadeInDurationMs,
		int32(c.MixMode), c.ResumeFile, c.ResumeSaveIntervalMs,
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

// Option mutates a [Config] as part of the functional-options constructor
// [New]. Use the With* helpers to build one.
type Option func(*Config)

// WithSampleRate sets the output sample rate in Hz. 0 (the default) means the
// output device default.
func WithSampleRate(hz uint32) Option {
	return func(c *Config) { c.SampleRate = hz }
}

// WithBufferSeconds sets the audio buffer size in seconds.
func WithBufferSeconds(seconds float32) Option {
	return func(c *Config) { c.BufferSeconds = seconds }
}

// WithVolume sets the initial linear output volume (0.0..=1.0).
func WithVolume(vol float32) Option {
	return func(c *Config) { c.Volume = vol }
}

// WithReplayGain configures ReplayGain: the mode (player encoding — see
// [ReplayGainMode], Off=0, Track=1, Album=2), the pre-amp in dB, and whether
// clipping prevention is applied.
func WithReplayGain(mode ReplayGainMode, preampDb float32, preventClipping bool) Option {
	return func(c *Config) {
		c.ReplayGainMode = mode
		c.ReplayGainPreampDb = preampDb
		c.ReplayGainPreventClipping = preventClipping
	}
}

// WithCrossfade configures crossfading: the mode (see [CrossfadeMode]), the
// fade-out delay/duration and fade-in delay/duration in milliseconds, and the
// mix mode (see [MixMode]).
func WithCrossfade(mode CrossfadeMode, foDelayMs, foDurationMs, fiDelayMs, fiDurationMs uint32, mix MixMode) Option {
	return func(c *Config) {
		c.CrossfadeMode = mode
		c.FadeOutDelayMs = foDelayMs
		c.FadeOutDurationMs = foDurationMs
		c.FadeInDelayMs = fiDelayMs
		c.FadeInDurationMs = fiDurationMs
		c.MixMode = mix
	}
}

// WithOutput selects the audio output backend (see [Config.Output]). Empty
// (the default) uses the local audio device via cpal. Other specs: "stdout"
// (or "-"), "fifo:/path", "unix:/path" (listen), "unix-connect:/path",
// "tcp:host:port" (listen), "tcp-connect:host:port". Listening variants block
// until a client connects; in stdout mode the host must keep stdout clean
// (e.g. pipe to `ffplay -f s16le -ar 44100 -ac 2 -`).
func WithOutput(spec string) Option {
	return func(c *Config) { c.Output = spec }
}

// WithResumeFile sets the .m3u8 path the player auto-persists the queue and
// exact playback position to, enabling [Player.Resume]. See [Config.ResumeFile].
func WithResumeFile(path string) Option {
	return func(c *Config) { c.ResumeFile = path }
}

// WithResumeSaveIntervalMs sets how often the resume file is rewritten while
// playing. 0 uses the Rockbox default (5 s). Ignored when no resume file is
// set. See [Config.ResumeSaveIntervalMs].
func WithResumeSaveIntervalMs(ms uint32) Option {
	return func(c *Config) { c.ResumeSaveIntervalMs = ms }
}

// New creates a player on the default device using the functional-options
// style: it starts from [DefaultConfig], applies each Option, and delegates to
// [NewPlayer]. It is a convenience wrapper — for full control over the [Config]
// struct, use [NewPlayer] directly.
//
//	p, err := rockbox.New(
//		rockbox.WithVolume(0.8),
//		rockbox.WithReplayGain(rockbox.ReplayGainTrack, 0.0, true),
//		rockbox.WithCrossfade(rockbox.CrossfadeAlways, 0, 2000, 0, 2000, rockbox.MixCrossfade),
//		rockbox.WithResumeFile("state.m3u8"),
//	)
//	if err != nil {
//		// ...
//	}
//	defer p.Close()
func New(opts ...Option) (*Player, error) {
	cfg := DefaultConfig()
	for _, opt := range opts {
		opt(&cfg)
	}
	return NewPlayer(cfg)
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

// SetQueue replaces the queue. Each entry may be a local file path, an
// http(s):// URL to a finite remote file, or a live-radio / streaming URL.
func (p *Player) SetQueue(paths []string) error {
	b, err := json.Marshal(paths)
	if err != nil {
		return fmt.Errorf("rockbox: marshaling queue: %w", err)
	}
	rbPlayerSetQueueJSON(p.ptr, string(b))
	return nil
}

// Enqueue appends a single track to the queue. path may be a local file
// path, an http(s):// URL to a finite remote file, or a live-radio /
// streaming URL.
func (p *Player) Enqueue(path string) { rbPlayerEnqueue(p.ptr, path) }

// Insert adds paths (local files or URLs) to the queue at position. index is
// only used when position is [InsertAtIndex].
func (p *Player) Insert(paths []string, position InsertPosition, index int) error {
	b, err := json.Marshal(paths)
	if err != nil {
		return fmt.Errorf("rockbox: marshaling insert paths: %w", err)
	}
	rbPlayerInsertJSON(p.ptr, string(b), int32(position), uint64(index))
	return nil
}

// Queue returns the current queue as a slice of paths/URLs.
func (p *Player) Queue() ([]string, error) {
	s, ok := takeString(rbPlayerQueueJSON(p.ptr))
	if !ok {
		return nil, errors.New("rockbox: rb_player_queue_json returned NULL")
	}
	var paths []string
	if err := json.Unmarshal([]byte(s), &paths); err != nil {
		return nil, fmt.Errorf("rockbox: decoding queue json: %w", err)
	}
	return paths, nil
}

// Remove deletes the queue entry at index (0-based). An out-of-range index is
// ignored. Removing a track before the current one keeps the current track
// playing; removing the currently-playing track hard-cuts to the track that
// slides into its place; removing the last remaining track stops playback.
func (p *Player) Remove(index int) { rbPlayerRemove(p.ptr, uint64(index)) }

// ClearQueue empties the queue and stops playback (also clearing any saved
// resume state).
func (p *Player) ClearQueue() { rbPlayerClearQueue(p.ptr) }

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

// SetBalance sets the stereo balance, -100 (full left) ..= +100 (full right);
// 0 is centred.
func (p *Player) SetBalance(balance int32) { rbPlayerSetBalance(p.ptr, balance) }

// Volume reports the current linear output volume.
func (p *Player) Volume() float32 { return rbPlayerVolume(p.ptr) }

// Balance reports the current stereo balance, -100 (full left) ..= +100
// (full right).
func (p *Player) Balance() int32 { return rbPlayerBalance(p.ptr) }

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

// SetShuffle turns queue shuffling on or off.
func (p *Player) SetShuffle(enabled bool) { rbPlayerSetShuffle(p.ptr, enabled) }

// IsShuffleEnabled reports whether queue shuffling is currently enabled.
func (p *Player) IsShuffleEnabled() bool { return rbPlayerIsShuffleEnabled(p.ptr) }

// SetRepeat sets the repeat mode (see [RepeatMode], Off=0, One=1, All=2).
func (p *Player) SetRepeat(mode RepeatMode) { rbPlayerSetRepeat(p.ptr, int32(mode)) }

// Repeat reports the current repeat mode (see [RepeatMode]).
func (p *Player) Repeat() RepeatMode { return RepeatMode(rbPlayerRepeat(p.ptr)) }

// SetEqEnabled turns the parametric equalizer on or off.
func (p *Player) SetEqEnabled(enabled bool) { rbPlayerSetEqEnabled(p.ptr, enabled) }

// IsEqEnabled reports whether the parametric equalizer is currently enabled.
func (p *Player) IsEqEnabled() bool { return rbPlayerIsEqEnabled(p.ptr) }

// SetEqBand configures a single EQ band: its cutoff/center frequency in Hz, Q
// factor, and gain in dB.
func (p *Player) SetEqBand(band int, cutoffHz int32, q, gainDb float32) {
	rbPlayerSetEqBand(p.ptr, uint64(band), cutoffHz, q, gainDb)
}

// SetEqPrecut sets the EQ pre-cut (headroom) in dB.
func (p *Player) SetEqPrecut(db float32) { rbPlayerSetEqPrecut(p.ptr, db) }

// SetEqPreset applies a built-in EQ preset (see [EqPreset]).
func (p *Player) SetEqPreset(preset EqPreset) { rbPlayerSetEqPreset(p.ptr, int32(preset)) }

// SetTone configures the bass/treble tone controls: bass and treble gain in dB
// and their respective cutoff frequencies in Hz.
func (p *Player) SetTone(bassDb, trebleDb, bassCutoffHz, trebleCutoffHz int32) {
	rbPlayerSetTone(p.ptr, bassDb, trebleDb, bassCutoffHz, trebleCutoffHz)
}

// SetBass sets the bass tone gain in dB.
func (p *Player) SetBass(bassDb int32) { rbPlayerSetBass(p.ptr, bassDb) }

// SetTreble sets the treble tone gain in dB.
func (p *Player) SetTreble(trebleDb int32) { rbPlayerSetTreble(p.ptr, trebleDb) }

// SetSurround configures surround processing: delay in ms, balance, and the low
// and high cutoff frequencies in Hz.
func (p *Player) SetSurround(delayMs, balance, cutoffLowHz, cutoffHighHz int32) {
	rbPlayerSetSurround(p.ptr, delayMs, balance, cutoffLowHz, cutoffHighHz)
}

// SetChannelMode selects the channel routing (see [ChannelMode]).
func (p *Player) SetChannelMode(mode ChannelMode) { rbPlayerSetChannelMode(p.ptr, int32(mode)) }

// SetStereoWidth sets the stereo width as a percentage.
func (p *Player) SetStereoWidth(percent int32) { rbPlayerSetStereoWidth(p.ptr, percent) }

// SetCompressor configures the dynamic-range compressor: threshold in dB,
// make-up gain, ratio, knee, attack in ms, and release in ms.
func (p *Player) SetCompressor(thresholdDb, makeupGain, ratio, knee, attackMs, releaseMs int32) {
	rbPlayerSetCompressor(p.ptr, thresholdDb, makeupGain, ratio, knee, attackMs, releaseMs)
}

// SetDither turns output dithering on or off.
func (p *Player) SetDither(enabled bool) { rbPlayerSetDither(p.ptr, enabled) }

// SetPitch sets the playback pitch ratio.
func (p *Player) SetPitch(ratio int32) { rbPlayerSetPitch(p.ptr, ratio) }

// SetBassCutoff sets the bass tone-control cutoff frequency in Hz.
func (p *Player) SetBassCutoff(hz int32) { rbPlayerSetBassCutoff(p.ptr, hz) }

// SetTrebleCutoff sets the treble tone-control cutoff frequency in Hz.
func (p *Player) SetTrebleCutoff(hz int32) { rbPlayerSetTrebleCutoff(p.ptr, hz) }

// SetCrossfeed configures crossfeed (headphone spatialization): the mode (see
// [CrossfeedMode]), the direct-signal gain, the cross-feed gain, the
// high-frequency gain, and the high-frequency cutoff in Hz. Gains are in the
// native Rockbox encoding.
func (p *Player) SetCrossfeed(mode CrossfeedMode, directGain, crossGain, hfGain, hfCutoff int32) {
	rbPlayerSetCrossfeed(p.ptr, int32(mode), directGain, crossGain, hfGain, hfCutoff)
}

// SetBassEnhancement configures the bass-enhancement effect: its strength and
// the pre-cut (headroom) applied to avoid clipping.
func (p *Player) SetBassEnhancement(strength, precut int32) {
	rbPlayerSetBassEnhancement(p.ptr, strength, precut)
}

// SetFatigueReduction sets the listening-fatigue-reduction strength.
func (p *Player) SetFatigueReduction(strength int32) {
	rbPlayerSetFatigueReduction(p.ptr, strength)
}

// DSPSettings returns a snapshot of the current DSP settings as a decoded map.
func (p *Player) DSPSettings() (map[string]any, error) {
	s, ok := takeString(rbPlayerDspSettingsJSON(p.ptr))
	if !ok {
		return nil, errors.New("rockbox: rb_player_dsp_settings_json returned NULL")
	}
	var settings map[string]any
	if err := json.Unmarshal([]byte(s), &settings); err != nil {
		return nil, fmt.Errorf("rockbox: decoding dsp settings json: %w", err)
	}
	return settings, nil
}

// ResumeState is a saved queue plus the exact playback position, as persisted
// to a resume file (see [Config.ResumeFile]).
type ResumeState struct {
	Tracks    []string `json:"tracks"`
	Index     int      `json:"index"`
	ElapsedMs uint64   `json:"elapsed_ms"`
}

// M3uEntry is one line of a parsed m3u/m3u8 playlist.
type M3uEntry struct {
	Path       string `json:"path"`
	DurationMs uint64 `json:"duration_ms"`
	Title      string `json:"title"`
}

// Resume restores the queue and exact position from the player's resume file
// (see [Config.ResumeFile]). It does NOT start playback. It returns nil, nil
// when there is no resume state to restore.
func (p *Player) Resume() (*ResumeState, error) {
	s, ok := takeString(rbPlayerResume(p.ptr))
	if !ok {
		return nil, nil
	}
	var rs ResumeState
	if err := json.Unmarshal([]byte(s), &rs); err != nil {
		return nil, fmt.Errorf("rockbox: decoding resume json: %w", err)
	}
	return &rs, nil
}

// SaveResume forces an immediate write of the resume file.
func (p *Player) SaveResume() { rbPlayerSaveResume(p.ptr) }

// ClearResume deletes the resume file.
func (p *Player) ClearResume() { rbPlayerClearResume(p.ptr) }

// ImportM3u imports a playlist file into the queue at position (index is only
// used when position is [InsertAtIndex]) and returns the imported paths.
func (p *Player) ImportM3u(path string, position InsertPosition, index int) ([]string, error) {
	s, ok := takeString(rbPlayerImportM3u(p.ptr, path, int32(position), uint64(index)))
	if !ok {
		return nil, fmt.Errorf("rockbox: could not import m3u %q", path)
	}
	var paths []string
	if err := json.Unmarshal([]byte(s), &paths); err != nil {
		return nil, fmt.Errorf("rockbox: decoding imported paths json: %w", err)
	}
	return paths, nil
}

// LoadM3u replaces the queue with a playlist file and returns the loaded paths.
func (p *Player) LoadM3u(path string) ([]string, error) {
	s, ok := takeString(rbPlayerLoadM3u(p.ptr, path))
	if !ok {
		return nil, fmt.Errorf("rockbox: could not load m3u %q", path)
	}
	var paths []string
	if err := json.Unmarshal([]byte(s), &paths); err != nil {
		return nil, fmt.Errorf("rockbox: decoding loaded paths json: %w", err)
	}
	return paths, nil
}

// ExportM3u writes the current queue to an .m3u8 file (atomically).
func (p *Player) ExportM3u(path string) error {
	if rbPlayerExportM3u(p.ptr, path) != 0 {
		return fmt.Errorf("rockbox: could not export m3u to %q", path)
	}
	return nil
}

// LoadResume peeks at a resume file without needing a Player. It returns
// nil, nil when the file has no resume state.
func LoadResume(path string) (*ResumeState, error) {
	s, ok := takeString(rbLoadResumeJSON(path))
	if !ok {
		return nil, nil
	}
	var rs ResumeState
	if err := json.Unmarshal([]byte(s), &rs); err != nil {
		return nil, fmt.Errorf("rockbox: decoding resume json: %w", err)
	}
	return &rs, nil
}

// M3uRead parses a playlist file into its entries.
func M3uRead(path string) ([]M3uEntry, error) {
	s, ok := takeString(rbM3uReadJSON(path))
	if !ok {
		return nil, fmt.Errorf("rockbox: could not read m3u %q", path)
	}
	var entries []M3uEntry
	if err := json.Unmarshal([]byte(s), &entries); err != nil {
		return nil, fmt.Errorf("rockbox: decoding m3u json: %w", err)
	}
	return entries, nil
}

// M3uWrite writes paths as an .m3u8 file.
func M3uWrite(path string, paths []string) error {
	b, err := json.Marshal(paths)
	if err != nil {
		return fmt.Errorf("rockbox: marshaling m3u paths: %w", err)
	}
	if rbM3uWriteJSON(path, string(b)) != 0 {
		return fmt.Errorf("rockbox: could not write m3u to %q", path)
	}
	return nil
}

// IsUrl reports whether s looks like an http(s):// URL.
func IsUrl(s string) bool { return rbIsURL(s) }

// Status is a snapshot of the player's playback state.
type Status struct {
	State      string `json:"state"` // "stopped" | "playing" | "paused"
	Index      *int   `json:"index"`
	PositionMs uint64 `json:"position_ms"`
	DurationMs uint64 `json:"duration_ms"`
	QueueLen   int    `json:"queue_len"`
	Metadata   *Meta  `json:"metadata"`
	Shuffle    bool   `json:"shuffle"`
	Repeat     string `json:"repeat"` // "off" | "one" | "all"
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
