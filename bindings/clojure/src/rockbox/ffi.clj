(ns rockbox.ffi
  "Runtime loader for the prebuilt librockbox_ffi shared library, built on the
  Java Foreign Function & Memory API (JEP 454, stable since JDK 22).

  Nothing is linked at build time: the library is located at runtime and every
  function is bound to a MethodHandle downcall. This mirrors
  include/rockbox_ffi.h — keep the descriptors in sync with the C ABI."
  (:require [clojure.string :as str]
            [clojure.java.io :as io])
  (:import [java.lang.foreign Linker Linker$Option FunctionDescriptor ValueLayout
            MemoryLayout Arena MemorySegment SymbolLookup]
           [java.lang.invoke MethodHandle]
           [java.io File]))

;; Layout shorthands (LP64: size_t / uint64_t / int64_t are all 8 bytes).
(def PTR ValueLayout/ADDRESS)   ; opaque pointer
(def I32 ValueLayout/JAVA_INT)  ; int32_t / uint32_t
(def I64 ValueLayout/JAVA_LONG) ; int64_t / uint64_t / size_t
(def F32 ValueLayout/JAVA_FLOAT)
(def I16 ValueLayout/JAVA_SHORT)
(def BOOL ValueLayout/JAVA_BOOLEAN)

;; ---- library location -------------------------------------------------

