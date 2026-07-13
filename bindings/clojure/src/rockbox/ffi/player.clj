(ns rockbox.ffi.player
  "Queue-based player with native ReplayGain and Rockbox crossfade.

  A player owns a live audio output device and a background engine thread —
  construct it only where an output device exists. A handle is an opaque
  MemorySegment; every function takes it as the first argument. Free it with
  `free` (or use `with-player`).

  ReplayGain `mode` here uses the *player* values (rockbox.ffi.enums/replaygain-mode:
  :off :track :album) — distinct from the DSP encoding.

  Mutating functions return the player handle, so they thread cleanly with `->`:

      (-> p
          (player/set-queue tracks)
          (player/set-shuffle true)
          (player/set-repeat :all)
          (player/play))

  Getters/queries (`volume`, `status`, `queue`, `dsp-settings`, …) return their
  values as usual."
  (:refer-clojure :exclude [next remove repeat])
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
   :mix-mode :crossfade
   :resume-file nil          ; path to an .m3u8 to auto-persist queue+position; nil disables
   :resume-save-interval-ms 0}) ; 0 => 5 s default

(defn new-player
  "Create a player. `config` overrides `default-config` (see the C ABI's
  rb_player_new_with_config / _ex). Throws if no output device is available.

  If `:resume-file` is set, the queue and exact position are auto-persisted to
  that .m3u8 every `:resume-save-interval-ms` ms (0 => 5 s), enabling
  `resume`/`save-resume`/`clear-resume`."
  (^MemorySegment [] (new-player {}))
  (^MemorySegment [config]
   (let [c (merge default-config config)]
     (with-open [a (Arena/ofConfined)]
       (let [resume-seg (if-let [rf (:resume-file c)]
                          (.allocateFrom a ^String rf)
                          MemorySegment/NULL)
             p ^MemorySegment
             (ffi/call :player-new-with-config-ex
                       (int (:sample-rate c)) (float (:buffer-seconds c)) (float (:volume c))
                       (int (enums/code enums/replaygain-mode (:replaygain-mode c)))
                       (float (:replaygain-preamp-db c)) (boolean (:replaygain-prevent-clipping c))
                       (int (enums/code enums/crossfade-mode (:crossfade-mode c)))
                       (int (:fade-out-delay-ms c)) (int (:fade-out-duration-ms c))
                       (int (:fade-in-delay-ms c)) (int (:fade-in-duration-ms c))
                       (int (enums/code enums/mix-mode (:mix-mode c)))
                       resume-seg (int (:resume-save-interval-ms c)))]
         (when (zero? (.address p))
           (throw (ex-info "rb_player_new_with_config_ex returned NULL (no output device?)" {})))
         p)))))

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

(defn set-queue
  "Replace the queue. Each entry may be a local file path, an http(s):// URL
  to a finite remote file, or a live-radio / streaming URL. Returns `p` for
  threading."
  [^MemorySegment p paths]
  (with-open [a (Arena/ofConfined)]
    (ffi/call :player-set-queue-json p (.allocateFrom a ^String (json/write-str (vec paths)))))
  p)

(defn enqueue
  "Append one track to the queue. `path` may be a local file path, an
  http(s):// URL to a finite remote file, or a live-radio / streaming URL.
  Returns `p` for threading."
  [^MemorySegment p path]
  (with-open [a (Arena/ofConfined)]
    (ffi/call :player-enqueue p (.allocateFrom a ^String path)))
  p)

(defn insert
  "Splice `paths` (local paths or http(s):// URLs) into the queue at `position`
  (rockbox.ffi.enums/insert-position; default :insert-last). `index` is only
  used when `position` is :index. Returns `p` for threading."
  ([^MemorySegment p paths] (insert p paths :insert-last 0))
  ([^MemorySegment p paths position] (insert p paths position 0))
  ([^MemorySegment p paths position index]
   (with-open [a (Arena/ofConfined)]
     (ffi/call :player-insert-json p
               (.allocateFrom a ^String (json/write-str (vec paths)))
               (int (enums/code enums/insert-position position))
               (long index)))
   p))

(defn remove
  "Remove the track at `index` (0-based) from the queue. An out-of-range index
  is ignored. Removing a track *before* the current one keeps the current track
  playing; removing the currently-playing track hard-cuts to the track that
  slides into its place; removing the last remaining track stops playback.
  Returns `p` for threading."
  [^MemorySegment p index]
  (ffi/call :player-remove p (long index))
  p)

(defn clear-queue
  "Empty the queue and stop playback (also clears any saved resume state).
  Returns `p` for threading."
  [^MemorySegment p]
  (ffi/call :player-clear-queue p)
  p)

