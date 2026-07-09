;; Read an audio file's tags and probe a codec from a filename extension.
;;
;; Run: clojure -M:examples examples/metadata.clj /path/to/song.flac
(require '[rockbox.ffi.metadata :as metadata])

(let [path (or (first *command-line-args*) "song.flac")]
  (println "probe" (pr-str path) "->" (metadata/probe path))
  (when (.exists (java.io.File. path))
    (let [m (metadata/read path)]
      (println "title:    " (:title m))
      (println "artist:   " (:artist m))
      (println "album:    " (:album m))
      (println "codec:    " (:codec m))
      (println "duration: " (:duration_ms m) "ms")
      (println "bitrate:  " (:bitrate m) "kbps")
      (println "rate:     " (:sample_rate m) "Hz")
      (println "replaygain:" (:replaygain m)))))
