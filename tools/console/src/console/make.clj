(ns console.make
  "Direct access to the Rockbox Make build dirs and the `tools/configure`
  target generator — the escape hatches beneath `console.build`.

      (make/make \"build-lib\" \"lib\")   ;; cd build-lib && make lib
      (make/lib)                       ;; make lib in build-lib (default)
      (make/clean \"build-headless\")    ;; make clean
      (make/configure \"build-lib\")     ;; regenerate the Makefile (careful!)"
  (:refer-clojure :exclude [make])
  (:require [console.shell :as sh]))

(defn make
  "Run `make <targets...>` inside `build-dir` (relative to repo root).
  With just a build dir, runs bare `make` (default target)."
  [build-dir & targets]
  (apply sh/make build-dir targets))

(defn lib
  "`make lib` inside a build dir (defaults to build-lib)."
  ([] (lib "build-lib"))
  ([build-dir] (sh/make build-dir "lib")))

(defn clean
  "`make clean` inside a build dir (defaults to build-lib)."
  ([] (clean "build-lib"))
  ([build-dir] (sh/make build-dir "clean")))

(defn configure
  "Run `tools/configure` from inside `build-dir` to (re)generate its
  Makefile for a target. Extra args are passed to configure.

  ⚠ build-lib and build-headless were pre-configured for their targets;
  re-running configure overwrites the Makefile and any local edits. Only
  do this if you know what you're doing (see CLAUDE.md → Build system)."
  [build-dir & args]
  (sh/sh (into [(sh/in "tools/configure")] (map str args))
         {:dir (sh/in build-dir)}))
