# frozen_string_literal: true

module RockboxFFI
  # The DSP pipeline: EQ, tone, surround, compressor, ReplayGain, resampler.
  #
  # The underlying dsp_config is a process-wide singleton, so only one Dsp may
  # exist at a time and it must be used from one thread. Call #close when done,
  # or use the block form Dsp.open(rate) { |dsp| ... }.
  class Dsp
    # Open a Dsp; if a block is given, close it automatically afterwards.
    def self.open(sample_rate)
      dsp = new(sample_rate)
      return dsp unless block_given?

      begin
        yield dsp
      ensure
        dsp.close
      end
    end

    def initialize(sample_rate)
      @ptr = Lib.rb_dsp_new(Integer(sample_rate))
      raise "rb_dsp_new returned NULL" if @ptr.null?

      @finalizer = self.class.send(:finalizer, @ptr)
      ObjectSpace.define_finalizer(self, @finalizer)
    end

    # -- lifecycle --------------------------------------------------------
    def close
      return if @ptr.nil?

      ObjectSpace.undefine_finalizer(self)
      Lib.rb_dsp_free(@ptr)
      @ptr = nil
    end

    def closed?
      @ptr.nil?
    end

    def self.finalizer(ptr)
      proc { Lib.rb_dsp_free(ptr) }
    end
    private_class_method :finalizer

    # -- configuration ----------------------------------------------------
    def set_input_frequency(hz)
      Lib.rb_dsp_set_input_frequency(@ptr, Integer(hz))
    end

    def flush
      Lib.rb_dsp_flush(@ptr)
    end

    def eq_enable(enable)
      Lib.rb_dsp_eq_enable(@ptr, RockboxFFI.b(enable))
    end

    # Configure one EQ band (0..=9). Band 0 low shelf, 9 high shelf.
    def set_eq_band(band, cutoff_hz, q, gain_db)
      Lib.rb_dsp_set_eq_band(@ptr, Integer(band), Integer(cutoff_hz), Float(q), Float(gain_db))
    end

    def set_eq_precut(db)
      Lib.rb_dsp_set_eq_precut(@ptr, Float(db))
    end

    def set_tone(bass_db, treble_db)
      Lib.rb_dsp_set_tone(@ptr, Integer(bass_db), Integer(treble_db))
    end

    def set_tone_cutoffs(bass_hz, treble_hz)
      Lib.rb_dsp_set_tone_cutoffs(@ptr, Integer(bass_hz), Integer(treble_hz))
    end

    def set_surround(delay_ms, balance, fx1, fx2)
      Lib.rb_dsp_set_surround(@ptr, Integer(delay_ms), Integer(balance), Integer(fx1), Integer(fx2))
    end

    def set_channel_config(mode)
      Lib.rb_dsp_set_channel_config(@ptr, Integer(mode))
    end

    def set_stereo_width(percent)
      Lib.rb_dsp_set_stereo_width(@ptr, Integer(percent))
    end

    def set_compressor(threshold, makeup_gain, ratio, knee, release_time, attack_time)
      Lib.rb_dsp_set_compressor(
        @ptr, Integer(threshold), Integer(makeup_gain), Integer(ratio),
        Integer(knee), Integer(release_time), Integer(attack_time)
      )
    end

    # mode: see DspReplayGainMode (TRACK=0, ALBUM=1, SHUFFLE=2, OFF=3).
    def set_replaygain(mode, noclip, preamp_db)
      Lib.rb_dsp_set_replaygain(@ptr, Integer(mode), RockboxFFI.b(noclip), Float(preamp_db))
    end

    # Per-track gains in plain dB / peaks as linear amplitude (1.0 = full
    # scale). nil for any absent tag (mapped to the ABI's NaN sentinel).
    def set_replaygain_gains(track_gain_db: nil, album_gain_db: nil, track_peak: nil, album_peak: nil)
      Lib.rb_dsp_set_replaygain_gains(
        @ptr, opt(track_gain_db), opt(album_gain_db), opt(track_peak), opt(album_peak)
      )
    end

    # Native Q7.24 linear factors (the raw_* fields from Metadata.read),
    # 0 = not tagged.
    def set_replaygain_gains_raw(track_gain, album_gain, track_peak, album_peak)
      Lib.rb_dsp_set_replaygain_gains_raw(
        @ptr, Integer(track_gain), Integer(album_gain), Integer(track_peak), Integer(album_peak)
      )
    end

    # -- processing -------------------------------------------------------
    # Run interleaved stereo S16 +samples+ (an Array of Integers) through the
    # pipeline. Returns a new Array of processed samples (length may differ
    # from the input — the resampler buffers).
    def process(samples)
      raise ArgumentError, "input must be interleaved stereo (even length)" if samples.length.odd?

      input = samples.pack("s<*")               # int16 little-endian
      out_len = "\0" * Fiddle::SIZEOF_VOIDP      # size_t* out-param (written in place)
      out_ptr = Lib.rb_dsp_process(@ptr, input, samples.length, out_len)

      produced = out_len.unpack1(Fiddle::SIZEOF_VOIDP == 8 ? "Q" : "L")
      return [] if out_ptr.null? || produced.zero?

      begin
        out_ptr.size = produced * 2
        out_ptr[0, produced * 2].unpack("s<*")
      ensure
        Lib.rb_buffer_free(out_ptr, produced)
      end
    end

    private

    NAN = Float::NAN
    private_constant :NAN

    # nil -> NaN, the ABI's "tag absent" sentinel.
    def opt(value)
      value.nil? ? NAN : Float(value)
    end
  end

  # Generate +seconds+ of a sine as interleaved stereo int16 (an Array).
  def self.sine_stereo(freq_hz, seconds, rate, amplitude = 16_000)
    n = (seconds * rate).to_i
    buf = Array.new(n * 2)
    (0...n).each do |i|
      s = (Math.sin(i * 2.0 * Math::PI * freq_hz / rate) * amplitude).to_i
      s = -32_768 if s < -32_768
      s = 32_767 if s > 32_767
      buf[i * 2] = s
      buf[(i * 2) + 1] = s
    end
    buf
  end
end