(defn queue
  "The current queue as a vector of path/URL strings."
  [^MemorySegment p]
  (let [s (ffi/take-string (ffi/call :player-queue-json p))]
    (when-not s (throw (ex-info "rb_player_queue_json returned NULL" {})))
    (vec (json/read-str s))))

;; ---- transport --------------------------------------------------------

(defn play "Start playback. Returns `p` for threading." [^MemorySegment p] (ffi/call :player-play p) p)
(defn pause "Pause playback. Returns `p` for threading." [^MemorySegment p] (ffi/call :player-pause p) p)
(defn toggle "Toggle play/pause. Returns `p` for threading." [^MemorySegment p] (ffi/call :player-toggle p) p)
(defn stop "Stop playback. Returns `p` for threading." [^MemorySegment p] (ffi/call :player-stop p) p)
(defn next "Skip to the next track. Returns `p` for threading." [^MemorySegment p] (ffi/call :player-next p) p)
(defn previous "Skip to the previous track. Returns `p` for threading." [^MemorySegment p] (ffi/call :player-previous p) p)
(defn skip-to "Jump to queue `index`. Returns `p` for threading." [^MemorySegment p index] (ffi/call :player-skip-to p (long index)) p)
(defn seek-ms "Seek to `ms` in the current track. Returns `p` for threading." [^MemorySegment p ms] (ffi/call :player-seek-ms p (long ms)) p)

;; ---- settings ---------------------------------------------------------

(defn set-volume "Set the output volume (0.0–1.0). Returns `p` for threading." [^MemorySegment p vol] (ffi/call :player-set-volume p (float vol)) p)
(defn set-balance "Set the stereo balance, -100 (full left)–+100 (full right); 0 = centre. Returns `p` for threading." [^MemorySegment p balance] (ffi/call :player-set-balance p (int balance)) p)
(defn volume ^double [^MemorySegment p] (double (ffi/call :player-volume p)))
(defn balance ^long [^MemorySegment p] (long (ffi/call :player-balance p)))
(defn sample-rate ^long [^MemorySegment p]
  (bit-and (long (ffi/call :player-sample-rate p)) 0xFFFFFFFF))

(defn set-crossfade
  "Configure crossfade. Returns `p` for threading."
  [^MemorySegment p mode & {:keys [fade-out-delay-ms fade-out-duration-ms
                                   fade-in-delay-ms fade-in-duration-ms mix-mode]
                            :or {fade-out-delay-ms 0 fade-out-duration-ms 2000
                                 fade-in-delay-ms 0 fade-in-duration-ms 2000 mix-mode :crossfade}}]
  (ffi/call :player-set-crossfade p (int (enums/code enums/crossfade-mode mode))
            (int fade-out-delay-ms) (int fade-out-duration-ms)
            (int fade-in-delay-ms) (int fade-in-duration-ms)
            (int (enums/code enums/mix-mode mix-mode)))
  p)

(defn set-replaygain
  "`mode`: rockbox.ffi.enums/replaygain-mode (:off :track :album). Returns `p`
  for threading."
  [^MemorySegment p mode preamp-db prevent-clipping?]
  (ffi/call :player-set-replaygain p (int (enums/code enums/replaygain-mode mode))
            (float preamp-db) (boolean prevent-clipping?))
  p)

(defn set-shuffle
  "Enable or disable shuffle playback. Returns `p` for threading."
  [^MemorySegment p enabled?]
  (ffi/call :player-set-shuffle p (boolean enabled?))
  p)

(defn shuffle-enabled?
  "Whether shuffle playback is currently enabled."
  [^MemorySegment p]
  (boolean (ffi/call :player-is-shuffle-enabled p)))

(defn set-repeat
  "Set the repeat mode (rockbox.ffi.enums/repeat-mode; :off :one :all). Returns
  `p` for threading."
  [^MemorySegment p mode]
  (ffi/call :player-set-repeat p (int (enums/code enums/repeat-mode mode)))
  p)

(defn repeat
  "The current repeat mode as its raw int (0 off, 1 one, 2 all)."
  ^long [^MemorySegment p]
  (long (ffi/call :player-repeat p)))

;; ---- DSP --------------------------------------------------------------

(defn set-eq-enabled
  "Enable or disable the parametric EQ. Returns `p` for threading."
  [^MemorySegment p enabled?]
  (ffi/call :player-set-eq-enabled p (boolean enabled?))
  p)

(defn eq-enabled?
  "Whether the parametric EQ is currently enabled."
  [^MemorySegment p]
  (boolean (ffi/call :player-is-eq-enabled p)))

