(ns console.wasm
  "WASM browser build. Compiles the Rockbox C firmware + the `crates/wasm`
  shim into web/rockboxd.{js,wasm} via Emscripten, and serves web/ with the
  COOP/COEP headers SharedArrayBuffer needs.

      (wasm/build)   ;; bash scripts/build-wasm.sh
      (wasm/serve)   ;; node scripts/wasm-dev-server.mjs

  Reminder: adding a new `#[no_mangle]` export in crates/wasm also needs an
  entry in EXPORTED_FUNCTIONS inside scripts/build-wasm.sh, then a rebuild —
  a Rust-only recompile won't re-run the emcc link step."
  (:require [console.shell :as sh]))

(defn build
  "Compile the WASM build via scripts/build-wasm.sh."
  []
  (sh/bash "scripts/build-wasm.sh"))

(defn serve
  "Serve web/ over HTTP with COOP/COEP headers (node scripts/wasm-dev-server.mjs)."
  []
  (sh/sh ["node" (sh/in "scripts/wasm-dev-server.mjs")]))
