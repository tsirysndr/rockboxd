# frozen_string_literal: true

require "json"

module RockboxFFI
  # Audio file metadata parsing.
  module Metadata
    module_function

    # Parse the metadata of the audio file at +path+.
    #
    # Returns a Hash (symbol keys) with tag strings, +:duration_ms+,
    # +:bitrate+, +:sample_rate+, a nested +:replaygain+ Hash (including the
    # raw Q7.24 +raw_*+ fields), and optional +:album_art+ / +:cuesheet+
    # locations. Raises on failure.
    def read(path)
      s = RockboxFFI.take_string(Lib.rb_meta_read_json(path.to_s))
      raise "could not read metadata from #{path.inspect}" if s.nil?

      JSON.parse(s, symbolize_names: true)
    end

    # Guess the codec label (e.g. "FLAC") from a filename's extension without
    # opening the file. Returns nil for an unknown extension.
    def probe(filename)
      RockboxFFI.take_string(Lib.rb_meta_probe(filename.to_s))
    end
  end
end
