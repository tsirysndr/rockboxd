(ns console.path
  "Repo-root discovery. Lives in its own namespace so both `console.env`
  and `console.shell` can use it without forming a cycle."
  (:require [babashka.fs :as fs]))

(defn repo-root
  "Walk up from cwd until we find the rockboxd monorepo root, identified
  by the Zig build script (`zig/build.zig`) sitting next to the workspace
  `Cargo.toml`. Both markers together disambiguate the root from any nested
  crate directory (which also has its own `Cargo.toml`)."
  []
  (loop [dir (fs/absolutize (fs/cwd))]
    (cond
      (nil? dir)
      (throw (ex-info "Could not locate rockboxd repo root"
                      {:cwd (str (fs/cwd))}))

      (and (fs/exists? (fs/path dir "Cargo.toml"))
           (fs/exists? (fs/path dir "zig" "build.zig")))
      (str dir)

      :else (recur (fs/parent dir)))))