(defn- native-target
  "The `<os>-<arch>` release target for the running JVM, or nil if unsupported."
  []
  (let [os (str/lower-case (System/getProperty "os.name"))
        arch (str/lower-case (System/getProperty "os.arch"))
        o (cond
            (or (str/includes? os "mac") (str/includes? os "darwin")) "darwin"
            (str/includes? os "linux") "linux"
            (str/includes? os "freebsd") "freebsd"
            (str/includes? os "netbsd") "netbsd")
        a (cond
            (#{"aarch64" "arm64"} arch) "arm64"
            (#{"amd64" "x86_64" "x64"} arch) "x64")]
    (when (and o a) (str o "-" a))))

(defn- extract-bundled
  "Extract the platform's bundled native/<target>/librockbox_ffi.<ext> classpath
  resource to a temp file and return its path, or nil if none is bundled.
  `record` logs the attempted resource for error reporting."
  [record]
  (when-let [target (native-target)]
    (let [ext (if (str/starts-with? target "darwin") "dylib" "so")
          resource (str "native/" target "/librockbox_ffi." ext)]
      (record (str "classpath:" resource))
      (when-let [stream (.getResourceAsStream (clojure.lang.RT/baseLoader) resource)]
        (with-open [in stream]
          (let [tmp (File/createTempFile "librockbox_ffi" (str "." ext))]
            (.deleteOnExit tmp)
            (io/copy in tmp)
            (.getAbsolutePath tmp)))))))

(defn- locate-lib
  "Locate the shared library. Precedence:
     1. ROCKBOX_FFI_LIB env var (explicit override)
     2. the native lib bundled in the jar for this OS/arch (published pkg)
     3. target/release, walking up from the working directory (repo checkout)"
  ^String []
  (let [names ["librockbox_ffi.dylib" "librockbox_ffi.so" "rockbox_ffi.dll"]
        tried (java.util.ArrayList.)
        attempt (fn [^String path]
                  (.add tried path)
                  (when (.exists (File. path)) path))]
    (or
     (when-let [env (System/getenv "ROCKBOX_FFI_LIB")] (attempt env))
     (extract-bundled (fn [s] (.add tried s)))
     (loop [dir (.getAbsoluteFile (File. (System/getProperty "user.dir")))]
       (when dir
         (or (some (fn [sub] (some #(attempt (.getPath (File. dir (str sub "/" %)))) names))
                   ["target/release" "Libs"])
             (recur (.getParentFile dir)))))
     (throw (ex-info (str "could not locate librockbox_ffi shared library. Set "
                          "ROCKBOX_FFI_LIB or run `cargo build --release -p "
                          "rockbox-ffi`. Tried:\n  " (str/join "\n  " tried))
                     {:tried (vec tried)})))))

;; Lazy: even Linker/nativeLinker is a Foreign-API call, kept out of ns load so
;; requiring this namespace does no FFM work (cljdoc analysis, older JDKs).
(def ^:private linker (delay (Linker/nativeLinker)))

;; The library outlives every call, so it lives in the global arena (never
;; unloaded). Loaded lazily on first FFI use — merely `require`-ing this
;; namespace (e.g. for cljdoc doc analysis) must NOT touch native code.
(def ^:private lookup (delay (SymbolLookup/libraryLookup (locate-lib) (Arena/global))))

(defn- fd-of ^FunctionDescriptor [ret args]
  (FunctionDescriptor/of ret (into-array MemoryLayout args)))

(defn- fd-void ^FunctionDescriptor [args]
  (FunctionDescriptor/ofVoid (into-array MemoryLayout args)))

(defn- dc ^MethodHandle [^String name ^FunctionDescriptor fd]
  (let [sym (-> @lookup (.find name)
                (.orElseThrow (reify java.util.function.Supplier
                                (get [_] (ex-info (str "symbol " name " not found in librockbox_ffi") {})))))]
    (.downcallHandle @linker sym fd (make-array Linker$Option 0))))

;; Bound lazily (a delay): the downcall handles are built on first FFI call,
;; not at namespace load, so requiring this ns never dlopens the library.
(def ^:private handles
  (delay
   {;; ---- deallocators / version ----
   :abi-version                (dc "rb_ffi_abi_version" (fd-of I32 []))
   :string-free                (dc "rb_string_free" (fd-void [PTR]))
   :buffer-free                (dc "rb_buffer_free" (fd-void [PTR I64]))
   ;; ---- DSP ----
   :dsp-new                    (dc "rb_dsp_new" (fd-of PTR [I32]))
   :dsp-free                   (dc "rb_dsp_free" (fd-void [PTR]))
   :dsp-set-input-frequency    (dc "rb_dsp_set_input_frequency" (fd-void [PTR I32]))
   :dsp-flush                  (dc "rb_dsp_flush" (fd-void [PTR]))
   :dsp-eq-enable              (dc "rb_dsp_eq_enable" (fd-void [PTR BOOL]))
   :dsp-set-tone               (dc "rb_dsp_set_tone" (fd-void [PTR I32 I32]))
   :dsp-set-tone-cutoffs       (dc "rb_dsp_set_tone_cutoffs" (fd-void [PTR I32 I32]))
   :dsp-set-surround           (dc "rb_dsp_set_surround" (fd-void [PTR I32 I32 I32 I32]))
   :dsp-set-channel-config     (dc "rb_dsp_set_channel_config" (fd-void [PTR I32]))
   :dsp-set-stereo-width       (dc "rb_dsp_set_stereo_width" (fd-void [PTR I32]))
   :dsp-set-compressor         (dc "rb_dsp_set_compressor" (fd-void [PTR I32 I32 I32 I32 I32 I32]))
   :dsp-set-replaygain         (dc "rb_dsp_set_replaygain" (fd-void [PTR I32 BOOL F32]))
   :dsp-set-replaygain-gains   (dc "rb_dsp_set_replaygain_gains" (fd-void [PTR F32 F32 F32 F32]))
   :dsp-set-replaygain-gains-raw (dc "rb_dsp_set_replaygain_gains_raw" (fd-void [PTR I64 I64 I64 I64]))
   :dsp-set-eq-band            (dc "rb_dsp_set_eq_band" (fd-void [PTR I64 I32 F32 F32]))
   :dsp-set-eq-precut          (dc "rb_dsp_set_eq_precut" (fd-void [PTR F32]))
   :dsp-process                (dc "rb_dsp_process" (fd-of PTR [PTR PTR I64 PTR]))
   ;; ---- metadata ----
   :meta-read-json             (dc "rb_meta_read_json" (fd-of PTR [PTR]))
   :meta-probe                 (dc "rb_meta_probe" (fd-of PTR [PTR]))
   ;; ---- decoder ----
   :decoder-open               (dc "rb_decoder_open" (fd-of PTR [PTR]))
   :decoder-free               (dc "rb_decoder_free" (fd-void [PTR]))
   :decoder-metadata-json      (dc "rb_decoder_metadata_json" (fd-of PTR [PTR]))
   :decoder-next-chunk         (dc "rb_decoder_next_chunk" (fd-of PTR [PTR PTR PTR]))
   :decoder-seek-ms            (dc "rb_decoder_seek_ms" (fd-void [PTR I64]))
   :decoder-elapsed-ms         (dc "rb_decoder_elapsed_ms" (fd-of I64 [PTR]))
   :decoder-finished           (dc "rb_decoder_finished" (fd-of BOOL [PTR PTR]))
   ;; ---- player ----
   :player-new                 (dc "rb_player_new" (fd-of PTR []))
   :player-new-with-config     (dc "rb_player_new_with_config"
                                   (fd-of PTR [I32 F32 F32 I32 F32 BOOL I32 I32 I32 I32 I32 I32]))
   :player-new-with-config-ex  (dc "rb_player_new_with_config_ex"
                                   (fd-of PTR [I32 F32 F32 I32 F32 BOOL I32 I32 I32 I32 I32 I32 PTR I32]))
   :player-free                (dc "rb_player_free" (fd-void [PTR]))
   :player-set-queue-json      (dc "rb_player_set_queue_json" (fd-void [PTR PTR]))
   :player-enqueue             (dc "rb_player_enqueue" (fd-void [PTR PTR]))
   :player-insert-json         (dc "rb_player_insert_json" (fd-void [PTR PTR I32 I64]))
   :player-remove              (dc "rb_player_remove" (fd-void [PTR I64]))
   :player-clear-queue         (dc "rb_player_clear_queue" (fd-void [PTR]))
   :player-queue-json          (dc "rb_player_queue_json" (fd-of PTR [PTR]))
   :player-play                (dc "rb_player_play" (fd-void [PTR]))
   :player-pause               (dc "rb_player_pause" (fd-void [PTR]))
   :player-toggle              (dc "rb_player_toggle" (fd-void [PTR]))
   :player-stop                (dc "rb_player_stop" (fd-void [PTR]))
   :player-next                (dc "rb_player_next" (fd-void [PTR]))
   :player-previous            (dc "rb_player_previous" (fd-void [PTR]))
   :player-skip-to             (dc "rb_player_skip_to" (fd-void [PTR I64]))
   :player-seek-ms             (dc "rb_player_seek_ms" (fd-void [PTR I64]))
   :player-set-volume          (dc "rb_player_set_volume" (fd-void [PTR F32]))
   :player-set-balance         (dc "rb_player_set_balance" (fd-void [PTR I32]))
   :player-set-crossfade       (dc "rb_player_set_crossfade" (fd-void [PTR I32 I32 I32 I32 I32 I32]))
   :player-set-replaygain      (dc "rb_player_set_replaygain" (fd-void [PTR I32 F32 BOOL]))
   :player-set-shuffle         (dc "rb_player_set_shuffle" (fd-void [PTR BOOL]))
   :player-is-shuffle-enabled  (dc "rb_player_is_shuffle_enabled" (fd-of BOOL [PTR]))
   :player-set-repeat          (dc "rb_player_set_repeat" (fd-void [PTR I32]))
   :player-repeat              (dc "rb_player_repeat" (fd-of I32 [PTR]))
   ;; ---- player DSP ----
   :player-set-eq-enabled      (dc "rb_player_set_eq_enabled" (fd-void [PTR BOOL]))
   :player-is-eq-enabled       (dc "rb_player_is_eq_enabled" (fd-of BOOL [PTR]))
   :player-set-eq-band         (dc "rb_player_set_eq_band" (fd-void [PTR I64 I32 F32 F32]))
   :player-set-eq-precut       (dc "rb_player_set_eq_precut" (fd-void [PTR F32]))
   :player-set-eq-preset       (dc "rb_player_set_eq_preset" (fd-void [PTR I32]))
   :player-set-tone            (dc "rb_player_set_tone" (fd-void [PTR I32 I32 I32 I32]))
   :player-set-bass            (dc "rb_player_set_bass" (fd-void [PTR I32]))
   :player-set-treble          (dc "rb_player_set_treble" (fd-void [PTR I32]))
   :player-set-bass-cutoff     (dc "rb_player_set_bass_cutoff" (fd-void [PTR I32]))
   :player-set-treble-cutoff   (dc "rb_player_set_treble_cutoff" (fd-void [PTR I32]))
   :player-set-crossfeed       (dc "rb_player_set_crossfeed" (fd-void [PTR I32 I32 I32 I32 I32]))
   :player-set-bass-enhancement (dc "rb_player_set_bass_enhancement" (fd-void [PTR I32 I32]))
   :player-set-fatigue-reduction (dc "rb_player_set_fatigue_reduction" (fd-void [PTR I32]))
   :player-set-surround        (dc "rb_player_set_surround" (fd-void [PTR I32 I32 I32 I32]))
   :player-set-channel-mode    (dc "rb_player_set_channel_mode" (fd-void [PTR I32]))
   :player-set-stereo-width    (dc "rb_player_set_stereo_width" (fd-void [PTR I32]))
   :player-set-compressor      (dc "rb_player_set_compressor" (fd-void [PTR I32 I32 I32 I32 I32 I32]))
   :player-set-dither          (dc "rb_player_set_dither" (fd-void [PTR BOOL]))
   :player-set-pitch           (dc "rb_player_set_pitch" (fd-void [PTR I32]))
   :player-dsp-settings-json   (dc "rb_player_dsp_settings_json" (fd-of PTR [PTR]))
   :player-volume              (dc "rb_player_volume" (fd-of F32 [PTR]))
   :player-balance             (dc "rb_player_balance" (fd-of I32 [PTR]))
   :player-sample-rate         (dc "rb_player_sample_rate" (fd-of I32 [PTR]))
   :player-status-json         (dc "rb_player_status_json" (fd-of PTR [PTR]))
   ;; ---- resume ----
   :player-resume              (dc "rb_player_resume" (fd-of PTR [PTR]))
   :player-save-resume         (dc "rb_player_save_resume" (fd-void [PTR]))
   :player-clear-resume        (dc "rb_player_clear_resume" (fd-void [PTR]))
   :load-resume-json           (dc "rb_load_resume_json" (fd-of PTR [PTR]))
   ;; ---- m3u / m3u8 ----
   :player-import-m3u          (dc "rb_player_import_m3u" (fd-of PTR [PTR PTR I32 I64]))
   :player-load-m3u            (dc "rb_player_load_m3u" (fd-of PTR [PTR PTR]))
   :player-export-m3u          (dc "rb_player_export_m3u" (fd-of I32 [PTR PTR]))
   :m3u-read-json              (dc "rb_m3u_read_json" (fd-of PTR [PTR]))
   :m3u-write-json             (dc "rb_m3u_write_json" (fd-of I32 [PTR PTR]))
   :is-url                     (dc "rb_is_url" (fd-of BOOL [PTR]))}))

(defn call
  "Invoke the C function bound to `k`. Args must already be the right JVM type
  (use int/long/float/boolean/MemorySegment at the call site — the ABI does no
  narrowing). Returns the boxed result, or nil for void functions."
  [k & args]
  (.invokeWithArguments ^MethodHandle (get @handles k) (to-array args)))

(defn take-string
  "Copy a heap C string returned by the ABI into a String, then free it.
  Returns nil for a NULL pointer (the ABI's error/absent signal)."
  [^MemorySegment seg]
  (when (and seg (not (zero? (.address seg))))
    (let [s (.getString (.reinterpret seg Long/MAX_VALUE) 0)]
      (call :string-free seg)
      s)))

(defn abi-version
  "ABI major version of the loaded library (bumped on breaking changes)."
  []
  (bit-and (long (call :abi-version)) 0xFFFFFFFF))
