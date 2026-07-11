(ns user
  "Auto-loaded REPL helpers. Drops every console namespace into scope under
  short aliases so you can poke around immediately.

      user=> (help)
      user=> (build/all)
      user=> (run/daemon)
      user=> (scripts/build-wasm)
      user=> (verify/stale)"
  (:require [console.core    :as c]
            [console.shell   :as sh]
            [console.env     :as env]
            [console.build   :as build]
            [console.make    :as make]
            [console.run     :as run]
            [console.dev     :as dev]
            [console.wasm    :as wasm]
            [console.expo    :as expo]
            [console.bindings :as bindings]
            [console.verify  :as verify]
            [console.scripts :as scripts]))

(def help c/help)
(def ls   c/ls)

;; Auto-load <repo>/.env on REPL startup. Silent if the file is missing,
;; so a fresh clone still drops into a usable REPL.
(let [n (try (env/load!) (catch Exception _ nil))]
  (println)
  (println "Rockbox Console — REPL loaded. Try (help) or (ls).")
  (println "Aliases: c, sh, env, build, make, run, dev, wasm, expo, bindings, verify, scripts")
  (when n
    (println (str "Loaded " n " env vars from .env — `(env/show)` to inspect.")))
  (println))
