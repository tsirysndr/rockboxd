(ns rockbox.ffi.play
  "Play an audio source through the real output device.

  The queue entry can be a local file, a remote http(s) file, an
  internet-radio stream, or an HLS (.m3u8) / MPEG-DASH (.mpd) manifest —
  the engine detects each kind automatically.

  Run: mise exec -- clojure -M:play [/path/to/audio-or-URL]
       mise exec -- clojure -M:play hls    ; public HLS test stream
       mise exec -- clojure -M:play dash   ; public MPEG-DASH test stream"
  (:require [rockbox.ffi.player :as player])
  (:import [java.io File]))

;; Public adaptive-streaming test streams (see crates/rockbox-playback/README.md
;; for more).
(def ^:private hls-default "https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8")
(def ^:private dash-default "https://dash.akamaized.net/akamai/bbb_30fps/bbb_30fps.mpd")

(defn- repo-root ^File []
  (loop [dir (.getAbsoluteFile (File. (System/getProperty "user.dir")))]
    (cond
      (nil? dir) (throw (ex-info "could not find repo root (crates/rocksky/fixtures)" {}))
      (.isDirectory (File. dir "crates/rocksky/fixtures")) dir
      :else (recur (.getParentFile dir)))))

(defn -main [& args]
  (let [arg (or (first args)
                (.getPath (File. (repo-root) "crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a")))
        file (case arg
               "hls"  hls-default
               "dash" dash-default
               arg)]
    (try
      (let [p (player/new-player {:volume 0.8})]
        (player/set-queue p [file])
        (player/play p)
        (println "▶ playing" file)
        ;; A live stream reports duration 0 and plays until Ctrl-C.
        (loop []
          (let [st (player/status p)
                pos (/ (long (:position_ms st 0)) 1000.0)
                dur (/ (long (:duration_ms st 0)) 1000.0)
                state (:state st "?")
                clock (if (zero? dur)
                        (format "%.1fs / LIVE" pos)
                        (format "%.1fs / %.1fs" pos dur))]
            (print (format "\r[%s] %s   " state clock))
            (flush)
            (if (and (= state "stopped") (pos? (long (:position_ms st 0))))
              (println "\n✔ done")
              (do (Thread/sleep 500) (recur)))))
        (System/exit 0))
      (catch Exception e
        (binding [*out* *err*] (println "error:" e))
        (System/exit 1)))))
