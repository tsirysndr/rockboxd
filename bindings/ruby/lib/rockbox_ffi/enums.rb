# frozen_string_literal: true

module RockboxFFI
  # Note the two *different* ReplayGain encodings in the C ABI.

  # Values for Dsp#set_replaygain (native Rockbox).
  module DspReplayGainMode
    TRACK = 0
    ALBUM = 1
    SHUFFLE = 2
    OFF = 3
  end

  # Values for Player#set_replaygain.
  module ReplayGainMode
    OFF = 0
    TRACK = 1
    ALBUM = 2
  end

  # Values for Player#set_repeat / #repeat.
  module RepeatMode
    OFF = 0
    ONE = 1
    ALL = 2
  end

  module CrossfadeMode
    OFF = 0
    AUTO_SKIP = 1
    MANUAL_SKIP = 2
    SHUFFLE = 3
    SHUFFLE_OR_MANUAL = 4
    ALWAYS = 5
  end

  module MixMode
    CROSSFADE = 0
    MIX = 1
  end

  # Where inserted tracks land in the queue (Player#insert / #import_m3u).
  # INDEX (7) uses the explicit +index+ argument.
  module InsertPosition
    PREPEND = 0
    INSERT = 1
    INSERT_NEXT = 2
    INSERT_LAST = 3
    INSERT_SHUFFLED = 4
    INSERT_LAST_SHUFFLED = 5
    REPLACE = 6
    INDEX = 7
  end

  module ChannelConfig
    STEREO = 0
    MONO = 1
    CUSTOM = 2
    MONO_LEFT = 3
    MONO_RIGHT = 4
    KARAOKE = 5
    SWAP = 6
  end

  # Built-in EQ presets for Player#set_eq_preset.
  module EqPreset
    FLAT = 0
    ACOUSTIC = 1
    BASS_BOOST = 2
    BASS_REDUCER = 3
    CLASSICAL = 4
    DANCE = 5
    DEEP = 6
    ELECTRONIC = 7
    HIP_HOP = 8
    JAZZ = 9
    LATIN = 10
    LOUDNESS = 11
    LOUNGE = 12
    PIANO = 13
    POP = 14
    RNB = 15
    ROCK = 16
    SMALL_SPEAKERS = 17
    TREBLE_BOOST = 18
    TREBLE_REDUCER = 19
    VOCAL_BOOST = 20
  end

  # Channel mode for Player#set_channel_mode.
  module ChannelMode
    STEREO = 0
    MONO = 1
    CUSTOM = 2
    MONO_LEFT = 3
    MONO_RIGHT = 4
    KARAOKE = 5
    SWAP = 6
  end
end
