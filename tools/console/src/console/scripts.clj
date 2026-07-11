(ns console.scripts
  "Every shell script under `<repo>/scripts/*.sh`, auto-discovered and
  runnable from the REPL. No hand-maintained list — drop a new `.sh` in
  `scripts/` and it shows up here on next load.

      (scripts/ls)                    ;; list every scripts/*.sh
      (scripts/run \"build-wasm\")       ;; bash scripts/build-wasm.sh
      (scripts/run \"build-armhf\" \"--clean\")

  For convenience, each discovered script is also interned as a plain
  function on this namespace (dashes preserved), so you can call it
  directly:

      (scripts/build-wasm)
      (scripts/build-headless)
      (scripts/flatten-archive \"...\")"
  (:require [console.shell :as sh]
            [babashka.fs :as fs]
            [clojure.string :as str]))

(defn- scripts-dir [] (sh/in "scripts"))

(defn names
  "Sorted vector of script base names (without the `.sh` extension)."
  []
  (->> (fs/glob (scripts-dir) "*.sh")
       (map (comp #(str/replace % #"\.sh$" "") str fs/file-name))
       sort
       vec))

(defn ls
  "Print every script found in `scripts/`."
  []
  (println (str "scripts/  (" (scripts-dir) ")"))
  (doseq [n (names)]
    (println "  " n))
  :ok)

(defn run
  "Run `bash scripts/<name>.sh [args...]` from the repo root. The `.sh`
  suffix is optional. Extra args pass straight through to the script."
  [name & args]
  (let [base (str/replace (str name) #"\.sh$" "")
        file (str "scripts/" base ".sh")]
    (when-not (fs/exists? (sh/in file))
      (throw (ex-info (str "No such script: " file)
                      {:name name :available (names)})))
    (apply sh/bash file args)))

;; ── intern one fn per script so `(scripts/build-wasm)` works ──────────

(defn- intern-scripts! []
  (doseq [n (try (names) (catch Exception _ nil))]
    (let [sym (symbol n)]
      (intern *ns* (with-meta sym
                     {:doc (str "Run scripts/" n ".sh (auto-generated). "
                                "Extra args pass through.")
                      :arglists '([& args])})
              (fn [& args] (apply run n args))))))

(intern-scripts!)
