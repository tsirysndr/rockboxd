# frozen_string_literal: true

# Fiddle (Ruby stdlib) loader for librockbox_ffi.
#
# Declares exactly the functions we call and dlopens the prebuilt shared
# library — no C is compiled here. The library is located via the
# ROCKBOX_FFI_LIB env var, then by walking up to the repo's target/release
# directory. Mirrors include/rockbox_ffi.h — keep in sync with the C ABI.

require "fiddle"
require "fiddle/import"

module RockboxFFI
  # Locate the prebuilt shared library. Precedence:
  #   1. ROCKBOX_FFI_LIB env var (explicit override)
  #   2. the platform binary bundled in the gem's vendor/ dir (published gems)
  #   3. target/release, by walking up from here (repo checkout / dev)
  def self.library_path
    names = %w[librockbox_ffi.dylib librockbox_ffi.so rockbox_ffi.dll]
    tried = []

    if (env = ENV["ROCKBOX_FFI_LIB"])
      tried << env
      return env if File.exist?(env)
    end

    # Bundled binary (platform gems ship one under <gem_root>/vendor/).
    vendor = File.expand_path("../../vendor", __dir__)
    names.each do |name|
      cand = File.join(vendor, name)
      tried << cand
      return cand if File.exist?(cand)
    end

    # Walk up from this file looking for target/release (repo checkout).
    dir = File.expand_path(__dir__)
    loop do
      rel = File.join(dir, "target", "release")
      if File.directory?(rel)
        names.each do |name|
          cand = File.join(rel, name)
          tried << cand
          return cand if File.exist?(cand)
        end
      end
      parent = File.dirname(dir)
      break if parent == dir

      dir = parent
    end

    raise LoadError, "could not locate librockbox_ffi shared library. Set " \
                     "ROCKBOX_FFI_LIB or run `cargo build --release -p " \
                     "rockbox-ffi`. Tried:\n  #{tried.join("\n  ")}"
  end

  # Thin Fiddle wrappers over the C ABI. Every function is a module method
  # (e.g. `Lib.rb_dsp_new`). Opaque handles and char*/int16* returns come back
  # as Fiddle::Pointer; scalars come back as Integer/Float.
  module Lib
    extend Fiddle::Importer
    dlload RockboxFFI.library_path

    # Fixed-width C types → Fiddle's native aliases (64-bit LP64 assumed).
    typealias "uint32_t", "unsigned int"
    typealias "int32_t", "int"
    typealias "int16_t", "short"
    typealias "int64_t", "long long"
    typealias "uint64_t", "unsigned long long"
    typealias "size_t", "unsigned long"
    typealias "bool", "int" # passed as 0/1 in a full register on SysV/AAPCS

    extern "uint32_t rb_ffi_abi_version()"

    extern "void rb_string_free(void*)"
    extern "void rb_buffer_free(void*, size_t)"

    # ---- DSP ----------------------------------------------------------
    extern "void* rb_dsp_new(uint32_t)"
    extern "void rb_dsp_free(void*)"
    extern "void rb_dsp_set_input_frequency(void*, uint32_t)"
    extern "void rb_dsp_flush(void*)"
    extern "void rb_dsp_eq_enable(void*, bool)"
    extern "void rb_dsp_set_tone(void*, int32_t, int32_t)"
    extern "void rb_dsp_set_tone_cutoffs(void*, int32_t, int32_t)"
    extern "void rb_dsp_set_surround(void*, int32_t, int32_t, int32_t, int32_t)"
    extern "void rb_dsp_set_channel_config(void*, int32_t)"
    extern "void rb_dsp_set_stereo_width(void*, int32_t)"
    extern "void rb_dsp_set_compressor(void*, int32_t, int32_t, int32_t, int32_t, int32_t, int32_t)"
    extern "void rb_dsp_set_replaygain(void*, int32_t, bool, float)"
    extern "void rb_dsp_set_replaygain_gains(void*, float, float, float, float)"
    extern "void rb_dsp_set_replaygain_gains_raw(void*, int64_t, int64_t, int64_t, int64_t)"
    extern "void rb_dsp_set_eq_band(void*, size_t, int32_t, float, float)"
    extern "void rb_dsp_set_eq_precut(void*, float)"
    extern "void* rb_dsp_process(void*, void*, size_t, void*)"

    # ---- metadata -----------------------------------------------------
    extern "void* rb_meta_read_json(void*)"
    extern "void* rb_meta_probe(void*)"

    # ---- player -------------------------------------------------------
    extern "void* rb_player_new()"
    extern "void* rb_player_new_with_config(uint32_t, float, float, int32_t, float, bool, int32_t, uint32_t, uint32_t, uint32_t, uint32_t, int32_t)"
    extern "void* rb_player_new_with_config_ex(uint32_t, float, float, int32_t, float, bool, int32_t, uint32_t, uint32_t, uint32_t, uint32_t, int32_t, void*, uint32_t)"
    extern "void rb_player_free(void*)"
    extern "void rb_player_set_queue_json(void*, void*)"
    extern "void rb_player_enqueue(void*, void*)"
    extern "void rb_player_insert_json(void*, void*, int32_t, size_t)"
    extern "void* rb_player_queue_json(void*)"
    extern "void rb_player_play(void*)"
    extern "void rb_player_pause(void*)"
    extern "void rb_player_toggle(void*)"
    extern "void rb_player_stop(void*)"
    extern "void rb_player_next(void*)"
    extern "void rb_player_previous(void*)"
    extern "void rb_player_skip_to(void*, size_t)"
    extern "void rb_player_seek_ms(void*, uint64_t)"
    extern "void rb_player_set_volume(void*, float)"
    extern "void rb_player_set_crossfade(void*, int32_t, uint32_t, uint32_t, uint32_t, uint32_t, int32_t)"
    extern "void rb_player_set_replaygain(void*, int32_t, float, bool)"
    extern "float rb_player_volume(void*)"
    extern "uint32_t rb_player_sample_rate(void*)"
    extern "void* rb_player_status_json(void*)"

    # ---- player DSP ---------------------------------------------------
    extern "void rb_player_set_eq_enabled(void*, bool)"
    extern "bool rb_player_is_eq_enabled(void*)"
    extern "void rb_player_set_eq_band(void*, size_t, int32_t, float, float)"
    extern "void rb_player_set_eq_precut(void*, float)"
    extern "void rb_player_set_eq_preset(void*, int32_t)"
    extern "void rb_player_set_tone(void*, int32_t, int32_t, int32_t, int32_t)"
    extern "void rb_player_set_bass(void*, int32_t)"
    extern "void rb_player_set_treble(void*, int32_t)"
    extern "void rb_player_set_surround(void*, int32_t, int32_t, int32_t, int32_t)"
    extern "void rb_player_set_channel_mode(void*, int32_t)"
    extern "void rb_player_set_stereo_width(void*, int32_t)"
    extern "void rb_player_set_compressor(void*, int32_t, int32_t, int32_t, int32_t, int32_t, int32_t)"
    extern "void rb_player_set_dither(void*, bool)"
    extern "void rb_player_set_pitch(void*, int32_t)"
    extern "void* rb_player_dsp_settings_json(void*)"

    # ---- resume -------------------------------------------------------
    extern "void* rb_player_resume(void*)"
    extern "void rb_player_save_resume(void*)"
    extern "void rb_player_clear_resume(void*)"
    extern "void* rb_load_resume_json(void*)"

    # ---- m3u / m3u8 playlists -----------------------------------------
    extern "void* rb_player_import_m3u(void*, void*, int32_t, size_t)"
    extern "void* rb_player_load_m3u(void*, void*)"
    extern "int32_t rb_player_export_m3u(void*, void*)"
    extern "void* rb_m3u_read_json(void*)"
    extern "int32_t rb_m3u_write_json(void*, void*)"
    extern "bool rb_is_url(void*)"
  end

  # true/false → 1/0 for the ABI's `bool` (declared as int above).
  def self.b(flag)
    flag ? 1 : 0
  end

  # Copy a heap C string returned by the ABI into a String, then free it.
  # Returns nil for a NULL pointer (the ABI's error/absent signal).
  def self.take_string(ptr)
    return nil if ptr.null?

    begin
      ptr.to_s.force_encoding("UTF-8")
    ensure
      Lib.rb_string_free(ptr)
    end
  end
end
