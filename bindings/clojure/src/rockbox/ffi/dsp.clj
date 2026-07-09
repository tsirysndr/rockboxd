(ns rockbox.ffi.dsp
  "The DSP pipeline: EQ, tone, surround, compressor, ReplayGain, resampler.

  The underlying dsp_config is a process-wide singleton, so only one DSP handle
  may exist at a time and it must be used from one thread. A handle is an opaque
  MemorySegment; every function takes it as the first argument. Free it with
  `free` (or use `with-dsp`)."
  (:refer-clojure :exclude [flush])
  (:require [rockbox.ffi :as ffi]
            [rockbox.ffi.enums :as enums])
  (:import [java.lang.foreign Arena MemorySegment ValueLayout]))

(defn new-dsp
  "Create a DSP pipeline for the given input sample rate (Hz)."
  ^MemorySegment [sample-rate]
  (let [p ^MemorySegment (ffi/call :dsp-new (int sample-rate))]
    (when (zero? (.address p))
      (throw (ex-info "rb_dsp_new returned NULL" {})))
    p))

(defn free
  "Free the native handle. Safe to call with nil."
  [^MemorySegment p]
  (when p (ffi/call :dsp-free p)))

(defmacro with-dsp
  "Bind `sym` to a fresh DSP handle for `sample-rate`, run `body`, then free it."
  [[sym sample-rate] & body]
  `(let [~sym (new-dsp ~sample-rate)]
     (try ~@body (finally (free ~sym)))))

;; ---- configuration ----------------------------------------------------

(defn set-input-frequency [^MemorySegment p hz] (ffi/call :dsp-set-input-frequency p (int hz)))
(defn flush [^MemorySegment p] (ffi/call :dsp-flush p))
(defn eq-enable [^MemorySegment p enable?] (ffi/call :dsp-eq-enable p (boolean enable?)))

(defn set-eq-band
  "Configure one EQ band (0..9). Band 0 is a low shelf, 9 a high shelf."
  [^MemorySegment p band cutoff-hz q gain-db]
  (ffi/call :dsp-set-eq-band p (long band) (int cutoff-hz) (float q) (float gain-db)))

(defn set-eq-precut [^MemorySegment p db] (ffi/call :dsp-set-eq-precut p (float db)))
(defn set-tone [^MemorySegment p bass-db treble-db] (ffi/call :dsp-set-tone p (int bass-db) (int treble-db)))
(defn set-tone-cutoffs [^MemorySegment p bass-hz treble-hz] (ffi/call :dsp-set-tone-cutoffs p (int bass-hz) (int treble-hz)))

(defn set-surround [^MemorySegment p delay-ms balance fx1 fx2]
  (ffi/call :dsp-set-surround p (int delay-ms) (int balance) (int fx1) (int fx2)))

(defn set-channel-config [^MemorySegment p mode]
  (ffi/call :dsp-set-channel-config p (int (enums/code enums/channel-config mode))))

(defn set-stereo-width [^MemorySegment p percent] (ffi/call :dsp-set-stereo-width p (int percent)))

(defn set-compressor [^MemorySegment p threshold makeup-gain ratio knee release-time attack-time]
  (ffi/call :dsp-set-compressor p (int threshold) (int makeup-gain) (int ratio)
            (int knee) (int release-time) (int attack-time)))

(defn set-replaygain
  "`mode`: rockbox.ffi.enums/dsp-replaygain-mode (:track :album :shuffle :off)."
  [^MemorySegment p mode noclip? preamp-db]
  (ffi/call :dsp-set-replaygain p (int (enums/code enums/dsp-replaygain-mode mode))
            (boolean noclip?) (float preamp-db)))

(defn set-replaygain-gains
  "Per-track gains in plain dB / peaks as linear amplitude (1.0 = full scale).
  Omit any absent tag — it defaults to the ABI's NaN sentinel."
  [^MemorySegment p & {:keys [track-gain-db album-gain-db track-peak album-peak]
                       :or {track-gain-db Float/NaN album-gain-db Float/NaN
                            track-peak Float/NaN album-peak Float/NaN}}]
  (ffi/call :dsp-set-replaygain-gains p (float track-gain-db) (float album-gain-db)
            (float track-peak) (float album-peak)))

(defn set-replaygain-gains-raw
  "Native Q7.24 linear factors (the raw_* fields from metadata), 0 = untagged."
  [^MemorySegment p track-gain album-gain track-peak album-peak]
  (ffi/call :dsp-set-replaygain-gains-raw p (long track-gain) (long album-gain)
            (long track-peak) (long album-peak)))

;; ---- processing -------------------------------------------------------

(defn process
  "Run interleaved stereo S16 `samples` (a short-array) through the pipeline.
  Returns a new short-array (length may differ — the resampler buffers)."
  ^shorts [^MemorySegment p ^shorts samples]
  (let [n (alength samples)]
    (when (odd? n) (throw (ex-info "input must be interleaved stereo (even length)" {})))
    (with-open [a (Arena/ofConfined)]
      (let [input (.allocate a ValueLayout/JAVA_SHORT (long n))
            _ (MemorySegment/copy samples 0 input ValueLayout/JAVA_SHORT 0 n)
            out-len (.allocate a ValueLayout/JAVA_LONG)
            out-ptr ^MemorySegment (ffi/call :dsp-process p input (long n) out-len)
            cnt (.get out-len ValueLayout/JAVA_LONG 0)]
        (if (or (zero? (.address out-ptr)) (<= cnt 0))
          (short-array 0)
          (let [out (.reinterpret out-ptr (* cnt 2))
                result (short-array cnt)]
            (MemorySegment/copy out ValueLayout/JAVA_SHORT 0 result 0 (int cnt))
            (ffi/call :buffer-free out-ptr (long cnt))
            result))))))

(defn sine-stereo
  "Generate `seconds` of a sine as an interleaved stereo short-array."
  (^shorts [freq-hz seconds rate] (sine-stereo freq-hz seconds rate 16000.0))
  (^shorts [freq-hz seconds rate amplitude]
   (let [n (int (* seconds rate))
         buf (short-array (* n 2))]
     (dotimes [i n]
       (let [v (* (Math/sin (/ (* i 2.0 Math/PI freq-hz) rate)) amplitude)
             s (short (max -32768 (min 32767 (int v))))]
         (aset buf (* i 2) s)
         (aset buf (inc (* i 2)) s)))
     buf)))
