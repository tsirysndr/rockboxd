(ns console.build
  "The rockboxd build pipeline. Three stages link into one binary:

    1. C firmware   (Make)  -> build-lib/libfirmware.a & friends
    2. Rust crates  (Cargo) -> target/release/librockbox_{cli,server}.a
    3. Zig linker   (Zig)   -> zig/zig-out/bin/rockboxd

  Stale-binary pitfall: `zig build` only re-links when the `.a` files are
  newer than the binary. After touching C, run `(firmware)` first; after
  touching Rust, run `(cli)`/`(server)`. `(all)` does the whole chain in
  order. `(console.verify/stale)` prints the relevant timestamps.

  REPL examples:

      (build/all)          ;; firmware + cli + server + zig -> rockboxd
      (build/firmware)     ;; just `make lib` in build-lib
      (build/zig)          ;; just re-link
      (build/lib)          ;; embeddable librockboxd.a (headless chain)
      (build/gpui)         ;; desktop client"
  (:require [console.shell :as sh]))

;; ── stage 1: C firmware ──────────────────────────────────────────────

(defn firmware
  "Stage 1 — build the SDL firmware: `cd build-lib && make lib`.
  Produces libfirmware.a, librockbox.a and the codec libs."
  []
  (sh/make "build-lib" "lib"))

(defn headless
  "Build the headless (no-SDL) firmware for the embeddable library:
  `cd build-headless && make lib`."
  []
  (sh/make "build-headless" "lib"))

;; ── stage 2: Rust crates ─────────────────────────────────────────────

(defn cli
  "Stage 2 — `cargo build --release -p rockbox-cli`
  (produces target/release/librockbox_cli.a)."
  []
  (sh/cargo-build "rockbox-cli"))

(defn server
  "Stage 2 — `cargo build --release -p rockbox-server`
  (produces target/release/librockbox_server.a)."
  []
  (sh/cargo-build "rockbox-server"))

(defn crates
  "Stage 2 — build both staticlib crates in one cargo invocation."
  []
  (sh/cargo-build "rockbox-cli" "rockbox-server"))

;; ── stage 3: Zig linker ──────────────────────────────────────────────

(defn zig
  "Stage 3 — `cd zig && zig build` (links zig-out/bin/rockboxd)."
  []
  (sh/zig "build"))

;; ── full pipelines ───────────────────────────────────────────────────

(defn all
  "Full rebuild in dependency order: firmware -> crates -> zig.
  Stops at the first stage that fails (non-zero exit)."
  []
  (let [steps [["firmware" firmware] ["crates" crates] ["zig" zig]]]
    (reduce (fn [_ [label f]]
              (println (str "\n▶ build: " label))
              (let [code (f)]
                (if (zero? code)
                  code
                  (reduced (do (println (str "✗ " label " failed (exit " code ")"))
                               code)))))
            0 steps)))

(defn lib
  "Build the embeddable `librockboxd.a` fat archive (headless/cpal, no SDL):
  headless firmware -> rockbox-embed + rockbox-server -> `zig build lib`.

  Output: zig/zig-out/lib/librockboxd.a. Consumed by the GPUI client."
  []
  (let [steps [["headless firmware" headless]
               ["embed+server"      #(sh/cargo-build "rockbox-embed" "rockbox-server")]
               ["zig build lib"     #(sh/zig "build" "lib")]]]
    (reduce (fn [_ [label f]]
              (println (str "\n▶ lib: " label))
              (let [code (f)]
                (if (zero? code) code
                    (reduced (do (println (str "✗ " label " failed (exit " code ")"))
                                 code)))))
            0 steps)))

;; ── extra targets ────────────────────────────────────────────────────

(defn gpui
  "Build the GPUI desktop client (`cd gpui && cargo build --release`).
  Links librockboxd.a automatically via gpui/build.rs — run `(lib)` first
  if the embeddable archive is stale."
  []
  (sh/cargo "build" "--release" "--manifest-path" (sh/in "gpui" "Cargo.toml")))

(defn armhf
  "Cross-build the firmware for armhf via scripts/build-armhf.sh."
  []
  (sh/bash "scripts/build-armhf.sh"))