(defn set-eq-band
  "Configure one EQ band. `q` and `gain-db` are plain (not tenths) values.
  Returns `p` for threading."
  [^MemorySegment p band cutoff-hz q gain-db]
  (ffi/call :player-set-eq-band p (long band) (int cutoff-hz) (float q) (float gain-db))
  p)

(defn set-eq-precut
  "Set the EQ pre-cut (attenuation) in dB. Returns `p` for threading."
  [^MemorySegment p db]
  (ffi/call :player-set-eq-precut p (float db))
  p)

(defn set-eq-preset
  "Apply a built-in EQ preset (rockbox.ffi.enums/eq-preset; e.g. :flat :rock).
  Returns `p` for threading."
  [^MemorySegment p preset]
  (ffi/call :player-set-eq-preset p (int (enums/code enums/eq-preset preset)))
  p)

(defn set-tone
  "Set the tone controls: bass/treble gain (dB) and their cutoffs (Hz). Returns
  `p` for threading."
  [^MemorySegment p bass-db treble-db bass-cutoff-hz treble-cutoff-hz]
  (ffi/call :player-set-tone p (int bass-db) (int treble-db)
            (int bass-cutoff-hz) (int treble-cutoff-hz))
  p)

(defn set-bass
  "Set the bass tone gain in dB. Returns `p` for threading."
  [^MemorySegment p bass-db]
  (ffi/call :player-set-bass p (int bass-db))
  p)

(defn set-treble
  "Set the treble tone gain in dB. Returns `p` for threading."
  [^MemorySegment p treble-db]
  (ffi/call :player-set-treble p (int treble-db))
  p)

(defn set-bass-cutoff
  "Set the bass tone-control cutoff frequency in Hz. Returns `p` for threading."
  [^MemorySegment p hz]
  (ffi/call :player-set-bass-cutoff p (int hz))
  p)

(defn set-treble-cutoff
  "Set the treble tone-control cutoff frequency in Hz. Returns `p` for threading."
  [^MemorySegment p hz]
  (ffi/call :player-set-treble-cutoff p (int hz))
  p)

(defn set-crossfeed
  "Configure the crossfeed effect (headphone spatialization).

  `mode` is rockbox.ffi.enums/crossfeed-mode (:off :meier :custom). The gains
  (`direct-gain`, `cross-gain`, `hf-gain`) and `hf-cutoff` (Hz) are only used in
  :custom mode. Returns `p` for threading."
  [^MemorySegment p mode direct-gain cross-gain hf-gain hf-cutoff]
  (ffi/call :player-set-crossfeed p (int (enums/code enums/crossfeed-mode mode))
            (int direct-gain) (int cross-gain) (int hf-gain) (int hf-cutoff))
  p)

(defn set-bass-enhancement
  "Configure bass enhancement: `strength` and `precut`. Returns `p` for
  threading."
  [^MemorySegment p strength precut]
  (ffi/call :player-set-bass-enhancement p (int strength) (int precut))
  p)

(defn set-fatigue-reduction
  "Set the listening-fatigue reduction `strength`. Returns `p` for threading."
  [^MemorySegment p strength]
  (ffi/call :player-set-fatigue-reduction p (int strength))
  p)

(defn set-surround
  "Configure the surround effect: delay (ms), balance, and low/high cutoffs (Hz).
  Returns `p` for threading."
  [^MemorySegment p delay-ms balance cutoff-low-hz cutoff-high-hz]
  (ffi/call :player-set-surround p (int delay-ms) (int balance)
            (int cutoff-low-hz) (int cutoff-high-hz))
  p)

(defn set-channel-mode
  "Set the channel mode (rockbox.ffi.enums/channel-mode; e.g. :stereo :mono).
  Returns `p` for threading."
  [^MemorySegment p mode]
  (ffi/call :player-set-channel-mode p (int (enums/code enums/channel-mode mode)))
  p)

(defn set-stereo-width
  "Set the stereo width as a percentage. Returns `p` for threading."
  [^MemorySegment p percent]
  (ffi/call :player-set-stereo-width p (int percent))
  p)

(defn set-compressor
  "Configure the dynamic-range compressor. Returns `p` for threading."
  [^MemorySegment p threshold-db makeup-gain ratio knee attack-ms release-ms]
  (ffi/call :player-set-compressor p (int threshold-db) (int makeup-gain)
            (int ratio) (int knee) (int attack-ms) (int release-ms))
  p)

(defn set-dither
  "Enable or disable dithering. Returns `p` for threading."
  [^MemorySegment p enabled?]
  (ffi/call :player-set-dither p (boolean enabled?))
  p)

