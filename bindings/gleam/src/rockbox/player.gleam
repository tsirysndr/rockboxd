//// Queue-based player with native ReplayGain and Rockbox crossfade.
////
//// A `Player` owns a live audio output device and a background engine
//// thread, so it only works where an output device exists. The handle is a
//// NIF resource, freed by the BEAM garbage collector (which stops playback).
////
//// ReplayGain `mode` here uses the player values: 0 off, 1 track, 2 album.
//// Crossfade `mode`: 0 off, 1 auto-skip, 2 manual-skip, 3 shuffle,
//// 4 shuffle-or-manual, 5 always. Mix mode: 0 crossfade, 1 mix.

import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode.{type Decoder}
import gleam/option.{type Option, None, Some}

/// Opaque player handle (a NIF resource).
pub type Player

/// Player configuration. `sample_rate: 0` means the output device default.
pub type Config {
  Config(
    sample_rate: Int,
    buffer_seconds: Float,
    volume: Float,
    replaygain_mode: Int,
    replaygain_preamp_db: Float,
    replaygain_prevent_clipping: Bool,
    crossfade_mode: Int,
    fade_out_delay_ms: Int,
    fade_out_duration_ms: Int,
    fade_in_delay_ms: Int,
    fade_in_duration_ms: Int,
    mix_mode: Int,
  )
}

/// Rockbox-default configuration (device sample rate, no crossfade,
/// ReplayGain off, full volume).
pub fn default_config() -> Config {
  Config(
    sample_rate: 0,
    buffer_seconds: 4.0,
    volume: 1.0,
    replaygain_mode: 0,
    replaygain_preamp_db: 0.0,
    replaygain_prevent_clipping: True,
    crossfade_mode: 0,
    fade_out_delay_ms: 0,
    fade_out_duration_ms: 2000,
    fade_in_delay_ms: 0,
    fade_in_duration_ms: 2000,
    mix_mode: 0,
  )
}

/// A snapshot of the player's status.
pub type Status {
  Status(
    state: String,
    index: Option(Int),
    position_ms: Int,
    duration_ms: Int,
    queue_len: Int,
  )
}

/// Create a player on the default device with default settings.
@external(erlang, "rockbox_ffi_nif", "player_new")
pub fn new() -> Player

/// Create a player with explicit configuration.
pub fn with_config(c: Config) -> Player {
  ffi_player_new_with_config(
    c.sample_rate,
    c.buffer_seconds,
    c.volume,
    c.replaygain_mode,
    c.replaygain_preamp_db,
    c.replaygain_prevent_clipping,
    c.crossfade_mode,
    c.fade_out_delay_ms,
    c.fade_out_duration_ms,
    c.fade_in_delay_ms,
    c.fade_in_duration_ms,
    c.mix_mode,
  )
}

/// Replace the queue with a list of file paths.
pub fn set_queue(player: Player, paths: List(String)) -> Nil {
  ffi_set_queue_json(player, iolist_to_binary(json_encode(paths)))
}

@external(erlang, "rockbox_ffi_nif", "player_enqueue")
pub fn enqueue(player: Player, path: String) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_play")
pub fn play(player: Player) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_pause")
pub fn pause(player: Player) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_toggle")
pub fn toggle(player: Player) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_stop")
pub fn stop(player: Player) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_next")
pub fn next(player: Player) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_previous")
pub fn previous(player: Player) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_skip_to")
pub fn skip_to(player: Player, index: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_seek_ms")
pub fn seek_ms(player: Player, ms: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_set_volume")
pub fn set_volume(player: Player, volume: Float) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_volume")
pub fn volume(player: Player) -> Float

@external(erlang, "rockbox_ffi_nif", "player_sample_rate")
pub fn sample_rate(player: Player) -> Int

@external(erlang, "rockbox_ffi_nif", "player_set_crossfade")
pub fn set_crossfade(
  player: Player,
  mode: Int,
  fade_out_delay_ms: Int,
  fade_out_duration_ms: Int,
  fade_in_delay_ms: Int,
  fade_in_duration_ms: Int,
  mix_mode: Int,
) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_set_replaygain")
pub fn set_replaygain(
  player: Player,
  mode: Int,
  preamp_db: Float,
  prevent_clipping: Bool,
) -> Nil

/// A snapshot of the player's status.
pub fn status(player: Player) -> Status {
  let assert Ok(status) =
    decode.run(json_decode(ffi_status_json(player)), status_decoder())
  status
}

// -- decoders -----------------------------------------------------------

fn status_decoder() -> Decoder(Status) {
  use state <- decode.field("state", decode.string)
  use index <- decode.field(
    "index",
    decode.one_of(decode.map(decode.int, Some), or: [decode.success(None)]),
  )
  use position_ms <- decode.field("position_ms", decode.int)
  use duration_ms <- decode.field("duration_ms", decode.int)
  use queue_len <- decode.field("queue_len", decode.int)
  decode.success(Status(state:, index:, position_ms:, duration_ms:, queue_len:))
}

// -- FFI ----------------------------------------------------------------

@external(erlang, "rockbox_ffi_nif", "player_new_with_config")
fn ffi_player_new_with_config(
  sample_rate: Int,
  buffer_seconds: Float,
  volume: Float,
  rg_mode: Int,
  rg_preamp_db: Float,
  rg_prevent_clipping: Bool,
  xfade_mode: Int,
  fo_delay_ms: Int,
  fo_dur_ms: Int,
  fi_delay_ms: Int,
  fi_dur_ms: Int,
  mix_mode: Int,
) -> Player

@external(erlang, "rockbox_ffi_nif", "player_set_queue_json")
fn ffi_set_queue_json(player: Player, json: BitArray) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_status_json")
fn ffi_status_json(player: Player) -> String

@external(erlang, "json", "encode")
fn json_encode(paths: List(String)) -> Dynamic

@external(erlang, "erlang", "iolist_to_binary")
fn iolist_to_binary(iodata: Dynamic) -> BitArray

@external(erlang, "json", "decode")
fn json_decode(json: String) -> Dynamic
