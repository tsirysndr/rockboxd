(ns rockbox.ffi.metadata
  "Audio file metadata parsing."
  (:refer-clojure :exclude [read])
  (:require [rockbox.ffi :as ffi]
            [clojure.data.json :as json])
  (:import [java.lang.foreign Arena]))

(defn read
  "Parse the metadata of the audio file at `path`.

  Returns a map (keyword keys) with tag strings, :duration_ms, :bitrate,
  :sample_rate, a nested :replaygain map (including the raw Q7.24 :raw_* fields),
  and optional :album_art / :cuesheet locations. Throws on failure."
  [path]
  (let [raw (with-open [a (Arena/ofConfined)]
              (ffi/call :meta-read-json (.allocateFrom a ^String path)))
        s (ffi/take-string raw)]
    (when-not s
      (throw (ex-info (str "rb_meta_read_json(" path ") returned NULL") {:path path})))
    (json/read-str s :key-fn keyword)))

(defn probe
  "Guess the codec label (e.g. \"FLAC\") from a filename's extension without
  opening the file. Returns nil for an unknown extension."
  [filename]
  (with-open [a (Arena/ofConfined)]
    (ffi/take-string (ffi/call :meta-probe (.allocateFrom a ^String filename)))))
