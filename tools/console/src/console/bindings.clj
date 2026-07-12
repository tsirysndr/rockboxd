(ns console.bindings
  "Multi-language bindings around the shared C ABI in `crates/rockbox-ffi`
  (cdylib + staticlib), consumed by bindings/{python,typescript,ruby,go,
  kotlin,clojure,elixir,gleam,swift}.

      (bindings/ffi)          ;; cargo build --release -p rockbox-ffi
      (bindings/fetch-libs)   ;; stage prebuilt libs from a GH release
      (bindings/publish :npm) ;; publish npm packages from a GH release
      (bindings/publish :python)
      (bindings/publish :ruby)
      (bindings/publish :clojure)  ;; -> Clojars       (jar bundles prebuilt libs)
      (bindings/publish :kotlin)   ;; -> Maven Central  (jar bundles prebuilt libs)
      (bindings/publish :elixir)   ;; -> Hex            (rebuilds rockbox-ffi first)
      (bindings/publish :gleam)    ;; -> Hex            (rebuilds rockbox-ffi first)"
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
  "Publish a language's packages to its registry.

  npm/python/ruby push the prebuilt assets from a GitHub Release. The JVM
  bindings (clojure -> Clojars, kotlin -> Maven Central) build the jar from
  source, bundling every platform's prebuilt lib staged from the Release. The
  BEAM bindings (elixir/gleam -> Hex, incl. HexDocs) ship source and rebuild
  the local rockbox-ffi static archive first (the NIF's compile-time dep).

      (bindings/publish :npm)
      (bindings/publish :python \"--dry-run\")
      (bindings/publish :ruby)
      (bindings/publish :clojure)
      (bindings/publish :kotlin \"--tag\" \"bindings-v0.2.0\")
      (bindings/publish :elixir)
      (bindings/publish :gleam \"--dry-run\")"
  [lang & args]
  (let [script (case (keyword lang)
                 :npm     "bindings/scripts/publish-npm.sh"
                 :python  "bindings/scripts/publish-python.sh"
                 :ruby    "bindings/scripts/publish-ruby.sh"
                 :clojure "bindings/scripts/publish-clojure.sh"
                 :kotlin  "bindings/scripts/publish-kotlin.sh"
                 :elixir  "bindings/scripts/publish-elixir.sh"
                 :gleam   "bindings/scripts/publish-gleam.sh"
                 (throw (ex-info "Unknown binding target"
                                 {:lang lang
                                  :known [:npm :python :ruby
                                          :clojure :kotlin :elixir :gleam]})))]
    (apply sh/bash script args)))
