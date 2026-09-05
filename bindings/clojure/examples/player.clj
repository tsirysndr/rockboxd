;; Queue a track, start playback, and print live status. Opens a real output
;; device, so run it where audio output exists. The source can be a local
;; file, a remote http(s) file, an internet-radio stream, or an HLS (.m3u8) /
;; MPEG-DASH (.mpd) manifest — the engine detects each kind automatically.
;;
;; Run: clojure -M:examples examples/player.clj /path/to/song.flac
;;      clojure -M:examples examples/player.clj hls    ; public HLS test stream
;;      clojure -M:examples examples/player.clj dash   ; public MPEG-DASH test stream
(require '[rockbox.ffi.player :as player])

;; Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
;; for more).
(def hls-default "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8")
(def dash-default "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd")

(let [path (case (first *command-line-args*)
             "hls"  hls-default
             "dash" dash-default
             (first *command-line-args*))]
  (player/with-player [p {:volume 0.8}]
    (println "sample-rate:" (player/sample-rate p) "Hz")
    (if path
      (do
        ;; Thread the setup through `p`: queue, DSP (Bass Boost preset + a
        ;; +7 dB bass / +4 dB treble lift), then play — every mutator returns `p`.
        (-> p
            (player/set-queue [path])
            (player/set-eq-preset :bass-boost)
            (player/set-bass 7)
            (player/set-treble 4)
            (player/play))
        (println "playing" path)
        (println "eq: BassBoost preset, bass +7 dB, treble +4 dB")
        ;; A live stream reports duration 0 (shown as LIVE) and plays until
        ;; Ctrl-C.
        (dotimes [_ 5]
          (Thread/sleep 1000)
          (let [s (player/status p)
                dur (long (:duration_ms s 0))]
            (println (format "[%s] %d ms / %s"
                             (:state s) (:position_ms s 0)
                             (if (zero? dur) "LIVE" (str dur " ms"))))))
        (player/stop p))
      (do
        (player/set-queue p ["/music/a.flac" "/music/b.flac"])
        (Thread/sleep 100)
        (println "status:" (player/status p))
        (println "(pass an audio path to actually play)")))))
