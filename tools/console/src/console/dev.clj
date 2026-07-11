(ns console.dev
  "Workspace code-quality shortcuts for the Rust crates.

      (dev/fmt)        ;; cargo fmt --all
      (dev/fmt-check)  ;; cargo fmt --all --check (CI-style)
      (dev/clippy)     ;; cargo clippy --workspace
      (dev/test)       ;; cargo test --workspace
      (dev/check)      ;; cargo check --workspace
      (dev/build)      ;; cargo build --release --workspace"
  (:refer-clojure :exclude [test])
  (:require [console.shell :as sh]))

(defn fmt        [] (sh/cargo "fmt" "--all"))
(defn fmt-check  [] (sh/cargo "fmt" "--all" "--check"))
(defn clippy     [& args] (apply sh/cargo "clippy" "--workspace" args))
(defn test       [& args] (apply sh/cargo "test" "--workspace" args))
(defn check      [] (sh/cargo "check" "--workspace"))
(defn build      [] (sh/cargo "build" "--release" "--workspace"))
