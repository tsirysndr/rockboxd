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

  spec.files = Dir["lib/**/*.rb", "examples/*.rb", "README.md"]
  spec.require_paths = ["lib"]

  # Fiddle is Ruby stdlib on 2.6–3.4 and a bundled gem from 3.5 on; declare it
  # so it resolves everywhere. JSON is a default gem (no explicit dep needed).
  spec.add_dependency "fiddle", ">= 1.0"
end
