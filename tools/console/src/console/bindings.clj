(ns console.bindings
  "Multi-language bindings around the shared C ABI in `crates/rockbox-ffi`
  (cdylib + staticlib), consumed by bindings/{python,typescript,ruby,go,
  kotlin,clojure,elixir,gleam,swift}.

      (bindings/ffi)          ;; cargo build --release -p rockbox-ffi
      (bindings/fetch-libs)   ;; stage prebuilt libs from a GH release
      (bindings/publish :npm) ;; publish npm packages from a GH release
      (bindings/publish :python)
      (bindings/publish :ruby)"
  (:require [console.shell :as sh]))

(defn ffi
  "Build the shared FFI library: `cargo build --release -p rockbox-ffi`.
  Produces the cdylib (librockbox_ffi.{dylib,so}) + staticlib every binding
  links against."
  [& args]
  (apply sh/cargo-build "rockbox-ffi" args))

(defn fetch-libs
  "Download prebuilt librockbox_ffi shared libs from a GitHub Release and
  stage them into each binding. Extra args pass through, e.g.

      (bindings/fetch-libs \"--all\")
      (bindings/fetch-libs \"--target\" \"linux-x64\")"
  [& args]
  (apply sh/bash "bindings/scripts/fetch-libs.sh" args))

(defn publish
  "Publish a language's packages from a GitHub Release.

      (bindings/publish :npm)
      (bindings/publish :python \"--dry-run\")
      (bindings/publish :ruby)"
  [lang & args]
  (let [script (case (keyword lang)
                 :npm    "bindings/scripts/publish-npm.sh"
                 :python "bindings/scripts/publish-python.sh"
                 :ruby   "bindings/scripts/publish-ruby.sh"
                 (throw (ex-info "Unknown binding target"
                                 {:lang lang :known [:npm :python :ruby]})))]
    (apply sh/bash script args)))
