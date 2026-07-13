//// Decode an audio file to interleaved-stereo S16LE PCM, one chunk at a time.
////
//// Wraps the Rockbox codec engine. The codec state is process-wide, so only
//// one `Decoder` may decode at a time — opening a second one blocks until the
//// first is freed. It is a NIF resource, freed by the BEAM garbage collector —
//// no explicit close.

import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode
import gleam/option.{type Option, None, Some}
import rockbox/metadata.{type Metadata}

/// Opaque decoder handle (a NIF resource).
pub type Decoder

/// Open a decoder for the local audio file at `path`.
@external(erlang, "rockbox_ffi_nif", "decoder_open")
pub fn open(path: String) -> Decoder

/// Tags and stream properties (same shape as `metadata.read`).
pub fn metadata(decoder: Decoder) -> Metadata {
  let assert Ok(meta) =
    decode.run(json_decode(ffi_metadata_json(decoder)), metadata.decoder())
  meta
}

/// Next buffer of decoded PCM as `#(samples, sample_rate)`, where `samples` is
/// a little-endian int16 buffer of interleaved stereo PCM. `None` at end of
/// track (or after a codec error — see `finished`).
pub fn next_chunk(decoder: Decoder) -> Option(#(BitArray, Int)) {
  case decode.run(ffi_next_chunk(decoder), chunk_decoder()) {
    Ok(chunk) -> Some(chunk)
    Error(_) -> None
  }
}

/// Request a seek to `ms` milliseconds.
@external(erlang, "rockbox_ffi_nif", "decoder_seek_ms")
pub fn seek_ms(decoder: Decoder, ms: Int) -> Nil

/// Playback position last reported by the codec, in milliseconds.
@external(erlang, "rockbox_ffi_nif", "decoder_elapsed_ms")
pub fn elapsed_ms(decoder: Decoder) -> Int

/// `#(done, code)`: `done` is `True` once the track has ended; `code` is the
/// exit status (`0` = clean end, negative = codec error), valid only when
/// `done`.
@external(erlang, "rockbox_ffi_nif", "decoder_finished")
pub fn finished(decoder: Decoder) -> #(Bool, Int)

// -- decoders -----------------------------------------------------------

// next_chunk returns the tuple `#(binary, sample_rate)`, or the atom `nil` at
// end of track (typed Dynamic so we classify it via decode).
fn chunk_decoder() -> decode.Decoder(#(BitArray, Int)) {
  use samples <- decode.field(0, decode.bit_array)
  use rate <- decode.field(1, decode.int)
  decode.success(#(samples, rate))
}

// -- FFI ----------------------------------------------------------------

@external(erlang, "rockbox_ffi_nif", "decoder_metadata_json")
fn ffi_metadata_json(decoder: Decoder) -> String

@external(erlang, "rockbox_ffi_nif", "decoder_next_chunk")
fn ffi_next_chunk(decoder: Decoder) -> Dynamic

@external(erlang, "json", "decode")
fn json_decode(json: String) -> Dynamic
