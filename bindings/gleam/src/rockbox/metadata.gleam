//// Audio file metadata / tag parsing.

import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode.{type Decoder}
import gleam/option.{type Option, None, Some}

/// ReplayGain values parsed from the file's tags. The `*_db` / `*_peak`
/// fields are user-friendly; the `raw_*` fields keep Rockbox's native Q7.24
/// encoding — feed them straight into `dsp.set_replaygain_gains_raw`.
pub type ReplayGain {
  ReplayGain(
    track_gain_db: Option(Float),
    album_gain_db: Option(Float),
    track_peak: Option(Float),
    album_peak: Option(Float),
    raw_track_gain: Int,
    raw_album_gain: Int,
    raw_track_peak: Int,
    raw_album_peak: Int,
  )
}

/// Parsed metadata for one audio file (the common fields).
pub type Metadata {
  Metadata(
    codec: String,
    codec_id: Int,
    title: String,
    artist: String,
    album: String,
    albumartist: String,
    composer: String,
    genre: String,
    year: Option(Int),
    track_number: Option(Int),
    disc_number: Option(Int),
    duration_ms: Int,
    bitrate: Int,
    sample_rate: Int,
    filesize: Int,
    samples: Int,
    vbr: Bool,
    replaygain: ReplayGain,
  )
}

/// Parse the metadata of the audio file at `path`. `Error(Nil)` if the file
/// cannot be opened, no parser recognises it, or the JSON fails to decode.
pub fn read(path: String) -> Result(Metadata, Nil) {
  case decode.run(ffi_meta_read_json(path), decode.string) {
    Ok(json) ->
      case decode.run(json_decode(json), metadata_decoder()) {
        Ok(meta) -> Ok(meta)
        Error(_) -> Error(Nil)
      }
    Error(_) -> Error(Nil)
  }
}

/// Guess the codec label (e.g. `"FLAC"`) from a filename's extension without
/// opening the file. `None` for an unknown extension.
pub fn probe(filename: String) -> Option(String) {
  case decode.run(ffi_meta_probe(filename), decode.string) {
    Ok(label) -> Some(label)
    Error(_) -> None
  }
}

// -- decoders -----------------------------------------------------------

fn metadata_decoder() -> Decoder(Metadata) {
  use codec <- decode.field("codec", decode.string)
  use codec_id <- decode.field("codec_id", decode.int)
  use title <- decode.field("title", decode.string)
  use artist <- decode.field("artist", decode.string)
  use album <- decode.field("album", decode.string)
  use albumartist <- decode.field("albumartist", decode.string)
  use composer <- decode.field("composer", decode.string)
  use genre <- decode.field("genre", decode.string)
  use year <- decode.field("year", optional_int())
  use track_number <- decode.field("track_number", optional_int())
  use disc_number <- decode.field("disc_number", optional_int())
  use duration_ms <- decode.field("duration_ms", decode.int)
  use bitrate <- decode.field("bitrate", decode.int)
  use sample_rate <- decode.field("sample_rate", decode.int)
  use filesize <- decode.field("filesize", decode.int)
  use samples <- decode.field("samples", decode.int)
  use vbr <- decode.field("vbr", decode.bool)
  use replaygain <- decode.field("replaygain", replaygain_decoder())
  decode.success(Metadata(
    codec:,
    codec_id:,
    title:,
    artist:,
    album:,
    albumartist:,
    composer:,
    genre:,
    year:,
    track_number:,
    disc_number:,
    duration_ms:,
    bitrate:,
    sample_rate:,
    filesize:,
    samples:,
    vbr:,
    replaygain:,
  ))
}

fn replaygain_decoder() -> Decoder(ReplayGain) {
  use track_gain_db <- decode.field("track_gain_db", optional_float())
  use album_gain_db <- decode.field("album_gain_db", optional_float())
  use track_peak <- decode.field("track_peak", optional_float())
  use album_peak <- decode.field("album_peak", optional_float())
  use raw_track_gain <- decode.field("raw_track_gain", decode.int)
  use raw_album_gain <- decode.field("raw_album_gain", decode.int)
  use raw_track_peak <- decode.field("raw_track_peak", decode.int)
  use raw_album_peak <- decode.field("raw_album_peak", decode.int)
  decode.success(ReplayGain(
    track_gain_db:,
    album_gain_db:,
    track_peak:,
    album_peak:,
    raw_track_gain:,
    raw_album_gain:,
    raw_track_peak:,
    raw_album_peak:,
  ))
}

// JSON null (decoded by :json to the atom `null`) is not Gleam's Nil, so use a
// tolerant one_of: try the value, else fall back to None.
fn optional_int() -> Decoder(Option(Int)) {
  decode.one_of(decode.map(decode.int, Some), or: [decode.success(None)])
}

fn optional_float() -> Decoder(Option(Float)) {
  decode.one_of(decode.map(decode.float, Some), or: [decode.success(None)])
}

// -- FFI ----------------------------------------------------------------

// Returns a JSON binary, or the atom `nil` on failure (typed as Dynamic so we
// can classify it via decode.string).
@external(erlang, "rockbox_ffi_nif", "meta_read_json")
fn ffi_meta_read_json(path: String) -> Dynamic

@external(erlang, "rockbox_ffi_nif", "meta_probe")
fn ffi_meta_probe(filename: String) -> Dynamic

// OTP 27+ built-in JSON decoder: binary -> term (Dynamic).
@external(erlang, "json", "decode")
fn json_decode(json: String) -> Dynamic
