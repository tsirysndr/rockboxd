# rockbox_ffi (Ruby)

[![Gem](https://img.shields.io/gem/v/rockbox_ffi?logo=rubygems&logoColor=white)](https://rubygems.org/gems/rockbox_ffi)
![Ruby](https://img.shields.io/badge/Ruby-2.6%2B-CC342D?logo=ruby&logoColor=white)
![FFI](https://img.shields.io/badge/FFI-fiddle-CC342D)
![License](https://img.shields.io/badge/license-GPL--2.0--or--later-blue)

Ruby bindings for the Rockbox **DSP**, **metadata**, and **playback** engine,
via [`fiddle`](https://docs.ruby-lang.org/en/master/Fiddle.html) (Ruby stdlib)
over the prebuilt `librockbox_ffi` shared library. No native extension is
compiled — the gem `dlopen`s the shared library at load time.

> 📖 **Sound settings reference** — the equalizer, tone, crossfeed, compressor
> and other DSP controls mirror Rockbox's own. See the official
> [Rockbox manual — Sound Settings](https://download.rockbox.org/daily/manual/rockbox-ipodvideo/rockbox-buildch6.html).

## Setup

Build the shared library once (from the repo root):

```sh
cargo build --release -p rockbox-ffi
```

Then, from this directory:

```sh
cd bindings/ruby
bundle install                 # installs rake + minitest (dev only)
ruby -Ilib examples/smoke.rb   # end-to-end check
```

The library is located automatically by walking up to
`target/release/librockbox_ffi.{dylib,so}`. Override with the
`ROCKBOX_FFI_LIB` environment variable.

## Usage

```ruby
require "rockbox_ffi"

# --- metadata ---------------------------------------------------------
meta = RockboxFFI::Metadata.read("song.flac")
puts "#{meta[:artist]} — #{meta[:title]} (#{meta[:duration_ms]} ms)"
RockboxFFI::Metadata.probe("track.opus")   # => "Opus"

# --- DSP (interleaved stereo int16) -----------------------------------
RockboxFFI::Dsp.open(44_100) do |dsp|
  dsp.eq_enable(true)
  dsp.set_eq_band(0, 60, 0.7, 3.0)
  dsp.set_replaygain(RockboxFFI::DspReplayGainMode::TRACK, true, 0.0)
  dsp.set_replaygain_gains(track_gain_db: -6.02)  # halves amplitude
  processed = dsp.process(samples)                # Array<Integer>
end

# --- playback (needs an output device) --------------------------------
RockboxFFI::Player.open(volume: 0.8) do |player|
  player.set_replaygain(RockboxFFI::ReplayGainMode::TRACK, 0.0, true)
  player.set_crossfade(RockboxFFI::CrossfadeMode::ALWAYS)
  player.set_queue(["a.flac", "b.mp3", "c.opus"])
  player.play
  player.status   # => {state: "playing", index: 0, ...}
end
```

`Dsp` and `Player` own native resources. Use the block form (`.open`) to close
them automatically, or call `#close` yourself; a GC finalizer frees the handle
as a backstop.

## API

| Namespace                | Contents                                                          |
| ------------------------ | ----------------------------------------------------------------- |
| `RockboxFFI::Metadata`   | `read(path) => Hash`, `probe(filename) => String \| nil`          |
| `RockboxFFI::Dsp`        | EQ / tone / surround / compressor / ReplayGain, `process(samples)` |
| `RockboxFFI::Player`     | queue + transport + crossfade + ReplayGain, `status => Hash`      |
| `RockboxFFI::*Mode`      | `DspReplayGainMode`, `ReplayGainMode`, `CrossfadeMode`, `MixMode` |

Rich values (metadata, player status) cross the FFI boundary as JSON and come
back as Hashes with **symbol keys**. Sample buffers are plain `Array<Integer>`
of interleaved-stereo signed 16-bit samples.

### Two ReplayGain encodings

The DSP and player use *different* mode integers (a quirk of the C ABI):

- `Dsp#set_replaygain` → `DspReplayGainMode` (`TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3`)
- `Player#set_replaygain` → `ReplayGainMode` (`OFF=0, TRACK=1, ALBUM=2`)

Use the named constants and you won't have to remember which is which.

## Examples

```sh
ruby -Ilib examples/smoke.rb          # metadata + DSP + player checks
ruby -Ilib examples/play.rb [path]    # play a file through the output device
```

## Interactive console

```sh
bundle exec rake console        # or: ./bin/console
```

Drops into IRB with `RockboxFFI` loaded and `FIXTURE` pointing at a sample
track:

```ruby
RockboxFFI::Metadata.read(FIXTURE)[:title]        # => "Speak"
p = RockboxFFI::Player.new(volume: 0.6)
p.set_queue([FIXTURE]); p.play
p.status[:state]                                  # => "playing"
```

The console bundles `irb` + `reline`, so **Tab autocompletion** and **syntax
highlighting** work out of the box — start typing `RockboxFFI::` and press
`Tab`. Both are on by default; toggle per-session with `irb --noautocomplete`,
or persist preferences in `~/.irbrc`:

```ruby
IRB.conf[:USE_AUTOCOMPLETE] = true
IRB.conf[:USE_COLORIZE]     = true
```

> Run the console under a modern Ruby (3.x/4.x, e.g. Homebrew's
> `/opt/homebrew/opt/ruby`). macOS system Ruby 2.6 can't build `fiddle`'s
> native extension, so `bundle install` fails there.

## Test

```sh
bundle exec rake test
```
