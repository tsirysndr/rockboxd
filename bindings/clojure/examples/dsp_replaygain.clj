;; Apply a -6.02 dB ReplayGain track gain and confirm it halves a 1 kHz sine.
;; A -6.0206 dB gain is a x0.5 linear scale, so the 16000-amplitude input
;; should come out at ~8000.
;;
;; Run: clojure -M:examples examples/dsp_replaygain.clj
(require '[rockbox.ffi.dsp :as dsp])

(dsp/with-dsp [d 44100]
  (dsp/set-replaygain d :track false 0.0)
  (dsp/set-replaygain-gains d :track-gain-db -6.0206)
  (let [sine (dsp/sine-stereo 1000.0 1.0 44100 16000.0)
        out  (dsp/process d sine)
        peak (reduce (fn [m s] (max m (Math/abs (int s)))) 0 out)]
    (println "input peak:  16000")
    (println "output peak:" peak "(expected ~8000)")))
