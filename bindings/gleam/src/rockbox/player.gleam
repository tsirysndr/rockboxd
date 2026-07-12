//// Queue-based player with native ReplayGain and Rockbox crossfade.
////
//// A `Player` owns a live audio output device and a background engine
//// thread, so it only works where an output device exists. The handle is a
//// NIF resource, freed by the BEAM garbage collector (which stops playback).
////
//// ReplayGain `mode` here uses the player values: 0 off, 1 track, 2 album.
//// Crossfade `mode`: 0 off, 1 auto-skip, 2 manual-skip, 3 shuffle,
//// 4 shuffle-or-manual, 5 always. Mix mode: 0 crossfade, 1 mix.
////
//// Mutating functions return the `Player`, so they can be piped:
////
////     p
////     |> player.set_queue(tracks)
////     |> player.set_shuffle(True)
////     |> player.set_repeat(player.All)
////     |> player.play

import gleam/dynamic.{type Dynamic}
import gleam/dynamic/decode.{type Decoder}
import gleam/option.{type Option, None, Some}

/// Opaque player handle (a NIF resource).
pub type Player

/// Where an `insert` / `import_m3u` places its tracks relative to the queue.
///
///   - `Prepend`          — before everything.
///   - `Insert`           — as a block right after the current track.
///   - `InsertNext`       — immediately next (single-track semantics).
///   - `InsertLast`       — at the end.
///   - `InsertShuffled`   — shuffled into the queue.
///   - `InsertLastShuffled` — appended then shuffled among the new tail.
///   - `Replace`          — clear the queue first.
///   - `Index(i)`         — at explicit index `i`.
pub type InsertPosition {
  Prepend
  Insert
  InsertNext
  InsertLast
  InsertShuffled
  InsertLastShuffled
  Replace
  Index(Int)
}

/// Player configuration. `sample_rate: 0` means the output device default.
///
/// `resume_file` (an `.m3u8` path) enables auto-persistence of the queue and
/// exact position: `None` (the default) disables it. `resume_save_interval_ms`
/// of 0 uses Rockbox's 5 s default.
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
    resume_file: Option(String),
    resume_save_interval_ms: Int,
  )
}

/// Restored playback state as returned by `resume` / `load_resume`.
pub type ResumeState {
  ResumeState(tracks: List(String), index: Int, elapsed_ms: Int)
}

/// One entry parsed from a playlist file by `m3u_read`.
pub type M3uEntry {
  M3uEntry(path: String, duration_ms: Option(Int), title: Option(String))
}

/// Rockbox-default configuration (device sample rate, no crossfade,
/// ReplayGain off, full volume, resume disabled).
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
    resume_file: None,
    resume_save_interval_ms: 0,
  )
}

/// Encode an `InsertPosition` as the ABI's integer tag plus the index arg
/// (only meaningful for `Index`).
fn insert_position_args(position: InsertPosition) -> #(Int, Int) {
  case position {
    Prepend -> #(0, 0)
    Insert -> #(1, 0)
    InsertNext -> #(2, 0)
    InsertLast -> #(3, 0)
    InsertShuffled -> #(4, 0)
    InsertLastShuffled -> #(5, 0)
    Replace -> #(6, 0)
    Index(i) -> #(7, i)
  }
}

/// A built-in graphic-EQ preset (`set_eq_preset`). Values map to the ABI's
/// integer preset ids 0..20.
pub type EqPreset {
  Flat
  Acoustic
  BassBoost
  BassReducer
  Classical
  Dance
  Deep
  Electronic
  HipHop
  Jazz
  Latin
  Loudness
  Lounge
  Piano
  Pop
  RnB
  Rock
  SmallSpeakers
  TrebleBoost
  TrebleReducer
  VocalBoost
}

/// Encode an `EqPreset` as the ABI's integer id (0..20).
pub fn eq_preset_to_int(preset: EqPreset) -> Int {
  case preset {
    Flat -> 0
    Acoustic -> 1
    BassBoost -> 2
    BassReducer -> 3
    Classical -> 4
    Dance -> 5
    Deep -> 6
    Electronic -> 7
    HipHop -> 8
    Jazz -> 9
    Latin -> 10
    Loudness -> 11
    Lounge -> 12
    Piano -> 13
    Pop -> 14
    RnB -> 15
    Rock -> 16
    SmallSpeakers -> 17
    TrebleBoost -> 18
    TrebleReducer -> 19
    VocalBoost -> 20
  }
}

