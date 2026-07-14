(ns console.wasm
  "WASM browser build. Compiles the extracted core crates (rockbox-codecs +
  rockbox-dsp + rockbox-metadata, via the rockbox-ffi flat C ABI) into
  web/rockbox-core.{js,wasm} with Emscripten, and serves web/ with the
  COOP/COEP headers SharedArrayBuffer needs. The player itself is plain JS
  under web/ — see WEBASSEMBLY.md.

      (wasm/build)   ;; bash scripts/build-wasm.sh
      (wasm/serve)   ;; node scripts/wasm-dev-server.mjs
      (wasm/dev)     ;; build, then serve the web example

  Reminder: adding a new `#[no_mangle]` export in rockbox-ffi also needs an
  entry in EXPORTED_FUNCTIONS inside scripts/build-wasm.sh, then a rebuild —
  a Rust-only recompile won't re-run the emcc link step."
  (:require [console.shell :as sh]))

(defn build
  "Compile the WASM decode + DSP core via scripts/build-wasm.sh."
  []
  (sh/bash "scripts/build-wasm.sh"))

(defn serve
  "Serve web/ over HTTP with COOP/COEP headers (node scripts/wasm-dev-server.mjs)."
  []
  (sh/sh ["node" (sh/in "scripts/wasm-dev-server.mjs")]))

(defn dev
  "Build the core, then serve the web example (Ctrl-C to stop the server)."
  []
  (build)
  (println "\n▶ Web example: open http://localhost:8090 (Ctrl-C to stop)\n")
  (serve))
