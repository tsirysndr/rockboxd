(ns console.verify
  "Sanity checks for the linked binary and staticlibs — the `nm`/`ar`/
  timestamp probes from CLAUDE.md, wrapped so you don't have to remember
  the incantations.

      (verify/stale)      ;; binary vs .a timestamps (the stale-binary pitfall)
      (verify/symbols)    ;; airplay + squeezelite symbols present in rockboxd
      (verify/staticlib)  ;; airplay + slim object files inside librockbox_cli.a
      (verify/nm \"pcm_\")   ;; grep the binary's symbol table
      (verify/ar  \"chromecast\") ;; grep the cli staticlib's member list"
  (:require [console.shell :as sh]
            [clojure.string :as str]))

(def ^:private bin "zig/zig-out/bin/rockboxd")
(def ^:private cli-a "target/release/librockbox_cli.a")

(defn stale
  "Print the mtimes behind the stale-binary pitfall so you can eyeball
  whether the binary is older than the libs it links."
  []
  (sh/sh ["ls" "-la"
          (sh/in bin)
          (sh/in "build-lib/libfirmware.a")
          (sh/in cli-a)
          (sh/in "target/release/librockbox_server.a")]))

(defn nm
  "Grep the daemon binary's symbol table for `pattern`.

      (verify/nm \"pcm_airplay\")"
  [pattern]
  (sh/sh ["sh" "-c"
          (str "nm " (sh/in bin) " | grep " (pr-str pattern))]))

(defn ar
  "Grep the cli staticlib's member (object-file) list for `pattern`.

      (verify/ar \"slim\")"
  [pattern]
  (sh/sh ["sh" "-c"
          (str "ar t " (sh/in cli-a) " | grep " (pr-str pattern))]))

(defn symbols
  "Check the airplay + squeezelite PCM-sink symbols made it into rockboxd."
  []
  (doseq [p ["pcm_airplay" "pcm_squeezelite"]]
    (println (str "── nm | grep " p " ──"))
    (nm p)))

(defn staticlib
  "Check the airplay + slim rlibs got bundled into librockbox_cli.a."
  []
  (doseq [p ["airplay" "slim"]]
    (println (str "── ar t | grep " p " ──"))
    (ar p)))
