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
end