/// Channel-mixing mode (`set_channel_mode`). Values map to the ABI's integer
/// mode ids 0..6.
pub type ChannelMode {
  Stereo
  Mono
  Custom
  MonoLeft
  MonoRight
  Karaoke
  Swap
}

/// Encode a `ChannelMode` as the ABI's integer id (0..6).
pub fn channel_mode_to_int(mode: ChannelMode) -> Int {
  case mode {
    Stereo -> 0
    Mono -> 1
    Custom -> 2
    MonoLeft -> 3
    MonoRight -> 4
    Karaoke -> 5
    Swap -> 6
  }
}

/// Repeat mode (`set_repeat` / `repeat`). Values map to the ABI's integer
/// mode ids: 0 off, 1 one, 2 all.
pub type RepeatMode {
  Off
  One
  All
}

/// Encode a `RepeatMode` as the ABI's integer id (0/1/2).
pub fn repeat_mode_to_int(mode: RepeatMode) -> Int {
  case mode {
    Off -> 0
    One -> 1
    All -> 2
  }
}

/// Decode the ABI's integer id (0/1/2) into a `RepeatMode` (unknown → `Off`).
pub fn repeat_mode_from_int(value: Int) -> RepeatMode {
  case value {
    1 -> One
    2 -> All
    _ -> Off
  }
}

/// A snapshot of the player's status.
pub type Status {
  Status(
    state: String,
    index: Option(Int),
    position_ms: Int,
    duration_ms: Int,
    queue_len: Int,
    shuffle: Bool,
    repeat: String,
  )
}

/// One EQ band as reported by `dsp_settings`.
pub type EqBand {
  EqBand(cutoff_hz: Int, q: Float, gain_db: Float)
}

/// The graphic EQ section of `dsp_settings`.
pub type Equalizer {
  Equalizer(enabled: Bool, precut_db: Float, bands: List(EqBand))
}

/// The bass/treble tone controls of `dsp_settings`.
pub type Tone {
  Tone(
    bass_db: Int,
    treble_db: Int,
    bass_cutoff_hz: Int,
    treble_cutoff_hz: Int,
  )
}

/// The surround section of `dsp_settings`.
pub type Surround {
  Surround(
    delay_ms: Int,
    balance: Int,
    cutoff_low_hz: Int,
    cutoff_high_hz: Int,
  )
}

/// The compressor section of `dsp_settings`.
pub type Compressor {
  Compressor(
    threshold_db: Int,
    makeup_gain: Int,
    ratio: Int,
    knee: Int,
    attack_ms: Int,
    release_ms: Int,
  )
}

/// The whole DSP chain state, as returned by `dsp_settings`.
pub type DspSettings {
  DspSettings(
    equalizer: Equalizer,
    tone: Tone,
    surround: Surround,
    channel_mode: String,
    stereo_width: Int,
    compressor: Compressor,
    dither: Bool,
    pitch: Int,
  )
}

/// Create a player on the default device with default settings.
@external(erlang, "rockbox_ffi_nif", "player_new")
pub fn new() -> Player

/// Create a player with explicit configuration (including resume).
pub fn with_config(c: Config) -> Player {
  // Gleam's `None` maps to the atom `nil`, which the NIF reads as "no resume
  // file"; a `Some(path)` becomes a binary the NIF turns into a C string.
  let resume_file = case c.resume_file {
    Some(path) -> dynamic.string(path)
    None -> dynamic.nil()
  }
  ffi_player_new_with_config_ex(
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
    resume_file,
    c.resume_save_interval_ms,
  )
}

/// Replace the queue. Each entry may be a local file path, an `http(s)://`
/// URL to a finite remote file, or a live-radio / streaming URL.
pub fn set_queue(player: Player, paths: List(String)) -> Player {
  ffi_set_queue_json(player, iolist_to_binary(json_encode(paths)))
  player
}

