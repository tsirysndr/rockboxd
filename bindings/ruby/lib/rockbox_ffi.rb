# frozen_string_literal: true

# Ruby bindings for the Rockbox DSP / metadata / playback engine.
#
# Thin Fiddle wrappers over the librockbox_ffi C ABI. See RockboxFFI::Metadata,
# RockboxFFI::Dsp, and RockboxFFI::Player.

require "rockbox_ffi/version"
require "rockbox_ffi/ffi"
require "rockbox_ffi/enums"
require "rockbox_ffi/metadata"
require "rockbox_ffi/dsp"
require "rockbox_ffi/player"

module RockboxFFI
  # ABI major version of the loaded library (bumped on breaking changes).
  def self.abi_version
    Lib.rb_ffi_abi_version
  end
end