(defn set-pitch
  "Set the playback pitch ratio. Returns `p` for threading."
  [^MemorySegment p ratio]
  (ffi/call :player-set-pitch p (int ratio))
  p)

(defn dsp-settings
  "A snapshot of the player's DSP settings as a map (keyword keys)."
  [^MemorySegment p]
  (let [s (ffi/take-string (ffi/call :player-dsp-settings-json p))]
    (when-not s (throw (ex-info "rb_player_dsp_settings_json returned NULL" {})))
    (json/read-str s :key-fn keyword)))

;; ---- status -----------------------------------------------------------

(defn status
  "A snapshot of the player's status as a map (keyword keys)."
  [^MemorySegment p]
  (let [s (ffi/take-string (ffi/call :player-status-json p))]
    (when-not s (throw (ex-info "rb_player_status_json returned NULL" {})))
    (json/read-str s :key-fn keyword)))

;; ---- resume -----------------------------------------------------------

(defn resume
  "Restore the queue + exact position from the player's resume file (does NOT
  auto-play). Returns a map {:tracks [...] :index n :elapsed_ms n}, or nil if
  there was nothing to resume."
  [^MemorySegment p]
  (when-let [s (ffi/take-string (ffi/call :player-resume p))]
    (json/read-str s :key-fn keyword)))

(defn save-resume
  "Persist the queue + current position to the player's resume file now.
  Returns `p` for threading."
  [^MemorySegment p]
  (ffi/call :player-save-resume p)
  p)

(defn clear-resume
  "Delete the player's resume file. Returns `p` for threading."
  [^MemorySegment p]
  (ffi/call :player-clear-resume p)
  p)

(defn load-resume
  "Peek at a resume/.m3u8 file at `path` without a player. Returns a map
  {:tracks [...] :index n :elapsed_ms n}, or nil if absent/invalid."
  [path]
  (with-open [a (Arena/ofConfined)]
    (when-let [s (ffi/take-string (ffi/call :load-resume-json (.allocateFrom a ^String path)))]
      (json/read-str s :key-fn keyword))))

;; ---- m3u / m3u8 playlists ---------------------------------------------

(defn import-m3u
  "Import the playlist file at `path` into the queue at `position`
  (rockbox.ffi.enums/insert-position; default :insert-last). `index` is only
  used when `position` is :index. Returns the imported paths as a vector, or
  nil on error."
  ([^MemorySegment p path] (import-m3u p path :insert-last 0))
  ([^MemorySegment p path position] (import-m3u p path position 0))
  ([^MemorySegment p path position index]
   (with-open [a (Arena/ofConfined)]
     (when-let [s (ffi/take-string
                   (ffi/call :player-import-m3u p (.allocateFrom a ^String path)
                             (int (enums/code enums/insert-position position))
                             (long index)))]
       (vec (json/read-str s))))))

(defn load-m3u
  "Replace the queue with the playlist file at `path`. Returns the loaded paths
  as a vector, or nil on error."
  [^MemorySegment p path]
  (with-open [a (Arena/ofConfined)]
    (when-let [s (ffi/take-string
                  (ffi/call :player-load-m3u p (.allocateFrom a ^String path)))]
      (vec (json/read-str s)))))

(defn export-m3u
  "Export the current queue to an .m3u8 at `path` (atomic). Returns true on
  success, false on error."
  [^MemorySegment p path]
  (with-open [a (Arena/ofConfined)]
    (zero? (long (ffi/call :player-export-m3u p (.allocateFrom a ^String path))))))

(defn m3u-read
  "Parse a playlist file at `path` into a vector of maps
  {:path s :duration_ms n :title s}, or nil on error. No player needed."
  [path]
  (with-open [a (Arena/ofConfined)]
    (when-let [s (ffi/take-string (ffi/call :m3u-read-json (.allocateFrom a ^String path)))]
      (vec (json/read-str s :key-fn keyword)))))

(defn m3u-write
  "Write `paths` (a seq of path/URL strings) as an .m3u8 at `path`. Returns true
  on success, false on error. No player needed."
  [path paths]
  (with-open [a (Arena/ofConfined)]
    (zero? (long (ffi/call :m3u-write-json (.allocateFrom a ^String path)
                           (.allocateFrom a ^String (json/write-str (vec paths))))))))

(defn is-url?
  "Whether `s` looks like an http(s):// URL."
  [s]
  (with-open [a (Arena/ofConfined)]
    (boolean (ffi/call :is-url (.allocateFrom a ^String s)))))
