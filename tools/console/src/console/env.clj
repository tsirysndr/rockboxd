(ns console.env
  "Load env vars from `.env` files into a Clojure atom, and have every
  console command inherit them via subprocess env injection
  (see `console.shell`).

  Handy for the knobs rockboxd reads at runtime — e.g. `RUST_LOG`,
  `ROCKBOX_LIBRARY`, `ROCKBOX_UPDATE_LIBRARY`, `ANDROID_NDK_HOME`.

  Quick tour:

      (env/load!)                  ; loads <repo>/.env
      (env/set! \"RUST_LOG\" \"debug\") ; in-memory, wins for later subprocesses
      (env/show)                   ; prints loaded keys, values masked
      (env/get \"RUST_LOG\")          ; raw value
      (env/save!)                  ; write to <repo>/.env.local

  Sources are layered: each `load!` merges on top of whatever is already
  there, so later sources win. `reload!` replays every source in order."
  (:refer-clojure :exclude [get])
  (:require [clojure.string :as str]
            [console.path :as path]
            [babashka.fs :as fs]))

(def ^:dynamic *env*
  "Atom holding the currently loaded env map ({string -> string}).
  Read by `console.shell/sh` and friends when spawning subprocesses."
  (atom {}))

(defonce ^:private sources
  ;; Vector of {:type :file :path "..."}
  (atom []))

;; ── parsing (dotenv format) ──────────────────────────────────────────

(defn- strip-quotes [s]
  (let [s (str/trim s)]
    (cond
      (and (>= (count s) 2)
           (str/starts-with? s "\"") (str/ends-with? s "\"")) (subs s 1 (dec (count s)))
      (and (>= (count s) 2)
           (str/starts-with? s "'")  (str/ends-with? s "'"))  (subs s 1 (dec (count s)))
      :else s)))

(defn parse-dotenv
  "Parse a dotenv-format string into {string -> string}. Handles
  `KEY=value`, quoted values, `export ` prefix, `# comments`, blanks."
  [text]
  (->> (str/split-lines text)
       (keep (fn [raw]
               (let [line (str/trim raw)]
                 (when-not (or (empty? line) (str/starts-with? line "#"))
                   (let [line (cond-> line
                                (str/starts-with? line "export ") (subs 7))
                         idx  (.indexOf line "=")]
                     (when (pos? idx)
                       [(str/trim (subs line 0 idx))
                        (strip-quotes (subs line (inc idx)))]))))))
       (into {})))

;; ── source management ───────────────────────────────────────────────

(defn- record-source! [src]
  (swap! sources (fn [xs] (-> xs (->> (remove #(= % src))) vec (conj src)))))

(defn- resolve-path [p]
  (let [p (str p)]
    (if (fs/absolute? p) p (str (fs/path (path/repo-root) p)))))

(defn load!
  "Merge a `.env` file into `*env*`. Path is relative to repo root if not
  absolute. With no args, loads `<repo>/.env`. Returns key-count or
  `nil` if the file does not exist."
  ([] (load! ".env"))
  ([path]
   (let [resolved (resolve-path path)]
     (when (fs/exists? resolved)
       (let [m (parse-dotenv (slurp resolved))]
         (swap! *env* merge m)
         (record-source! {:type :file :path resolved})
         (count m))))))

(defn set!
  "Set one key in `*env*`. Immediately visible to every subsequent
  subprocess; does not write to disk.

      (env/set! \"RUST_LOG\" \"debug\")"
  [k v]
  (swap! *env* assoc (str k) (str v))
  k)

(defn unset!
  "Remove one key from `*env*`. In-memory only."
  [k]
  (swap! *env* dissoc (str k))
  k)

(defn merge!
  "Merge a map of {key value} into `*env*` (later values win)."
  [m]
  (let [m (into {} (for [[k v] m] [(str k) (str v)]))]
    (swap! *env* clojure.core/merge m)
    (count m)))

(defn unload!
  "Clear `*env*` and forget every loaded source."
  []
  (reset! *env* {})
  (reset! sources [])
  :ok)

(defn reload!
  "Re-load every previously loaded file, in order."
  []
  (let [srcs @sources]
    (reset! *env* {})
    (reset! sources [])
    (doseq [s srcs] (load! (:path s)))
    (count @*env*)))

;; ── persistence ─────────────────────────────────────────────────────

(defn- needs-quoting? [v]
  (re-find #"[\s\"'#$=]" (str v)))

(defn- emit-dotenv [m]
  (->> (sort-by key m)
       (map (fn [[k v]]
              (let [v (str v)]
                (str k "="
                     (if (needs-quoting? v)
                       (str "\""
                            (-> v
                                (str/replace "\\" "\\\\")
                                (str/replace "\"" "\\\""))
                            "\"")
                       v)))))
       (str/join "\n")))

(defn save!
  "Write the current `*env*` to disk as a `.env`-format file.

  Path is relative to repo root if not absolute. With no args, writes
  `.env.local` — so the original `.env` is never silently clobbered.

  NOTE: writes the *resolved* in-memory map. Comments, ordering, and
  `${VAR}` interpolation from the original source files are lost."
  ([] (save! ".env.local"))
  ([path]
   (let [resolved (resolve-path path)]
     (spit resolved (str (emit-dotenv @*env*) "\n"))
     resolved)))

;; ── inspection ──────────────────────────────────────────────────────

(defn- mask [v]
  (let [s (str v)]
    (cond
      (empty? s)        ""
      (<= (count s) 4)  "***"
      :else             (str (subs s 0 2) "***" (subs s (- (count s) 2))))))

(defn show
  "Print every loaded key, values masked. Pass `:unmask` for raw values
  (never paste that anywhere shared)."
  ([] (show {}))
  ([opts]
   (let [unmask? (or (= opts :unmask) (true? (:unmask? opts)))
         render  (if unmask? identity mask)
         m       @*env*]
     (if (empty? m)
       (println "(env empty — try (env/load!) or (env/set! ...))")
       (do
         (println (str (count m) " key(s) loaded from "
                       (mapv :path @sources)
                       (when unmask? "  [UNMASKED]") ":"))
         (doseq [[k v] (sort-by key m)]
           (println " " k "=" (render v)))))
     :ok)))

(defn get
  "Fetch one value (raw, not masked)."
  ([k] (get k nil))
  ([k default] (clojure.core/get @*env* (str k) default)))

(defn as-map
  "Return a copy of the current env map. Used by `console.shell` to seed
  every subprocess's `:extra-env`."
  []
  (into {} @*env*))
