;; Queue a track, start playback, and print live status. Opens a real output
;; device, so run it where audio output exists.
;;
;; Run: clojure -M:examples examples/player.clj /path/to/song.flac
(require '[rockbox.ffi.player :as player])

(let [path (first *command-line-args*)]
  (player/with-player [p {:volume 0.8}]
    (println "sample-rate:" (player/sample-rate p) "Hz")
    (if path
      (do
        (player/set-queue p [path])
        (player/play p)
        (println "playing" path)
        (dotimes [_ 5]
          (Thread/sleep 1000)
          (let [s (player/status p)]
            (println (format "[%s] %d / %d ms"
                             (:state s) (:position_ms s 0) (:duration_ms s 0)))))
        (player/stop p))
      (do
        (player/set-queue p ["/music/a.flac" "/music/b.flac"])
        (Thread/sleep 100)
        (println "status:" (player/status p))
        (println "(pass an audio path to actually play)")))))
