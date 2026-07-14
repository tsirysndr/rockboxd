(ns console.wasm
  "WASM browser package (bindings/wasm). Compiles the extracted core crates
  (rockbox-codecs + rockbox-dsp + rockbox-metadata, via the rockbox-ffi flat C
  ABI) to a single-threaded WebAssembly module with the .wasm embedded, and
  ships it as the `rockbox-wasm` npm package plus a Vite/React/TS example.

      (wasm/build)     ;; bash bindings/wasm/scripts/build.sh  -> dist/
      (wasm/example)   ;; run the Vite React example (npm install + dev)
      (wasm/dev)       ;; build the package, then run the example
      (wasm/publish)   ;; build, then npm publish (args pass through)

  No COOP/COEP headers are needed — the build is single-threaded (no
  SharedArrayBuffer). Adding a new `#[no_mangle]` export in rockbox-ffi also
  needs an entry in EXPORTED_FUNCTIONS inside scripts/build-wasm.sh."
  (:require [console.shell :as sh]))

(defn build
  "Build the rockbox-wasm npm package (embeds wasm into dist/rockbox-core.js)."
  []
  (sh/bash "bindings/wasm/scripts/build.sh"))

(defn example
  "Run the Vite + React + TypeScript example (npm install, then dev server)."
  []
  (sh/sh ["bash" "-c" "cd bindings/wasm/example && npm install && npm run dev"]))

(defn dev
  "Build the package, then run the example (Ctrl-C to stop)."
  []
  (build)
  (println "\n▶ Starting the Vite example (http://localhost:5173)…\n")
  (example))

(defn publish
  "Build the package, then npm publish. Extra args pass through, e.g.
  (wasm/publish \"--dry-run\") or (wasm/publish \"--tag\" \"next\")."
  [& args]
  (sh/sh (into ["bash" (sh/in "bindings/wasm/scripts/publish.sh")] args)))
