(ns rockbox.ffi.player
  "Queue-based player with native ReplayGain and Rockbox crossfade.

  A player owns a live audio output device and a background engine thread —
  construct it only where an output device exists. A handle is an opaque
  MemorySegment; every function takes it as the first argument. Free it with
  `free` (or use `with-player`).

  ReplayGain `mode` here uses the *player* values (rockbox.ffi.enums/replaygain-mode:
  :off :track :album) — distinct from the DSP encoding."
  (:refer-clojure :exclude [next])
  (:require [rockbox.ffi :as ffi]
            [rockbox.ffi.enums :as enums]
            [clojure.data.json :as json])
  (:import [java.lang.foreign Arena MemorySegment]))

(def default-config
  {:sample-rate 0            ; 0 => device default
   :buffer-seconds 4.0
   :volume 1.0
   :replaygain-mode :off
   :replaygain-preamp-db 0.0
   :replaygain-prevent-clipping true
   :crossfade-mode :off
   :fade-out-delay-ms 0
   :fade-out-duration-ms 2000
   :fade-in-delay-ms 0
   :fade-in-duration-ms 2000
   :mix-mode :crossfade})

(defn new-player
  "Create a player. `config` overrides `default-config` (see the C ABI's
  rb_player_new_with_config). Throws if no output device is available."
  (^MemorySegment [] (new-player {}))
  (^MemorySegment [config]
   (let [c (merge default-config config)
         p ^MemorySegment
         (ffi/call :player-new-with-config
                   (int (:sample-rate c)) (float (:buffer-seconds c)) (float (:volume c))
                   (int (enums/code enums/replaygain-mode (:replaygain-mode c)))
                   (float (:replaygain-preamp-db c)) (boolean (:replaygain-prevent-clipping c))
                   (int (enums/code enums/crossfade-mode (:crossfade-mode c)))
                   (int (:fade-out-delay-ms c)) (int (:fade-out-duration-ms c))
                   (int (:fade-in-delay-ms c)) (int (:fade-in-duration-ms c))
                   (int (enums/code enums/mix-mode (:mix-mode c))))]
     (when (zero? (.address p))
       (throw (ex-info "rb_player_new_with_config returned NULL (no output device?)" {})))
     p)))

(defn free
  "Free the native handle. Safe to call with nil."
  [^MemorySegment p]
  (when p (ffi/call :player-free p)))

(defmacro with-player
  "Bind `sym` to a fresh player (optional `config`), run `body`, then free it."
  [[sym & [config]] & body]
  `(let [~sym (new-player ~(or config {}))]
     (try ~@body (finally (free ~sym)))))

;; ---- queue ------------------------------------------------------------

(defn set-queue [^MemorySegment p paths]
  (with-open [a (Arena/ofConfined)]
    (ffi/call :player-set-queue-json p (.allocateFrom a ^String (json/write-str (vec paths))))))

(defn enqueue [^MemorySegment p path]
  (with-open [a (Arena/ofConfined)]
    (ffi/call :player-enqueue p (.allocateFrom a ^String path))))

;; ---- transport --------------------------------------------------------

(defn play [^MemorySegment p] (ffi/call :player-play p))
(defn pause [^MemorySegment p] (ffi/call :player-pause p))
(defn toggle [^MemorySegment p] (ffi/call :player-toggle p))
(defn stop [^MemorySegment p] (ffi/call :player-stop p))
(defn next [^MemorySegment p] (ffi/call :player-next p))
(defn previous [^MemorySegment p] (ffi/call :player-previous p))
(defn skip-to [^MemorySegment p index] (ffi/call :player-skip-to p (long index)))
(defn seek-ms [^MemorySegment p ms] (ffi/call :player-seek-ms p (long ms)))

;; ---- settings ---------------------------------------------------------

(defn set-volume [^MemorySegment p vol] (ffi/call :player-set-volume p (float vol)))
(defn volume ^double [^MemorySegment p] (double (ffi/call :player-volume p)))
(defn sample-rate ^long [^MemorySegment p]
  (bit-and (long (ffi/call :player-sample-rate p)) 0xFFFFFFFF))

(defn set-crossfade
  [^MemorySegment p mode & {:keys [fade-out-delay-ms fade-out-duration-ms
                                   fade-in-delay-ms fade-in-duration-ms mix-mode]
                            :or {fade-out-delay-ms 0 fade-out-duration-ms 2000
                                 fade-in-delay-ms 0 fade-in-duration-ms 2000 mix-mode :crossfade}}]
  (ffi/call :player-set-crossfade p (int (enums/code enums/crossfade-mode mode))
            (int fade-out-delay-ms) (int fade-out-duration-ms)
            (int fade-in-delay-ms) (int fade-in-duration-ms)
            (int (enums/code enums/mix-mode mix-mode))))

(defn set-replaygain
  "`mode`: rockbox.ffi.enums/replaygain-mode (:off :track :album)."
  [^MemorySegment p mode preamp-db prevent-clipping?]
  (ffi/call :player-set-replaygain p (int (enums/code enums/replaygain-mode mode))
            (float preamp-db) (boolean prevent-clipping?)))

;; ---- status -----------------------------------------------------------

(defn status
  "A snapshot of the player's status as a map (keyword keys)."
  [^MemorySegment p]
  (let [s (ffi/take-string (ffi/call :player-status-json p))]
    (when-not s (throw (ex-info "rb_player_status_json returned NULL" {})))
    (json/read-str s :key-fn keyword)))
