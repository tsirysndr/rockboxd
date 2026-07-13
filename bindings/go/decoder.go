package rockbox

import (
	"encoding/json"
	"errors"
	"fmt"
	"unsafe"
)

// Decoder decodes a local audio file to interleaved-stereo S16 PCM, one chunk
// at a time, using the Rockbox codec engine.
//
// The codec state is process-wide, so only one Decoder may decode at a time and
// it must be used from one goroutine. Call [Decoder.Close] when done.
type Decoder struct {
	ptr uintptr
}

// OpenDecoder opens the audio file at path for decoding. It returns an error if
// the file cannot be opened or no codec recognises it.
func OpenDecoder(path string) (*Decoder, error) {
	ptr := rbDecoderOpen(path)
	if ptr == 0 {
		return nil, fmt.Errorf("rockbox: could not open decoder for %q", path)
	}
	return &Decoder{ptr: ptr}, nil
}

// Close frees the native decoder. It is safe to call more than once.
func (d *Decoder) Close() {
	if d.ptr == 0 {
		return
	}
	rbDecoderFree(d.ptr)
	d.ptr = 0
}

// Metadata returns the file's tags and stream properties (same shape as
// [Metadata.Read]).
func (d *Decoder) Metadata() (map[string]any, error) {
	s, ok := takeString(rbDecoderMetadataJSON(d.ptr))
	if !ok {
		return nil, errors.New("rockbox: rb_decoder_metadata_json returned NULL")
	}
	var m map[string]any
	if err := json.Unmarshal([]byte(s), &m); err != nil {
		return nil, fmt.Errorf("rockbox: decoding metadata json: %w", err)
	}
	return m, nil
}

// NextChunk pulls the next buffer of decoded PCM as interleaved stereo int16
// samples plus the sample rate (Hz). The bool is false at end of track (or
// after a codec error — see [Decoder.Finished]).
func (d *Decoder) NextChunk() ([]int16, uint32, bool) {
	var outLen uint64
	var outRate uint32
	ptr := rbDecoderNextChunk(d.ptr, unsafe.Pointer(&outLen), unsafe.Pointer(&outRate))
	if ptr == 0 || outLen == 0 {
		return nil, 0, false
	}
	src := unsafe.Slice((*int16)(unsafe.Pointer(ptr)), outLen)
	out := make([]int16, outLen)
	copy(out, src)
	rbBufferFree(ptr, outLen)
	return out, outRate, true
}

// SeekMs requests a seek to ms milliseconds.
func (d *Decoder) SeekMs(ms uint64) { rbDecoderSeekMs(d.ptr, ms) }

// ElapsedMs returns the playback position last reported by the codec, in
// milliseconds.
func (d *Decoder) ElapsedMs() uint64 { return rbDecoderElapsedMs(d.ptr) }

// Finished reports whether the track has ended. When done is true, code is the
// exit status (0 = clean end, negative = codec error); it is meaningful only
// when done is true.
func (d *Decoder) Finished() (bool, int32) {
	var outCode int32
	done := rbDecoderFinished(d.ptr, unsafe.Pointer(&outCode))
	return done, outCode
}
