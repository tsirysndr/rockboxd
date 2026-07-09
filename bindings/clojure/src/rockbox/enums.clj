(ns rockbox.enums
  "Symbolic names for the small-integer enums in the C ABI. Pass either the
  keyword (resolved here) or the raw int to the dsp/player functions.")

;; Note the two *different* ReplayGain encodings in the C ABI.

;; Values for rockbox.dsp/set-replaygain (native Rockbox encoding).
(def dsp-replaygain-mode {:track 0 :album 1 :shuffle 2 :off 3})

;; Values for rockbox.player/set-replaygain and the :replaygain-mode config key.
(def replaygain-mode {:off 0 :track 1 :album 2})

(def crossfade-mode
  {:off 0 :auto-skip 1 :manual-skip 2 :shuffle 3 :shuffle-or-manual 4 :always 5})

(def mix-mode {:crossfade 0 :mix 1})

(def channel-config
  {:stereo 0 :mono 1 :custom 2 :mono-left 3 :mono-right 4 :karaoke 5 :swap 6})

(defn code
  "Resolve `v` in enum map `m`: a keyword maps through, an int passes through."
  [m v]
  (cond
    (keyword? v) (or (get m v) (throw (ex-info (str "unknown enum value " v) {:map m})))
    (integer? v) v
    :else (throw (ex-info (str "expected keyword or int, got " (pr-str v)) {}))))