/// Insert a list of paths / URLs into the queue at `position`.
pub fn insert(
  player: Player,
  paths: List(String),
  position: InsertPosition,
) -> Player {
  let #(pos, index) = insert_position_args(position)
  ffi_insert_json(player, iolist_to_binary(json_encode(paths)), pos, index)
  player
}

/// The current queue as a list of paths / URLs.
pub fn queue(player: Player) -> List(String) {
  case decode.run(json_decode(ffi_queue_json(player)), decode.list(decode.string)) {
    Ok(paths) -> paths
    Error(_) -> []
  }
}

/// Append one track to the queue. `path` may be a local file path, an
/// `http(s)://` URL to a finite remote file, or a live-radio / streaming URL.
pub fn enqueue(player: Player, path: String) -> Player {
  ffi_enqueue(player, path)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_enqueue")
fn ffi_enqueue(player: Player, path: String) -> Nil

/// Start (or resume) playback.
pub fn play(player: Player) -> Player {
  ffi_play(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_play")
fn ffi_play(player: Player) -> Nil

/// Pause playback.
pub fn pause(player: Player) -> Player {
  ffi_pause(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_pause")
fn ffi_pause(player: Player) -> Nil

/// Toggle between play and pause.
pub fn toggle(player: Player) -> Player {
  ffi_toggle(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_toggle")
fn ffi_toggle(player: Player) -> Nil

/// Stop playback.
pub fn stop(player: Player) -> Player {
  ffi_stop(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_stop")
fn ffi_stop(player: Player) -> Nil

/// Skip to the next track.
pub fn next(player: Player) -> Player {
  ffi_next(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_next")
fn ffi_next(player: Player) -> Nil

/// Skip to the previous track.
pub fn previous(player: Player) -> Player {
  ffi_previous(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_previous")
fn ffi_previous(player: Player) -> Nil

/// Skip to the track at `index` in the queue.
pub fn skip_to(player: Player, index: Int) -> Player {
  ffi_skip_to(player, index)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_skip_to")
fn ffi_skip_to(player: Player, index: Int) -> Nil

/// Seek to `ms` within the current track.
pub fn seek_ms(player: Player, ms: Int) -> Player {
  ffi_seek_ms(player, ms)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_seek_ms")
fn ffi_seek_ms(player: Player, ms: Int) -> Nil

/// Set the output volume (0.0..1.0).
pub fn set_volume(player: Player, volume: Float) -> Player {
  ffi_set_volume(player, volume)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_volume")
fn ffi_set_volume(player: Player, volume: Float) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_volume")
pub fn volume(player: Player) -> Float

@external(erlang, "rockbox_ffi_nif", "player_sample_rate")
pub fn sample_rate(player: Player) -> Int

/// Configure crossfade behaviour.
pub fn set_crossfade(
  player: Player,
  mode: Int,
  fade_out_delay_ms: Int,
  fade_out_duration_ms: Int,
  fade_in_delay_ms: Int,
  fade_in_duration_ms: Int,
  mix_mode: Int,
) -> Player {
  ffi_set_crossfade(
    player,
    mode,
    fade_out_delay_ms,
    fade_out_duration_ms,
    fade_in_delay_ms,
    fade_in_duration_ms,
    mix_mode,
  )
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_crossfade")
fn ffi_set_crossfade(
  player: Player,
  mode: Int,
  fade_out_delay_ms: Int,
  fade_out_duration_ms: Int,
  fade_in_delay_ms: Int,
  fade_in_duration_ms: Int,
  mix_mode: Int,
) -> Nil

/// Configure ReplayGain (mode, pre-amp in dB, clipping prevention).
pub fn set_replaygain(
  player: Player,
  mode: Int,
  preamp_db: Float,
  prevent_clipping: Bool,
) -> Player {
  ffi_set_replaygain(player, mode, preamp_db, prevent_clipping)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_replaygain")
fn ffi_set_replaygain(
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

// -- shuffle / repeat ---------------------------------------------------

/// Enable or disable shuffle playback.
pub fn set_shuffle(player: Player, enabled: Bool) -> Player {
  ffi_set_shuffle(player, enabled)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_shuffle")
fn ffi_set_shuffle(player: Player, enabled: Bool) -> Nil

/// Whether shuffle playback is currently enabled.
@external(erlang, "rockbox_ffi_nif", "player_is_shuffle_enabled")
pub fn is_shuffle_enabled(player: Player) -> Bool

/// Set the repeat mode (off / one / all).
pub fn set_repeat(player: Player, mode: RepeatMode) -> Player {
  ffi_set_repeat(player, repeat_mode_to_int(mode))
  player
}

/// The current repeat mode (off / one / all).
pub fn repeat(player: Player) -> RepeatMode {
  repeat_mode_from_int(ffi_repeat(player))
}

// -- DSP chain ----------------------------------------------------------

/// Enable or disable the graphic equalizer.
pub fn set_eq_enabled(player: Player, enabled: Bool) -> Player {
  ffi_set_eq_enabled(player, enabled)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_eq_enabled")
fn ffi_set_eq_enabled(player: Player, enabled: Bool) -> Nil

/// Whether the graphic equalizer is currently enabled.
@external(erlang, "rockbox_ffi_nif", "player_is_eq_enabled")
pub fn is_eq_enabled(player: Player) -> Bool

/// Configure one EQ band: `cutoff_hz` in Hz, `q` factor, `gain_db` in dB.
pub fn set_eq_band(
  player: Player,
  band: Int,
  cutoff_hz: Int,
  q: Float,
  gain_db: Float,
) -> Player {
  ffi_set_eq_band(player, band, cutoff_hz, q, gain_db)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_eq_band")
fn ffi_set_eq_band(
  player: Player,
  band: Int,
  cutoff_hz: Int,
  q: Float,
  gain_db: Float,
) -> Nil

/// Set the EQ pre-cut (headroom), in dB.
pub fn set_eq_precut(player: Player, db: Float) -> Player {
  ffi_set_eq_precut(player, db)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_eq_precut")
fn ffi_set_eq_precut(player: Player, db: Float) -> Nil

/// Apply a built-in EQ preset.
pub fn set_eq_preset(player: Player, preset: EqPreset) -> Player {
  ffi_set_eq_preset(player, eq_preset_to_int(preset))
  player
}

/// Set the bass/treble tone controls (gains in dB, cutoffs in Hz).
pub fn set_tone(
  player: Player,
  bass_db: Int,
  treble_db: Int,
  bass_cutoff_hz: Int,
  treble_cutoff_hz: Int,
) -> Player {
  ffi_set_tone(player, bass_db, treble_db, bass_cutoff_hz, treble_cutoff_hz)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_tone")
fn ffi_set_tone(
  player: Player,
  bass_db: Int,
  treble_db: Int,
  bass_cutoff_hz: Int,
  treble_cutoff_hz: Int,
) -> Nil

/// Set the bass tone gain, in dB.
pub fn set_bass(player: Player, bass_db: Int) -> Player {
  ffi_set_bass(player, bass_db)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_bass")
fn ffi_set_bass(player: Player, bass_db: Int) -> Nil

/// Set the treble tone gain, in dB.
pub fn set_treble(player: Player, treble_db: Int) -> Player {
  ffi_set_treble(player, treble_db)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_treble")
fn ffi_set_treble(player: Player, treble_db: Int) -> Nil

/// Configure the surround effect (delay in ms, balance, cutoffs in Hz).
pub fn set_surround(
  player: Player,
  delay_ms: Int,
  balance: Int,
  cutoff_low_hz: Int,
  cutoff_high_hz: Int,
) -> Player {
  ffi_set_surround(player, delay_ms, balance, cutoff_low_hz, cutoff_high_hz)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_surround")
fn ffi_set_surround(
  player: Player,
  delay_ms: Int,
  balance: Int,
  cutoff_low_hz: Int,
  cutoff_high_hz: Int,
) -> Nil

/// Set the channel-mixing mode.
pub fn set_channel_mode(player: Player, mode: ChannelMode) -> Player {
  ffi_set_channel_mode(player, channel_mode_to_int(mode))
  player
}

/// Set the stereo width, as a percentage.
pub fn set_stereo_width(player: Player, percent: Int) -> Player {
  ffi_set_stereo_width(player, percent)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_stereo_width")
fn ffi_set_stereo_width(player: Player, percent: Int) -> Nil

/// Configure the dynamic-range compressor.
pub fn set_compressor(
  player: Player,
  threshold_db: Int,
  makeup_gain: Int,
  ratio: Int,
  knee: Int,
  attack_ms: Int,
  release_ms: Int,
) -> Player {
  ffi_set_compressor(
    player,
    threshold_db,
    makeup_gain,
    ratio,
    knee,
    attack_ms,
    release_ms,
  )
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_compressor")
fn ffi_set_compressor(
  player: Player,
  threshold_db: Int,
  makeup_gain: Int,
  ratio: Int,
  knee: Int,
  attack_ms: Int,
  release_ms: Int,
) -> Nil

/// Enable or disable output dithering.
pub fn set_dither(player: Player, enabled: Bool) -> Player {
  ffi_set_dither(player, enabled)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_dither")
fn ffi_set_dither(player: Player, enabled: Bool) -> Nil

/// Set the pitch ratio (Rockbox's fixed-point pitch value).
pub fn set_pitch(player: Player, ratio: Int) -> Player {
  ffi_set_pitch(player, ratio)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_set_pitch")
fn ffi_set_pitch(player: Player, ratio: Int) -> Nil

/// Read back the whole DSP chain state.
pub fn dsp_settings(player: Player) -> DspSettings {
  let assert Ok(settings) =
    decode.run(json_decode(ffi_dsp_settings_json(player)), dsp_settings_decoder())
  settings
}

// -- resume -------------------------------------------------------------

/// Restore the queue and exact position from the configured resume file (does
/// NOT auto-play). `None` if there is nothing to resume.
pub fn resume(player: Player) -> Option(ResumeState) {
  decode_resume(ffi_resume(player))
}

/// Persist the current queue + position to the resume file right now.
pub fn save_resume(player: Player) -> Player {
  ffi_save_resume(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_save_resume")
fn ffi_save_resume(player: Player) -> Nil

/// Delete the resume file (forget the saved state).
pub fn clear_resume(player: Player) -> Player {
  ffi_clear_resume(player)
  player
}

@external(erlang, "rockbox_ffi_nif", "player_clear_resume")
fn ffi_clear_resume(player: Player) -> Nil

/// Peek at a resume file without a player. `None` if it is missing/unreadable.
pub fn load_resume(path: String) -> Option(ResumeState) {
  case decode.run(ffi_load_resume_json(path), decode.string) {
    Ok(json) -> decode_resume(dynamic.string(json))
    Error(_) -> None
  }
}

// -- m3u / m3u8 playlists -----------------------------------------------

/// Import a playlist file into the queue at `position`; returns the imported
/// paths.
pub fn import_m3u(
  player: Player,
  path: String,
  position: InsertPosition,
) -> List(String) {
  let #(pos, index) = insert_position_args(position)
  decode_string_list(ffi_import_m3u(player, path, pos, index))
}

/// Replace the queue with a playlist file; returns the loaded paths.
pub fn load_m3u(player: Player, path: String) -> List(String) {
  decode_string_list(ffi_load_m3u(player, path))
}

/// Export the current queue to an `.m3u8` (atomic). `Ok(Nil)` / `Error(Nil)`.
pub fn export_m3u(player: Player, path: String) -> Result(Nil, Nil) {
  case ffi_export_m3u(player, path) {
    0 -> Ok(Nil)
    _ -> Error(Nil)
  }
}

/// Parse a playlist file into its entries. `Error(Nil)` if it cannot be read.
pub fn m3u_read(path: String) -> Result(List(M3uEntry), Nil) {
  case decode.run(ffi_m3u_read_json(path), decode.string) {
    Ok(json) ->
      case decode.run(json_decode(json), decode.list(m3u_entry_decoder())) {
        Ok(entries) -> Ok(entries)
        Error(_) -> Error(Nil)
      }
    Error(_) -> Error(Nil)
  }
}

/// Write a list of paths as an `.m3u8`. `Ok(Nil)` / `Error(Nil)`.
pub fn m3u_write(path: String, paths: List(String)) -> Result(Nil, Nil) {
  case ffi_m3u_write_json(path, iolist_to_binary(json_encode(paths))) {
    0 -> Ok(Nil)
    _ -> Error(Nil)
  }
}

/// Whether a string looks like an http(s):// URL.
@external(erlang, "rockbox_ffi_nif", "is_url")
pub fn is_url(s: String) -> Bool

// -- decoders -----------------------------------------------------------

// The resume NIFs return a JSON binary or the atom `nil`; classify via
// decode.string then decode the object.
fn decode_resume(value: Dynamic) -> Option(ResumeState) {
  case decode.run(value, decode.string) {
    Ok(json) ->
      case decode.run(json_decode(json), resume_decoder()) {
        Ok(state) -> Some(state)
        Error(_) -> None
      }
    Error(_) -> None
  }
}

fn decode_string_list(value: Dynamic) -> List(String) {
  case decode.run(value, decode.string) {
    Ok(json) ->
      case decode.run(json_decode(json), decode.list(decode.string)) {
        Ok(paths) -> paths
        Error(_) -> []
      }
    Error(_) -> []
  }
}

fn resume_decoder() -> Decoder(ResumeState) {
  use tracks <- decode.field("tracks", decode.list(decode.string))
  use index <- decode.field("index", decode.int)
  use elapsed_ms <- decode.field("elapsed_ms", decode.int)
  decode.success(ResumeState(tracks:, index:, elapsed_ms:))
}

fn m3u_entry_decoder() -> Decoder(M3uEntry) {
  use path <- decode.field("path", decode.string)
  use duration_ms <- decode.field("duration_ms", optional_int())
  use title <- decode.field("title", optional_string())
  decode.success(M3uEntry(path:, duration_ms:, title:))
}

fn optional_int() -> Decoder(Option(Int)) {
  decode.one_of(decode.map(decode.int, Some), or: [decode.success(None)])
}

fn optional_string() -> Decoder(Option(String)) {
  decode.one_of(decode.map(decode.string, Some), or: [decode.success(None)])
}

// -- status decoder -----------------------------------------------------

fn status_decoder() -> Decoder(Status) {
  use state <- decode.field("state", decode.string)
  use index <- decode.field(
    "index",
    decode.one_of(decode.map(decode.int, Some), or: [decode.success(None)]),
  )
  use position_ms <- decode.field("position_ms", decode.int)
  use duration_ms <- decode.field("duration_ms", decode.int)
  use queue_len <- decode.field("queue_len", decode.int)
  use shuffle <- decode.field("shuffle", decode.bool)
  use repeat <- decode.field("repeat", decode.string)
  decode.success(Status(
    state:,
    index:,
    position_ms:,
    duration_ms:,
    queue_len:,
    shuffle:,
    repeat:,
  ))
}

// -- dsp settings decoder -----------------------------------------------

fn eq_band_decoder() -> Decoder(EqBand) {
  use cutoff_hz <- decode.field("cutoff_hz", decode.int)
  use q <- decode.field("q", decode.float)
  use gain_db <- decode.field("gain_db", decode.float)
  decode.success(EqBand(cutoff_hz:, q:, gain_db:))
}

fn equalizer_decoder() -> Decoder(Equalizer) {
  use enabled <- decode.field("enabled", decode.bool)
  use precut_db <- decode.field("precut_db", decode.float)
  use bands <- decode.field("bands", decode.list(eq_band_decoder()))
  decode.success(Equalizer(enabled:, precut_db:, bands:))
}

fn tone_decoder() -> Decoder(Tone) {
  use bass_db <- decode.field("bass_db", decode.int)
  use treble_db <- decode.field("treble_db", decode.int)
  use bass_cutoff_hz <- decode.field("bass_cutoff_hz", decode.int)
  use treble_cutoff_hz <- decode.field("treble_cutoff_hz", decode.int)
  decode.success(Tone(bass_db:, treble_db:, bass_cutoff_hz:, treble_cutoff_hz:))
}

fn surround_decoder() -> Decoder(Surround) {
  use delay_ms <- decode.field("delay_ms", decode.int)
  use balance <- decode.field("balance", decode.int)
  use cutoff_low_hz <- decode.field("cutoff_low_hz", decode.int)
  use cutoff_high_hz <- decode.field("cutoff_high_hz", decode.int)
  decode.success(Surround(delay_ms:, balance:, cutoff_low_hz:, cutoff_high_hz:))
}

fn compressor_decoder() -> Decoder(Compressor) {
  use threshold_db <- decode.field("threshold_db", decode.int)
  use makeup_gain <- decode.field("makeup_gain", decode.int)
  use ratio <- decode.field("ratio", decode.int)
  use knee <- decode.field("knee", decode.int)
  use attack_ms <- decode.field("attack_ms", decode.int)
  use release_ms <- decode.field("release_ms", decode.int)
  decode.success(Compressor(
    threshold_db:,
    makeup_gain:,
    ratio:,
    knee:,
    attack_ms:,
    release_ms:,
  ))
}

fn dsp_settings_decoder() -> Decoder(DspSettings) {
  use equalizer <- decode.field("equalizer", equalizer_decoder())
  use tone <- decode.field("tone", tone_decoder())
  use surround <- decode.field("surround", surround_decoder())
  use channel_mode <- decode.field("channel_mode", decode.string)
  use stereo_width <- decode.field("stereo_width", decode.int)
  use compressor <- decode.field("compressor", compressor_decoder())
  use dither <- decode.field("dither", decode.bool)
  use pitch <- decode.field("pitch", decode.int)
  decode.success(DspSettings(
    equalizer:,
    tone:,
    surround:,
    channel_mode:,
    stereo_width:,
    compressor:,
    dither:,
    pitch:,
  ))
}

// -- FFI ----------------------------------------------------------------

@external(erlang, "rockbox_ffi_nif", "player_new_with_config_ex")
fn ffi_player_new_with_config_ex(
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
  resume_file: Dynamic,
  resume_save_interval_ms: Int,
) -> Player

@external(erlang, "rockbox_ffi_nif", "player_set_queue_json")
fn ffi_set_queue_json(player: Player, json: BitArray) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_insert_json")
fn ffi_insert_json(
  player: Player,
  json: BitArray,
  position: Int,
  index: Int,
) -> Nil

// Returns a JSON binary array of strings.
@external(erlang, "rockbox_ffi_nif", "player_queue_json")
fn ffi_queue_json(player: Player) -> String

@external(erlang, "rockbox_ffi_nif", "player_status_json")
fn ffi_status_json(player: Player) -> String

@external(erlang, "rockbox_ffi_nif", "player_set_eq_preset")
fn ffi_set_eq_preset(player: Player, preset: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_set_channel_mode")
fn ffi_set_channel_mode(player: Player, mode: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_set_repeat")
fn ffi_set_repeat(player: Player, mode: Int) -> Nil

@external(erlang, "rockbox_ffi_nif", "player_repeat")
fn ffi_repeat(player: Player) -> Int

@external(erlang, "rockbox_ffi_nif", "player_dsp_settings_json")
fn ffi_dsp_settings_json(player: Player) -> String

// Returns a JSON binary, or the atom `nil` (typed Dynamic so we classify it).
@external(erlang, "rockbox_ffi_nif", "player_resume")
fn ffi_resume(player: Player) -> Dynamic

@external(erlang, "rockbox_ffi_nif", "load_resume_json")
fn ffi_load_resume_json(path: String) -> Dynamic

// Returns a JSON binary array of the imported/loaded paths.
@external(erlang, "rockbox_ffi_nif", "player_import_m3u")
fn ffi_import_m3u(
  player: Player,
  path: String,
  position: Int,
  index: Int,
) -> Dynamic

@external(erlang, "rockbox_ffi_nif", "player_load_m3u")
fn ffi_load_m3u(player: Player, path: String) -> Dynamic

@external(erlang, "rockbox_ffi_nif", "player_export_m3u")
fn ffi_export_m3u(player: Player, path: String) -> Int

// Returns a JSON binary, or the atom `nil` (typed Dynamic so we classify it).
@external(erlang, "rockbox_ffi_nif", "m3u_read_json")
fn ffi_m3u_read_json(path: String) -> Dynamic

@external(erlang, "rockbox_ffi_nif", "m3u_write_json")
fn ffi_m3u_write_json(path: String, json: BitArray) -> Int

@external(erlang, "json", "encode")
fn json_encode(paths: List(String)) -> Dynamic

@external(erlang, "erlang", "iolist_to_binary")
fn iolist_to_binary(iodata: Dynamic) -> BitArray

@external(erlang, "json", "decode")
fn json_decode(json: String) -> Dynamic
