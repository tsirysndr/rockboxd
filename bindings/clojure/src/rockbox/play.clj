(ns rockbox.play
  "Play an audio file through the real output device.

  Run: mise exec -- clojure -M:play [/path/to/audio]"
  (:require [rockbox.player :as player])
  (:import [java.io File]))

(defn- repo-root ^File []
  (loop [dir (.getAbsoluteFile (File. (System/getProperty "user.dir")))]
    (cond
      (nil? dir) (throw (ex-info "could not find repo root (crates/rocksky/fixtures)" {}))
      (.isDirectory (File. dir "crates/rocksky/fixtures")) dir
      :else (recur (.getParentFile dir)))))

(defn -main [& args]
  (let [file (or (first args)
                 (.getPath (File. (repo-root) "crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a")))]
    (try
      (let [p (player/new-player {:volume 0.8})]
        (player/set-queue p [file])
        (player/play p)
        (println "▶ playing" file)
        (loop []
          (let [st (player/status p)
                pos (/ (long (:position_ms st 0)) 1000.0)
                dur (/ (long (:duration_ms st 0)) 1000.0)
                state (:state st "?")]
            (print (format "\r[%s] %.1fs / %.1fs   " state pos dur))
            (flush)
            (if (and (= state "stopped") (pos? (long (:position_ms st 0))))
              (println "\n✔ done")
              (do (Thread/sleep 500) (recur)))))
        (System/exit 0))
      (catch Exception e
        (binding [*out* *err*] (println "error:" e))
        (System/exit 1)))))
