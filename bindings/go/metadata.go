package rockbox

import (
	"encoding/json"
	"fmt"
)

// ReplayGain holds the ReplayGain tags parsed from an audio file. The *Db /
// *Peak fields are nil when the corresponding tag is absent; the Raw* fields
// are the native Q7.24 fixed-point values Rockbox uses internally (0 = not
// tagged) and can be fed straight to [Dsp.SetReplaygainGainsRaw].
type ReplayGain struct {
	TrackGainDb  *float32 `json:"track_gain_db"`
	AlbumGainDb  *float32 `json:"album_gain_db"`
	TrackPeak    *float32 `json:"track_peak"`
	AlbumPeak    *float32 `json:"album_peak"`
	RawTrackGain int64    `json:"raw_track_gain"`
	RawAlbumGain int64    `json:"raw_album_gain"`
	RawTrackPeak int64    `json:"raw_track_peak"`
	RawAlbumPeak int64    `json:"raw_album_peak"`
}

// AlbumArt locates embedded cover art inside the audio file (Rockbox does not
// decode it — it hands back the byte range so the caller can).
type AlbumArt struct {
	Kind         string `json:"kind"` // "bmp" | "png" | "jpeg" | "unknown"
	Offset       uint64 `json:"offset"`
	Size         uint32 `json:"size"`
	ID3Unsync    bool   `json:"id3_unsync"`
	VorbisBase64 bool   `json:"vorbis_base64"`
}

// Cuesheet locates an embedded cuesheet inside the audio file.
type Cuesheet struct {
	Offset   uint64 `json:"offset"`
	Size     uint32 `json:"size"`
	Encoding string `json:"encoding"` // "latin1" | "utf8" | "utf16le" | "utf16be" | "unknown"
}

// Meta is the parsed metadata of an audio file returned by [Metadata.Read].
type Meta struct {
	Codec            string     `json:"codec"`
	CodecID          uint32     `json:"codec_id"`
	Title            string     `json:"title"`
	Artist           string     `json:"artist"`
	Album            string     `json:"album"`
	AlbumArtist      string     `json:"albumartist"`
	Composer         string     `json:"composer"`
	Grouping         string     `json:"grouping"`
	Comment          string     `json:"comment"`
	Genre            string     `json:"genre"`
	YearString       string     `json:"year_string"`
	TrackString      string     `json:"track_string"`
	DiscString       string     `json:"disc_string"`
	MBTrackID        string     `json:"mb_track_id"`
	TrackNumber      *uint32    `json:"track_number"`
	DiscNumber       *uint32    `json:"disc_number"`
	Year             *uint32    `json:"year"`
	DurationMs       uint64     `json:"duration_ms"`
	Bitrate          uint32     `json:"bitrate"`
	SampleRate       uint32     `json:"sample_rate"`
	Filesize         uint64     `json:"filesize"`
	Samples          uint64     `json:"samples"`
	VBR              bool       `json:"vbr"`
	FirstFrameOffset uint64     `json:"first_frame_offset"`
	ReplayGain       ReplayGain `json:"replaygain"`
	AlbumArt         *AlbumArt  `json:"album_art"`
	Cuesheet         *Cuesheet  `json:"cuesheet"`
}

// Metadata is the audio-file metadata surface. It is stateless; use the
// package-level [Read] and [Probe] functions via the Metadata value, e.g.
// rockbox.Metadata.Read(path).
type metadataNS struct{}

// Metadata groups the metadata functions ([Metadata.Read], [Metadata.Probe]).
var Metadata metadataNS

// Read parses the metadata of the audio file at path. It returns an error if
// the file cannot be opened or no parser recognises it.
func (metadataNS) Read(path string) (*Meta, error) {
	s, ok := takeString(rbMetaReadJSON(path))
	if !ok {
		return nil, fmt.Errorf("rockbox: could not read metadata from %q", path)
	}
	var m Meta
	if err := json.Unmarshal([]byte(s), &m); err != nil {
		return nil, fmt.Errorf("rockbox: decoding metadata json: %w", err)
	}
	return &m, nil
}

// Probe guesses the codec label (e.g. "FLAC") from a filename's extension
// without opening the file. The bool is false for an unknown extension.
func (metadataNS) Probe(filename string) (string, bool) {
	return takeString(rbMetaProbe(filename))
}
