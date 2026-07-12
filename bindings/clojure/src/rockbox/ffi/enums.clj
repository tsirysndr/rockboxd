(ns rockbox.ffi.enums
  "Symbolic names for the small-integer enums in the C ABI. Pass either the
  keyword (resolved here) or the raw int to the dsp/player functions.")

;; Note the two *different* ReplayGain encodings in the C ABI.

;; Values for rockbox.ffi.dsp/set-replaygain (native Rockbox encoding).
(def dsp-replaygain-mode {:track 0 :album 1 :shuffle 2 :off 3})

;; Values for rockbox.ffi.player/set-replaygain and the :replaygain-mode config key.
(def replaygain-mode {:off 0 :track 1 :album 2})

(def crossfade-mode
  {:off 0 :auto-skip 1 :manual-skip 2 :shuffle 3 :shuffle-or-manual 4 :always 5})

(def mix-mode {:crossfade 0 :mix 1})

;; Values for rockbox.ffi.player/insert (and import-m3u): where to splice new
;; entries into the queue. :index uses the explicit `index` argument.
(def insert-position
  {:prepend 0 :insert 1 :insert-next 2 :insert-last 3
   :insert-shuffled 4 :insert-last-shuffled 5 :replace 6 :index 7})

(def channel-config
  {:stereo 0 :mono 1 :custom 2 :mono-left 3 :mono-right 4 :karaoke 5 :swap 6})

;; Values for rockbox.ffi.player/set-channel-mode.
(def channel-mode
  {:stereo 0 :mono 1 :custom 2 :mono-left 3 :mono-right 4 :karaoke 5 :swap 6})

;; Values for rockbox.ffi.player/set-repeat.
(def repeat-mode {:off 0 :one 1 :all 2})

;; Values for rockbox.ffi.player/set-crossfeed.
(def crossfeed-mode {:off 0 :meier 1 :custom 2})

;; Values for rockbox.ffi.player/set-eq-preset (built-in EQ curve presets).
(def eq-preset
  {:flat 0 :acoustic 1 :bass-boost 2 :bass-reducer 3 :classical 4 :dance 5
   :deep 6 :electronic 7 :hip-hop 8 :jazz 9 :latin 10 :loudness 11 :lounge 12
   :piano 13 :pop 14 :rnb 15 :rock 16 :small-speakers 17 :treble-boost 18
   :treble-reducer 19 :vocal-boost 20})

(defn code
  "Resolve `v` in enum map `m`: a keyword maps through, an int passes through."
  [m v]
  (cond
    (keyword? v) (or (get m v) (throw (ex-info (str "unknown enum value " v) {:map m})))
    (integer? v) v
    :else (throw (ex-info (str "expected keyword or int, got " (pr-str v)) {}))))
