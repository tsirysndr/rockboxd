# frozen_string_literal: true

# Ruby bindings for the Rockbox DSP / metadata / playback engine.
#
# Thin Fiddle wrappers over the librockbox_ffi C ABI. See RockboxFFI::Metadata,
# RockboxFFI::Dsp, RockboxFFI::Player, and RockboxFFI::Decoder.

require "rockbox_ffi/version"
require "rockbox_ffi/ffi"
require "rockbox_ffi/enums"
require "rockbox_ffi/metadata"
require "rockbox_ffi/dsp"
require "rockbox_ffi/player"
require "rockbox_ffi/decoder"

require "json"

module RockboxFFI
  # ABI major version of the loaded library (bumped on breaking changes).
  def self.abi_version
    Lib.rb_ffi_abi_version
  end

  # Peek at a resume file without a Player. Returns a Hash
  # {tracks:, index:, elapsed_ms:} or nil if the file is absent/invalid.
  def self.load_resume(path)
    s = take_string(Lib.rb_load_resume_json(path.to_s))
    return nil if s.nil?

    JSON.parse(s, symbolize_names: true)
  end

  # Parse a playlist (.m3u / .m3u8) into an Array of Hashes
  # {path:, duration_ms:, title:}. Returns nil on failure.
  def self.m3u_read(path)
    s = take_string(Lib.rb_m3u_read_json(path.to_s))
    return nil if s.nil?

    JSON.parse(s, symbolize_names: true)
  end

  # Write +paths+ (a path/URL or Array of them) as an .m3u8. Returns true on
  # success.
  def self.m3u_write(path, paths)
    Lib.rb_m3u_write_json(path.to_s, JSON.generate(Array(paths).map(&:to_s))).zero?
  end

  # Whether +s+ looks like an http(s):// URL.
  def self.is_url?(s)
    r = Lib.rb_is_url(s.to_s)
    r == true || r == 1
  end
end
