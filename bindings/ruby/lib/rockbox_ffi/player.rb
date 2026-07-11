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
    def set_queue(paths)
      Lib.rb_player_set_queue_json(@ptr, JSON.generate(Array(paths).map(&:to_s)))
    end

    def enqueue(path)
      Lib.rb_player_enqueue(@ptr, path.to_s)
    end

    # Insert +paths+ (a path/URL or Array of them) into the queue at
    # +position+ (see InsertPosition). +index+ is only used when position is
    # InsertPosition::INDEX (7).
    def insert(paths, position, index = 0)
      Lib.rb_player_insert_json(
        @ptr, JSON.generate(Array(paths).map(&:to_s)), Integer(position), Integer(index)
      )
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
    end

    def pause
      Lib.rb_player_pause(@ptr)
    end

    def toggle
      Lib.rb_player_toggle(@ptr)
    end

    def stop
      Lib.rb_player_stop(@ptr)
    end

    def next
      Lib.rb_player_next(@ptr)
    end

    def previous
      Lib.rb_player_previous(@ptr)
    end

    def skip_to(index)
      Lib.rb_player_skip_to(@ptr, Integer(index))
    end

    def seek_ms(ms)
      Lib.rb_player_seek_ms(@ptr, Integer(ms))
    end

    # -- settings ---------------------------------------------------------
    def set_volume(vol)
      Lib.rb_player_set_volume(@ptr, Float(vol))
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
    end

    # mode: see ReplayGainMode (OFF=0, TRACK=1, ALBUM=2).
    def set_replaygain(mode, preamp_db, prevent_clipping)
      Lib.rb_player_set_replaygain(@ptr, Integer(mode), Float(preamp_db), RockboxFFI.b(prevent_clipping))
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
