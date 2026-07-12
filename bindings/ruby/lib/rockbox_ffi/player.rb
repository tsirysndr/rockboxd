# frozen_string_literal: true

require "json"

module RockboxFFI
  # Queue-based player with native ReplayGain and Rockbox crossfade.
  #
  # A Player owns a live audio output device and a background engine thread —
  # construct it only where an output device exists. Call #close when done,
  # or use the block form Player.open(...) { |p| ... }.
  #
  # ReplayGain +mode+ here uses the *player* values: 0 off, 1 track, 2 album
  # (see ReplayGainMode) — distinct from the DSP encoding.
  #
  # Mutating setters (queue, transport, settings, DSP) return +self+ so calls
  # can be fluently chained, e.g.
  #   player.set_queue([file]).set_shuffle(true).play
  # Getters/queries and lifecycle methods keep their own return values.
  class Player
    DEFAULT_CONFIG = {
      sample_rate: 0, # 0 => device default
      buffer_seconds: 4.0,
      volume: 1.0,
      replaygain_mode: ReplayGainMode::OFF,
      replaygain_preamp_db: 0.0,
      replaygain_prevent_clipping: true,
      crossfade_mode: CrossfadeMode::OFF,
      fade_out_delay_ms: 0,
      fade_out_duration_ms: 2000,
      fade_in_delay_ms: 0,
      fade_in_duration_ms: 2000,
      mix_mode: MixMode::CROSSFADE,
      resume_file: nil, # an .m3u8 to auto-persist queue + position to
      resume_save_interval_ms: 0 # 0 => 5 s default
    }.freeze

    # Open a Player; if a block is given, close it automatically afterwards.
    def self.open(**opts)
      player = new(**opts)
      return player unless block_given?

      begin
        yield player
      ensure
        player.close
      end
    end

    # Player on the default device with Rockbox default settings.
    def self.default
      player = allocate
      player.send(:init_ptr, Lib.rb_player_new)
      player
    end

    # Create a player with configuration overrides (see DEFAULT_CONFIG keys).
    # sample_rate: 0 means the device default. Passing +resume_file:+ enables
    # auto-persisting the queue + exact position to that .m3u8 file.
    def initialize(**opts)
      c = DEFAULT_CONFIG.merge(opts)
      resume_file = c[:resume_file].nil? ? nil : c[:resume_file].to_s
      ptr = Lib.rb_player_new_with_config_ex(
        Integer(c[:sample_rate]), Float(c[:buffer_seconds]), Float(c[:volume]),
        Integer(c[:replaygain_mode]), Float(c[:replaygain_preamp_db]),
        RockboxFFI.b(c[:replaygain_prevent_clipping]), Integer(c[:crossfade_mode]),
        Integer(c[:fade_out_delay_ms]), Integer(c[:fade_out_duration_ms]),
        Integer(c[:fade_in_delay_ms]), Integer(c[:fade_in_duration_ms]),
        Integer(c[:mix_mode]), resume_file,
        Integer(c[:resume_save_interval_ms])
      )
      init_ptr(ptr)
    end

    # -- lifecycle --------------------------------------------------------
    def close
      return if @ptr.nil?

      ObjectSpace.undefine_finalizer(self)
      Lib.rb_player_free(@ptr)
      @ptr = nil
    end

    def closed?
      @ptr.nil?
    end

    def self.finalizer(ptr)
      proc { Lib.rb_player_free(ptr) }
    end
    private_class_method :finalizer

    # -- queue ------------------------------------------------------------
    # Replace the queue. Each entry may be a local file path, an http(s)://
    # URL to a finite remote file, or a live-radio / streaming URL.
    def set_queue(paths)
      Lib.rb_player_set_queue_json(@ptr, JSON.generate(Array(paths).map(&:to_s)))
      self
    end

    # Append one track to the queue. +path+ may be a local file path, an
    # http(s):// URL to a finite remote file, or a live-radio / streaming URL.
    def enqueue(path)
      Lib.rb_player_enqueue(@ptr, path.to_s)
      self
    end

    # Insert +paths+ (a path/URL or Array of them) into the queue at
    # +position+ (see InsertPosition). +index+ is only used when position is
    # InsertPosition::INDEX (7).
    def insert(paths, position, index = 0)
      Lib.rb_player_insert_json(
        @ptr, JSON.generate(Array(paths).map(&:to_s)), Integer(position), Integer(index)
      )
      self
    end

    # The current queue as an Array of String paths/URLs.
    def queue
      s = RockboxFFI.take_string(Lib.rb_player_queue_json(@ptr))
      return [] if s.nil?

      JSON.parse(s)
    end

    # -- transport --------------------------------------------------------
    def play
      Lib.rb_player_play(@ptr)
      self
    end

    def pause
      Lib.rb_player_pause(@ptr)
      self
    end

    def toggle
      Lib.rb_player_toggle(@ptr)
      self
    end

    def stop
      Lib.rb_player_stop(@ptr)
      self
    end

    def next
      Lib.rb_player_next(@ptr)
      self
    end

    def previous
      Lib.rb_player_previous(@ptr)
      self
    end

    def skip_to(index)
      Lib.rb_player_skip_to(@ptr, Integer(index))
      self
    end

    def seek_ms(ms)
      Lib.rb_player_seek_ms(@ptr, Integer(ms))
      self
    end

    # -- settings ---------------------------------------------------------
    def set_volume(vol)
      Lib.rb_player_set_volume(@ptr, Float(vol))
      self
    end

    def volume
      Lib.rb_player_volume(@ptr)
    end

    def sample_rate
      Lib.rb_player_sample_rate(@ptr)
    end

    def set_crossfade(mode, fade_out_delay_ms: 0, fade_out_duration_ms: 2000,
                      fade_in_delay_ms: 0, fade_in_duration_ms: 2000,
                      mix_mode: MixMode::CROSSFADE)
      Lib.rb_player_set_crossfade(
        @ptr, Integer(mode), Integer(fade_out_delay_ms), Integer(fade_out_duration_ms),
        Integer(fade_in_delay_ms), Integer(fade_in_duration_ms), Integer(mix_mode)
      )
      self
    end

    # mode: see ReplayGainMode (OFF=0, TRACK=1, ALBUM=2).
    def set_replaygain(mode, preamp_db, prevent_clipping)
      Lib.rb_player_set_replaygain(@ptr, Integer(mode), Float(preamp_db), RockboxFFI.b(prevent_clipping))
      self
    end

    # Enable/disable shuffle.
    def set_shuffle(enabled)
      Lib.rb_player_set_shuffle(@ptr, RockboxFFI.b(enabled))
      self
    end

    # Whether shuffle is currently enabled.
    def shuffle_enabled?
      !Lib.rb_player_is_shuffle_enabled(@ptr).zero?
    end

    # Set the repeat mode (see RepeatMode: OFF=0, ONE=1, ALL=2).
    def set_repeat(mode)
      Lib.rb_player_set_repeat(@ptr, Integer(mode))
      self
    end

    # The current repeat mode as an Integer (see RepeatMode).
    def repeat
      Lib.rb_player_repeat(@ptr)
    end

    # -- DSP --------------------------------------------------------------
    # Enable/disable the parametric equalizer.
    def set_eq_enabled(enabled)
      Lib.rb_player_set_eq_enabled(@ptr, RockboxFFI.b(enabled))
      self
    end

    # Whether the parametric equalizer is currently enabled.
    def eq_enabled?
      !Lib.rb_player_is_eq_enabled(@ptr).zero?
    end

    # Configure one EQ band: +band+ index, +cutoff_hz+ center frequency,
    # +q+ factor, +gain_db+ gain in dB.
    def set_eq_band(band, cutoff_hz, q, gain_db)
      Lib.rb_player_set_eq_band(@ptr, Integer(band), Integer(cutoff_hz), Float(q), Float(gain_db))
      self
    end

    # Global EQ pre-cut in dB.
    def set_eq_precut(db)
      Lib.rb_player_set_eq_precut(@ptr, Float(db))
      self
    end

    # Apply a built-in EQ preset (see EqPreset).
    def set_eq_preset(preset)
      Lib.rb_player_set_eq_preset(@ptr, Integer(preset))
      self
    end

    # Bass/treble tone controls with explicit cutoff frequencies.
    def set_tone(bass_db, treble_db, bass_cutoff_hz, treble_cutoff_hz)
      Lib.rb_player_set_tone(
        @ptr, Integer(bass_db), Integer(treble_db),
        Integer(bass_cutoff_hz), Integer(treble_cutoff_hz)
      )
      self
    end

    # Bass gain in dB.
    def set_bass(bass_db)
      Lib.rb_player_set_bass(@ptr, Integer(bass_db))
      self
    end

    # Treble gain in dB.
    def set_treble(treble_db)
      Lib.rb_player_set_treble(@ptr, Integer(treble_db))
      self
    end

    # Surround effect: delay (ms), balance, low/high cutoff frequencies (Hz).
    def set_surround(delay_ms, balance, cutoff_low_hz, cutoff_high_hz)
      Lib.rb_player_set_surround(
        @ptr, Integer(delay_ms), Integer(balance),
        Integer(cutoff_low_hz), Integer(cutoff_high_hz)
      )
      self
    end

    # Channel mode (see ChannelMode).
    def set_channel_mode(mode)
      Lib.rb_player_set_channel_mode(@ptr, Integer(mode))
      self
    end

    # Stereo width as a percentage.
    def set_stereo_width(percent)
      Lib.rb_player_set_stereo_width(@ptr, Integer(percent))
      self
    end

    # Dynamic-range compressor: threshold (dB), makeup gain, ratio, knee,
    # attack (ms), release (ms).
    def set_compressor(threshold_db, makeup_gain, ratio, knee, attack_ms, release_ms)
      Lib.rb_player_set_compressor(
        @ptr, Integer(threshold_db), Integer(makeup_gain), Integer(ratio),
        Integer(knee), Integer(attack_ms), Integer(release_ms)
      )
      self
    end

    # Enable/disable output dithering.
    def set_dither(enabled)
      Lib.rb_player_set_dither(@ptr, RockboxFFI.b(enabled))
      self
    end

    # Pitch shift ratio.
    def set_pitch(ratio)
      Lib.rb_player_set_pitch(@ptr, Integer(ratio))
      self
    end

    # A snapshot of the current DSP settings as a Hash with symbol keys.
    def dsp_settings
      s = RockboxFFI.take_string(Lib.rb_player_dsp_settings_json(@ptr))
      raise "rb_player_dsp_settings_json returned NULL" if s.nil?

      JSON.parse(s, symbolize_names: true)
    end

    # -- status -----------------------------------------------------------
    # A snapshot of the player's status as a Hash with symbol keys.
    def status
      s = RockboxFFI.take_string(Lib.rb_player_status_json(@ptr))
      raise "rb_player_status_json returned NULL" if s.nil?

      JSON.parse(s, symbolize_names: true)
    end

    # -- resume -----------------------------------------------------------
    # Restore the queue + exact position from the resume file (does NOT
    # auto-play). Returns a Hash {tracks:, index:, elapsed_ms:} or nil.
    def resume
      s = RockboxFFI.take_string(Lib.rb_player_resume(@ptr))
      return nil if s.nil?

      JSON.parse(s, symbolize_names: true)
    end

    # Force-persist the current queue + position to the resume file now.
    def save_resume
      Lib.rb_player_save_resume(@ptr)
    end

    # Delete the resume file.
    def clear_resume
      Lib.rb_player_clear_resume(@ptr)
    end

    # -- m3u / m3u8 playlists ---------------------------------------------
    # Import a playlist file into the queue at +position+ (see InsertPosition;
    # +index+ only used for INDEX). Returns the imported paths as an Array.
    def import_m3u(path, position, index = 0)
      s = RockboxFFI.take_string(
        Lib.rb_player_import_m3u(@ptr, path.to_s, Integer(position), Integer(index))
      )
      return [] if s.nil?

      JSON.parse(s)
    end

    # Replace the queue with a playlist file. Returns the loaded paths as an Array.
    def load_m3u(path)
      s = RockboxFFI.take_string(Lib.rb_player_load_m3u(@ptr, path.to_s))
      return [] if s.nil?

      JSON.parse(s)
    end

    # Export the current queue to an .m3u8 (atomic). Returns true on success.
    def export_m3u(path)
      Lib.rb_player_export_m3u(@ptr, path.to_s).zero?
    end

    private

    def init_ptr(ptr)
      raise "failed to create Player (no output device?)" if ptr.nil? || ptr.null?

      @ptr = ptr
      ObjectSpace.define_finalizer(self, self.class.send(:finalizer, ptr))
    end
  end
end
