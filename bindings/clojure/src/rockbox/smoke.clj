(ns rockbox.smoke
  "End-to-end smoke test for the Clojure bindings.

  Run: mise exec -- clojure -M:smoke"
  (:require [rockbox.ffi :as ffi]
            [rockbox.metadata :as metadata]
            [rockbox.dsp :as dsp]
            [rockbox.player :as player])
  (:import [java.io File]))

(defn- fail [msg]
  (binding [*out* *err*] (println (str "SMOKE FAILED: " msg)))
  (System/exit 1))

(defn- repo-root ^File []
  (loop [dir (.getAbsoluteFile (File. (System/getProperty "user.dir")))]
    (cond
      (nil? dir) (throw (ex-info "could not find repo root (crates/rocksky/fixtures)" {}))
      (.isDirectory (File. dir "crates/rocksky/fixtures")) dir
      :else (recur (.getParentFile dir)))))

(defn -main [& _]
  (let [fixture (.getPath (File. (repo-root) "crates/rocksky/fixtures/08 - Internet Money - Speak(Explicit).m4a"))]
    (println "ABI version:" (ffi/abi-version))

    ;; -- metadata -------------------------------------------------------
    (try
      (let [meta (metadata/read fixture)
            codec (:codec meta "")]
        (println (str "codec=" codec " title=" (:title meta) " artist=" (:artist meta)
                      " duration_ms=" (:duration_ms meta) " sample_rate=" (:sample_rate meta)))
        (when (empty? codec) (fail "codec label should not be empty"))
        (let [probe (metadata/probe "song.flac")]
          (println "probe('song.flac') =" probe)
          (when-not (= probe "FLAC") (fail (str "expected FLAC, got " probe)))))
      (catch Exception e (fail (str "metadata: " e))))

    ;; -- DSP: -6.0206 dB track gain should HALVE amplitude --------------
    (try
      (let [rate 44100]
        (dsp/with-dsp [d rate]
          (dsp/set-replaygain d :track false 0.0)
          (dsp/set-replaygain-gains d :track-gain-db -6.0206) ; x0.5
          (let [sine (dsp/sine-stereo 1000.0 1.0 rate 16000.0)
                out (dsp/process d sine)
                peak (reduce (fn [m s] (max m (Math/abs (int s)))) 0 out)]
            (println (str "DSP peak after -6.02 dB track gain: " peak " (expected ~8000)"))
            (when-not (<= 7600 peak 8400) (fail (str "peak " peak " out of range"))))))
      (catch Exception e (fail (str "dsp: " e))))

    ;; -- Player (construct only, no audible playback) -------------------
    (try
      (player/with-player [p {:volume 0.0}]
        (let [sr (player/sample-rate p)]
          (println "Player sample_rate=" sr)
          (when (zero? sr) (fail "sample_rate should be > 0"))
          (player/set-volume p 0.0)
          (player/set-queue p [fixture])
          (Thread/sleep 100) ; queue command applied asynchronously
          (let [st (player/status p)
                state (:state st "")
                queue-len (:queue_len st -1)]
            (println (str "Player status: state=" state " queue_len=" queue-len))
            (when-not (= state "stopped") (fail (str "expected stopped, got " state)))
            (when-not (= queue-len 1) (fail (str "expected queue_len 1, got " queue-len))))))
      (catch Exception e (fail (str "player: " e))))

    (println "\nALL CHECKS PASSED ✔")
    (System/exit 0)))
