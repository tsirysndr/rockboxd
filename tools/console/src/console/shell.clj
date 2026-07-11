(ns console.shell
  "Shell-out helpers shared by every command wrapper.

  Two flavors:
    `sh`   — inherit stdio (you see live output, exit-code returned)
    `sh!`  — capture (returns {:out :err :exit}), throws on non-zero
    `sh*`  — background (returns a process handle you can deref)

  Every subprocess automatically inherits `@console.env/*env*` via
  `:extra-env`. Pass `:extra-env {...}` in opts to override individual
  keys for one call."
  (:require [babashka.process :as p]
            [babashka.fs :as fs]
            [clojure.string :as str]
            [console.path :as path]
            [console.env  :as env]))

(defn repo-root
  "Re-exported for callers that want the repo root."
  []
  (path/repo-root))

(defn in
  "Absolute path of `rel` (a sub-path relative to repo root)."
  [& rel]
  (str (apply fs/path (path/repo-root) rel)))

(defn- with-env
  "Make `@env/*env*` the default `:extra-env`. Per-call `:extra-env` wins."
  [opts]
  (assoc opts :extra-env (merge (env/as-map) (:extra-env opts))))

(defn- in-repo [opts]
  (-> (merge {:dir (path/repo-root) :inherit true} opts)
      with-env))

(defn sh
  "Run a command with inherited stdio. Returns the exit code.
  Accepts either a vector of args or a single string (which is split)."
  ([cmd] (sh cmd {}))
  ([cmd opts]
   (let [args (cond
                (vector? cmd) (mapv str cmd)
                (string? cmd) (str/split cmd #"\s+")
                :else (throw (ex-info "cmd must be string or vector" {:cmd cmd})))
         proc (p/process args (in-repo opts))]
     (:exit @proc))))

(defn sh!
  "Like `sh` but captures stdout/stderr and throws on non-zero exit."
  ([cmd] (sh! cmd {}))
  ([cmd opts]
   (let [args (if (vector? cmd) (mapv str cmd) (str/split cmd #"\s+"))
         opts (-> {:dir (path/repo-root) :out :string :err :string}
                  (merge opts)
                  with-env)]
     @(p/process args opts))))

(defn sh*
  "Run in the background. Returns a process handle (deref for exit info)."
  ([cmd] (sh* cmd {}))
  ([cmd opts]
   (let [args (if (vector? cmd) (mapv str cmd) (str/split cmd #"\s+"))]
     (p/process args (in-repo opts)))))

;; ── project-specific shortcuts ───────────────────────────────────────

(defn make
  "Run `make <targets...>` inside a build dir relative to repo root.

      (make \"build-lib\" \"lib\")   ;; cd build-lib && make lib"
  [build-dir & targets]
  (sh (into ["make"] (map str targets)) {:dir (in build-dir)}))

(defn cargo
  "Run `cargo <args...>` from repo root.

      (cargo \"build\" \"--release\" \"-p\" \"rockbox-cli\")"
  [& args]
  (sh (into ["cargo"] (map str args))))

(defn cargo-build
  "`cargo build --release -p <crate> [-p <crate> ...]`."
  [& crates]
  (apply cargo "build" "--release" (mapcat (fn [c] ["-p" c]) crates)))

(defn zig
  "Run `zig <args...>` inside the `zig/` dir.

      (zig \"build\")       ;; links zig-out/bin/rockboxd
      (zig \"build\" \"lib\") ;; librockboxd.a"
  [& args]
  (sh (into ["zig"] (map str args)) {:dir (in "zig")}))

(defn bun
  "`bun <args...>` inside `dir` (relative to repo root)."
  [dir & args]
  (sh (into ["bun"] (map str args)) {:dir (in dir)}))

(defn bunx
  "`bunx <args...>` inside `dir` (relative to repo root)."
  [dir & args]
  (sh (into ["bunx"] (map str args)) {:dir (in dir)}))

(defn bash
  "`bash <script> [args...]` from repo root (script path is repo-relative)."
  [script & args]
  (sh (into ["bash" (in script)] (map str args))))
