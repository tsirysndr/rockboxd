(ns rockbox.ffi.decoder
  "Decode a local audio file to interleaved-stereo S16 PCM, one chunk at a time.

  Wraps the Rockbox codec engine. The codec state is process-wide, so only one
  decoder may decode at a time — opening a second one blocks until the first is
  freed. A handle is an opaque MemorySegment; every function takes it as the
  first argument. Free it with `free` (or use `with-decoder`)."
  (:refer-clojure :exclude [next])
  (:require [rockbox.ffi :as ffi]
            [clojure.data.json :as json])
  (:import [java.lang.foreign Arena MemorySegment ValueLayout]))

(defn open
  "Open a streaming decoder for the audio file at `path`."
  ^MemorySegment [path]
  (let [p ^MemorySegment (with-open [a (Arena/ofConfined)]
                           (ffi/call :decoder-open (.allocateFrom a ^String path)))]
    (when (zero? (.address p))
      (throw (ex-info (str "could not open decoder for " path) {:path path})))
    p))

(defn free
  "Free the native handle. Safe to call with nil."
  [^MemorySegment p]
  (when p (ffi/call :decoder-free p)))

(defmacro with-decoder
  "Bind `sym` to a fresh decoder for `path`, run `body`, then free it."
  [[sym path] & body]
  `(let [~sym (open ~path)]
     (try ~@body (finally (free ~sym)))))

;; ---- metadata ---------------------------------------------------------

(defn metadata
  "Tags and stream properties (same shape as `rockbox.ffi.metadata/read`),
  as a map with keyword keys."
  [^MemorySegment p]
  (let [s (ffi/take-string (ffi/call :decoder-metadata-json p))]
    (when-not s
      (throw (ex-info "rb_decoder_metadata_json returned NULL" {})))
    (json/read-str s :key-fn keyword)))

;; ---- decoding ---------------------------------------------------------

(defn next-chunk
  "Next buffer of decoded PCM as `[samples sample-rate]`, where `samples` is an
  interleaved stereo short-array. Returns nil at end of track (or after a codec
  error — see `finished`)."
  [^MemorySegment p]
  (with-open [a (Arena/ofConfined)]
    (let [out-len (.allocate a ValueLayout/JAVA_LONG)
          out-rate (.allocate a ValueLayout/JAVA_INT)
          out-ptr ^MemorySegment (ffi/call :decoder-next-chunk p out-len out-rate)
          cnt (.get out-len ValueLayout/JAVA_LONG 0)]
      (when-not (or (zero? (.address out-ptr)) (<= cnt 0))
        (let [out (.reinterpret out-ptr (* cnt 2))
              result (short-array cnt)]
          (MemorySegment/copy out ValueLayout/JAVA_SHORT 0 result 0 (int cnt))
          (ffi/call :buffer-free out-ptr (long cnt))
          [result (.get out-rate ValueLayout/JAVA_INT 0)])))))

(defn seek-ms
  "Request a seek to `ms` milliseconds."
  [^MemorySegment p ms]
  (ffi/call :decoder-seek-ms p (long ms)))

(defn elapsed-ms
  "Playback position last reported by the codec, in milliseconds."
  [^MemorySegment p]
  (ffi/call :decoder-elapsed-ms p))

(defn finished
  "`[done code]`: `done` is true once the track has ended; `code` is the exit
  status (0 = clean end, negative = codec error), valid only when `done`."
  [^MemorySegment p]
  (with-open [a (Arena/ofConfined)]
    (let [out-code (.allocate a ValueLayout/JAVA_INT)
          done (ffi/call :decoder-finished p out-code)]
      [done (.get out-code ValueLayout/JAVA_INT 0)])))
