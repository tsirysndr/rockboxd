# frozen_string_literal: true

require "json"

module RockboxFFI
  # Decode a local audio file to interleaved-stereo S16 PCM, one chunk at a
  # time, using the Rockbox codec engine.
  #
  # The codec state is process-wide, so only one Decoder may decode at a time
  # and it must be used from one thread. Call #close when done, or use the block
  # form Decoder.open(path) { |dec| ... }.
  class Decoder
    # Open a Decoder for +path+; if a block is given, close it automatically
    # afterwards.
    def self.open(path)
      dec = new(path)
      return dec unless block_given?

      begin
        yield dec
      ensure
        dec.close
      end
    end

    def initialize(path)
      @ptr = Lib.rb_decoder_open(path.to_s)
      raise "could not open decoder for #{path.inspect}" if @ptr.null?

      @finalizer = self.class.send(:finalizer, @ptr)
      ObjectSpace.define_finalizer(self, @finalizer)
    end

    # -- lifecycle --------------------------------------------------------
    def close
      return if @ptr.nil?

      ObjectSpace.undefine_finalizer(self)
      Lib.rb_decoder_free(@ptr)
      @ptr = nil
    end

    def closed?
      @ptr.nil?
    end

    def self.finalizer(ptr)
      proc { Lib.rb_decoder_free(ptr) }
    end
    private_class_method :finalizer

    # -- metadata ---------------------------------------------------------
    # Tags and stream properties (same shape as Metadata.read). Returns a Hash
    # with symbol keys. Raises on failure.
    def metadata
      s = RockboxFFI.take_string(Lib.rb_decoder_metadata_json(@ptr))
      raise "rb_decoder_metadata_json returned NULL" if s.nil?

      JSON.parse(s, symbolize_names: true)
    end

    # -- decoding ---------------------------------------------------------
    # Next buffer of decoded PCM as +[samples, sample_rate]+, where +samples+
    # is an Array of interleaved stereo int16 Integers. Returns nil at end of
    # track (or after a codec error — see #finished).
    def next_chunk
      out_len = "\0" * Fiddle::SIZEOF_VOIDP      # size_t* out-param (i16 count)
      out_rate = "\0" * Fiddle::SIZEOF_INT       # uint32_t* out-param (Hz)
      ptr = Lib.rb_decoder_next_chunk(@ptr, out_len, out_rate)

      produced = out_len.unpack1(Fiddle::SIZEOF_VOIDP == 8 ? "Q" : "L")
      return nil if ptr.null? || produced.zero?

      rate = out_rate.unpack1("L")
      begin
        ptr.size = produced * 2
        [ptr[0, produced * 2].unpack("s<*"), rate]
      ensure
        Lib.rb_buffer_free(ptr, produced)
      end
    end

    # Yield each +[samples, sample_rate]+ chunk until end of track. Returns an
    # Enumerator when no block is given.
    def each_chunk
      return enum_for(:each_chunk) unless block_given?

      while (chunk = next_chunk)
        yield chunk
      end
    end

    # Request a seek to +ms+ milliseconds.
    def seek_ms(ms)
      Lib.rb_decoder_seek_ms(@ptr, Integer(ms))
    end

    # Playback position last reported by the codec, in milliseconds.
    def elapsed_ms
      Lib.rb_decoder_elapsed_ms(@ptr)
    end

    # +[done, code]+: +done+ is true once the track has ended; +code+ is the
    # exit status (0 = clean end, negative = codec error), valid only when
    # +done+.
    def finished
      out_code = "\0" * Fiddle::SIZEOF_INT       # int32_t* out-param
      r = Lib.rb_decoder_finished(@ptr, out_code)
      done = r == true || r == 1
      [done, out_code.unpack1("l")]
    end
  end
end
