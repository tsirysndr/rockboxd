;; Enable the 10-band parametric EQ and set a couple of shelf bands, then run
;; audio through it. Band 0 is a low shelf, band 9 a high shelf; cutoff is in
;; Hz, Q and gain in plain units (the binding scales gain/Q to the ABI's tenths).
;;
;; Run: clojure -M:examples examples/dsp_eq.clj
(require '[rockbox.ffi.dsp :as dsp])

(dsp/with-dsp [d 44100]
  (dsp/eq-enable d true)
  (dsp/set-eq-band d 0 100   0.7  6.0)    ; +6.0 dB low shelf  @ 100 Hz
  (dsp/set-eq-band d 4 1000  1.0  3.0)    ; +3.0 dB peaking     @ 1 kHz
  (dsp/set-eq-band d 9 10000 0.7 -3.0)    ; -3.0 dB high shelf @ 10 kHz
  (dsp/set-eq-precut d 3.0)               ; 3 dB pre-cut to avoid clipping
  (let [out (dsp/process d (dsp/sine-stereo 100.0 0.5 44100 8000.0))]
    (println "processed" (count out) "samples through the EQ")))
