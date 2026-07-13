(ns console.bindings
  "Multi-language bindings around the shared C ABI in `crates/rockbox-ffi`
  (cdylib + staticlib), consumed by bindings/{python,typescript,ruby,go,
  kotlin,clojure,elixir,gleam,swift}. bindings/erlang is the shared native NIF
  package (rockbox_ffi_nif) that the Elixir + Gleam bindings depend on.

      (bindings/ffi)          ;; cargo build --release -p rockbox-ffi
      (bindings/fetch-libs)   ;; stage prebuilt libs from a GH release
      (bindings/publish :npm) ;; publish npm packages from a GH release
      (bindings/publish :python)
      (bindings/publish :ruby)
      (bindings/publish :clojure)  ;; -> Clojars       (jar bundles prebuilt libs)
      (bindings/publish :kotlin)   ;; -> Maven Central  (jar bundles prebuilt libs)
      (bindings/publish :erlang)   ;; -> Hex   shared NIF; publish BEFORE elixir/gleam
      (bindings/publish :elixir)   ;; -> Hex   wrappers only; depends on :erlang
      (bindings/publish :gleam)    ;; -> Hex   wrappers only; depends on :erlang"
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
  source, bundling every platform's prebuilt lib staged from the Release.

  The BEAM bindings share one native package: :erlang (rockbox_ffi_nif) owns
  the NIF and must be published FIRST — it writes the checksum manifest from the
  `erlang-v<version>` GitHub Release, then `rebar3 hex publish`. :elixir and
  :gleam then ship wrappers only (no native build); each pins its dependency on
  the released rockbox_ffi_nif version.

      (bindings/publish :npm)
      (bindings/publish :python \"--dry-run\")
      (bindings/publish :ruby)
      (bindings/publish :clojure)
      (bindings/publish :kotlin \"--tag\" \"bindings-v0.2.0\")
      (bindings/publish :erlang)   ;; publish this before :elixir / :gleam
      (bindings/publish :elixir)
      (bindings/publish :gleam \"--dry-run\")"
  [lang & args]
  (let [script (case (keyword lang)
                 :npm     "bindings/scripts/publish-npm.sh"
                 :python  "bindings/scripts/publish-python.sh"
                 :ruby    "bindings/scripts/publish-ruby.sh"
                 :clojure "bindings/scripts/publish-clojure.sh"
                 :kotlin  "bindings/scripts/publish-kotlin.sh"
                 :erlang  "bindings/scripts/publish-erlang.sh"
                 :elixir  "bindings/scripts/publish-elixir.sh"
                 :gleam   "bindings/scripts/publish-gleam.sh"
                 (throw (ex-info "Unknown binding target"
                                 {:lang lang
                                  :known [:npm :python :ruby :clojure :kotlin
                                          :erlang :elixir :gleam]})))]
    (apply sh/bash script args)))
