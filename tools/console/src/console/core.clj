(ns console.core
  "Rockbox console — a centralized REPL for every build/dev/ops command in
  the monorepo. One place to build the firmware, link rockboxd, run the
  daemon, drive the WASM + Expo targets, and publish the language bindings.

  Quick start (REPL):
      (require '[console.core :as c])
      (c/help)   ;; or (c/ls)

  Or as one-shot babashka tasks:
      $ bb build:all
      $ bb run
      $ bb wasm:build"
  (:require [console.shell :as sh]
            [console.scripts :as scripts]))

(def ^:private registry
  "Hand-written index of every command grouped by namespace. Keeps `(help)`
  cheap and discoverable — namespaces are still loaded lazily. The
  `scripts` group is generated at runtime from `scripts/*.sh`."
  [{:group "build" :ns 'console.build
    :cmds [[:all            "Full rebuild: firmware -> crates -> zig -> rockboxd"]
           [:firmware       "Stage 1: make lib in build-lib (SDL firmware)"]
           [:cli            "Stage 2: cargo build --release -p rockbox-cli"]
           [:server         "Stage 2: cargo build --release -p rockbox-server"]
           [:crates         "Stage 2: build cli + server in one cargo run"]
           [:zig            "Stage 3: cd zig && zig build -> rockboxd"]
           [:lib            "Embeddable librockboxd.a (headless -> embed -> zig lib)"]
           [:headless       "Headless (no-SDL) firmware for the embeddable lib"]
           [:gpui           "GPUI desktop client (links librockboxd.a)"]
           [:armhf          "Cross-build firmware for armhf"]]}

   {:group "make" :ns 'console.make
    :cmds [[:make           "make <targets> in a build dir. Args: build-dir & targets"]
           [:lib            "make lib in a build dir (default build-lib)"]
           [:clean          "make clean in a build dir (default build-lib)"]
           [:configure      "tools/configure in a build dir — regenerates Makefile (careful!)"]]}

   {:group "run" :ns 'console.run
    :cmds [[:daemon         "Run ./zig/zig-out/bin/rockboxd"]
           [:debug          "Run with RUST_LOG (default debug). Args: [rust-log]"]
           [:pipe           "FIFO stdout mode piped into ffplay"]]}

   {:group "dev" :ns 'console.dev
    :cmds [[:fmt            "cargo fmt --all"]
           [:fmt-check      "cargo fmt --all --check"]
           [:clippy         "cargo clippy --workspace"]
           [:test           "cargo test --workspace"]
           [:check          "cargo check --workspace"]
           [:build          "cargo build --release --workspace"]]}

   {:group "wasm" :ns 'console.wasm
    :cmds [[:build          "bash bindings/wasm/scripts/build.sh -> rockbox-wasm dist/"]
           [:example        "run the Vite + React + TS example (npm install + dev)"]
           [:dev            "build the package, then run the example"]
           [:publish        "build, then npm publish (args pass through)"]]}

   {:group "expo" :ns 'console.expo
    :cmds [[:install        "bun install"]
           [:start          "bun run start (Metro / expo-router)"]
           [:typecheck      "bunx tsc --noEmit"]
           [:lint           "bunx expo lint"]
           [:export-web     "bunx expo export --platform web (smoke test)"]
           [:ios            "Build RockboxExpo.xcframework"]
           [:android        "Build librockbox_expo.so (embedded-daemon)"]
           [:prebuild       "bunx expo prebuild"]
           [:run-ios        "bunx expo run:ios"]
           [:run-android    "bunx expo run:android"]]}

   {:group "bindings" :ns 'console.bindings
    :cmds [[:ffi            "cargo build --release -p rockbox-ffi"]
           [:fetch-libs     "Stage prebuilt libs from a GH release"]
           [:publish        "Publish a binding's packages. Args: lang [flags]"]]}

   {:group "verify" :ns 'console.verify
    :cmds [[:stale          "Binary vs .a timestamps (stale-binary pitfall)"]
           [:symbols        "airplay + squeezelite symbols present in rockboxd"]
           [:staticlib      "airplay + slim object files inside librockbox_cli.a"]
           [:nm             "grep the binary's symbol table. Args: pattern"]
           [:ar             "grep the cli staticlib's members. Args: pattern"]]}

   {:group "env" :ns 'console.env
    :cmds [[:load!          "Load a .env file (defaults to <repo>/.env)"]
           [:set!           "Set one key. Args: key value (in-memory)"]
           [:unset!         "Remove one key. Args: key"]
           [:show           "Print loaded keys (masked). (show :unmask) for raw"]
           [:get            "Fetch one value (raw). Args: key [default]"]
           [:save!          "Write current env to disk (default <repo>/.env.local)"]]}])

(defn- pad [s n] (let [s (str s)] (str s (apply str (repeat (max 0 (- n (count s))) " ")))))

(defn- print-group [{:keys [group ns cmds]}]
  (println)
  (println (str "── " group "  (" ns ") ──"))
  (doseq [[sym desc] cmds]
    (println " " (pad sym 22) "  " desc)))

(defn ls
  "Print every registered command, grouped by namespace, with a one-liner.
  The `scripts` group is discovered live from `scripts/*.sh`."
  []
  (doseq [g registry] (print-group g))
  ;; Dynamic scripts group.
  (println)
  (println (str "── scripts  (console.scripts — bash scripts/*.sh) ──"))
  (doseq [n (try (scripts/names) (catch Exception _ nil))]
    (println " " (pad n 22) "  " (str "bash scripts/" n ".sh")))
  :ok)

(defn help
  "Pretty banner + ls. Use this from the REPL for a quick tour."
  []
  (println)
  (println "Rockbox Console — REPL-driven ops for the whole monorepo")
  (println "    (require '[console.build :as build])")
  (println "    (build/all)   ;; then (run/daemon)")
  (println)
  (println "Commands:")
  (ls)
  (println)
  (println "From shell:   bb <task>     (see `bb tasks`)")
  (println "Repo root:   " (sh/repo-root))
  :ok)

(defn dispatch
  "Entry point for `clj -X console.core/dispatch :cmd :build/all :args []`."
  [{:keys [cmd args] :or {args []}}]
  (let [[grp sym] ((juxt namespace name) cmd)
        ns-sym    (symbol (str "console." grp))]
    (require ns-sym)
    (let [f (ns-resolve ns-sym (symbol sym))]
      (when-not f
        (throw (ex-info (str "Unknown command: " cmd) {:cmd cmd})))
      (apply f args))))
