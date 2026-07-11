import gleam/float
import gleam/int
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import gleeunit
import gleeunit/should
import rockbox/dsp
import rockbox/metadata
import rockbox/player
import rockbox_ffi

const fixture = "../../crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a"

pub fn main() {
  gleeunit.main()
}

pub fn abi_version_test() {
  rockbox_ffi.abi_version()
  |> should.equal(1)
}

pub fn metadata_test() {
  let assert Ok(meta) = metadata.read(fixture)
  meta.codec |> should.equal("AAC")
  meta.sample_rate |> should.equal(44_100)
  should.be_true(meta.duration_ms > 0)

  metadata.probe("song.flac") |> should.equal(Some("FLAC"))
  metadata.probe("nope.xyz") |> should.equal(None)
}

pub fn dsp_replaygain_halves_amplitude_test() {
  let d = dsp.new(44_100)
  dsp.set_replaygain(d, 0, False, 0.0)
  dsp.set_replaygain_gains(d, Some(-6.0206), None, None, None)

  let sine = build_sine(0, <<>>)
  let out = dsp.process(d, sine)
  let peak = max_abs(out, 0)
  should.be_true(peak >= 7600 && peak <= 8400)
}

pub fn player_construct_test() {
  let p = player.with_config(player.Config(..player.default_config(), volume: 0.0))
  should.be_true(player.sample_rate(p) > 0)
  player.set_volume(p, 0.0)
  player.set_queue(p, [fixture])
  sleep(100)

  let status = player.status(p)
  status.state |> should.equal("stopped")
  status.queue_len |> should.equal(1)
}

pub fn player_queue_and_insert_test() {
  let p = player.with_config(player.Config(..player.default_config(), volume: 0.0))
  player.set_queue(p, [fixture])
  sleep(100)
  player.queue(p) |> list.length |> should.equal(1)

  // Prepend a second entry; the queue now holds two tracks.
  player.insert(p, [fixture], player.Prepend)
  sleep(100)
  player.status(p).queue_len |> should.equal(2)
}

pub fn is_url_test() {
  player.is_url("http://example.com/a.mp3") |> should.be_true
  player.is_url("https://example.com/a.mp3") |> should.be_true
  player.is_url("/local/path.mp3") |> should.be_false
}

pub fn m3u_round_trip_test() {
  let path = "/tmp/rockbox_gleam_test.m3u8"
  let assert Ok(Nil) = player.m3u_write(path, [fixture])
  let assert Ok(entries) = player.m3u_read(path)
  // The writer resolves paths relative to the .m3u8's directory, so compare
  // the basename rather than the exact (now absolutised) path.
  case entries {
    [entry, ..] -> should.be_true(string.ends_with(entry.path, ".m4a"))
    [] -> should.fail()
  }
}

pub fn player_load_m3u_test() {
  let path = "/tmp/rockbox_gleam_load.m3u8"
  let assert Ok(Nil) = player.m3u_write(path, [fixture])

  let p = player.with_config(player.Config(..player.default_config(), volume: 0.0))
  player.load_m3u(p, path) |> list.length |> should.equal(1)
  sleep(100)
  player.status(p).queue_len |> should.equal(1)

  let assert Ok(Nil) = player.export_m3u(p, "/tmp/rockbox_gleam_export.m3u8")
  Nil
}

// -- helpers ------------------------------------------------------------

// Build 1s of a 1000 Hz sine as interleaved stereo little-endian int16.
fn build_sine(i: Int, acc: BitArray) -> BitArray {
  case i >= 44_100 {
    True -> acc
    False -> {
      let s =
        float.round(
          sin(int.to_float(i) *. 2.0 *. pi() *. 1000.0 /. 44_100.0) *. 16_000.0,
        )
      build_sine(i + 1, <<acc:bits, s:size(16)-little, s:size(16)-little>>)
    }
  }
}

fn max_abs(bin: BitArray, acc: Int) -> Int {
  case bin {
    <<s:size(16)-little-signed, rest:bits>> -> {
      let a = case s < 0 {
        True -> -s
        False -> s
      }
      max_abs(rest, case a > acc {
        True -> a
        False -> acc
      })
    }
    _ -> acc
  }
}

@external(erlang, "math", "sin")
fn sin(x: Float) -> Float

@external(erlang, "math", "pi")
fn pi() -> Float

fn sleep(ms: Int) -> Nil {
  do_sleep(ms)
  Nil
}

@external(erlang, "timer", "sleep")
fn do_sleep(ms: Int) -> a
