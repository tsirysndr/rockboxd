# frozen_string_literal: true

require_relative "lib/rockbox_ffi/version"

Gem::Specification.new do |spec|
  spec.name = "rockbox_ffi"
  spec.version = RockboxFFI::VERSION
  spec.authors = ["Tsiry Sandratraina"]
  spec.email = ["tsiry.sndr@rocksky.app"]

  spec.summary = "Ruby bindings for the Rockbox DSP / metadata / playback engine"
  spec.description = "Fiddle-based bindings over the librockbox_ffi C ABI: audio " \
                     "metadata parsing, the Rockbox DSP pipeline (EQ / tone / " \
                     "surround / compressor / ReplayGain), and a queue-based player."
  spec.homepage = "https://github.com/tsirysndr/rockboxd"
  spec.license = "GPL-2.0-or-later"
  spec.required_ruby_version = ">= 2.6"

  spec.metadata["homepage_uri"] = spec.homepage
  spec.metadata["source_code_uri"] = "https://github.com/tsirysndr/rockboxd/tree/master/bindings/ruby"

  # Platform gems: set ROCKBOX_GEM_PLATFORM (e.g. arm64-darwin, x86_64-linux)
  # and stage the matching binary in vendor/ before `gem build`. The resulting
  # gem carries the native lib and is selected by RubyGems for that platform.
  # With no override this builds the source gem (no bundled binary; relies on
  # ROCKBOX_FFI_LIB or a target/release checkout at runtime).
  gem_platform = ENV.fetch("ROCKBOX_GEM_PLATFORM", "").strip
  spec.platform = gem_platform unless gem_platform.empty?

  spec.files = Dir["lib/**/*.rb", "examples/*.rb", "README.md"] +
               Dir["vendor/librockbox_ffi.{dylib,so,dll}"]
  spec.require_paths = ["lib"]

  # Fiddle is Ruby stdlib on 2.6–3.4 and a bundled gem from 3.5 on; declare it
  # so it resolves everywhere. JSON is a default gem (no explicit dep needed).
  spec.add_dependency "fiddle", ">= 1.0"
end
