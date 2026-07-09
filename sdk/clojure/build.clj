(ns build
  "Build + deploy the rockbox-clj SDK jar to Clojars.

  cljdoc renders ONE repo-root doc/cljdoc.edn per checked-out tag (no per-subdir
  config). This monorepo also ships io.github.tsirysndr/rockbox-ffi (tag
  clojure-ffi-v*). Each artifact releases from its own tag, and `release` stamps
  the repo-root doc/cljdoc.edn to point at THIS SDK's README before tagging, so
  cljdoc renders the right README for clojure-v* — no cross-contamination with
  the FFI binding.

  Usage:
    clojure -T:build jar
    VERSION=0.1.0 CLOJARS_USERNAME=<user> CLOJARS_PASSWORD=<token> \\
      clojure -T:build release                    # stamp+tag+deploy+cljdoc"
  (:require [clojure.tools.build.api :as b]
            [clojure.java.io :as io]
            [deps-deploy.deps-deploy :as dd]))

(def lib 'org.clojars.tsiry/rockbox-clj)
(def version (or (System/getenv "VERSION") "0.1.0"))
(def tag (str "clojure-v" version))
(def class-dir "target/classes")
(def basis (delay (b/create-basis {:project "deps.edn"})))
(def jar-file (format "target/%s-%s.jar" (name lib) version))

;; build.clj runs from sdk/clojure/, so the git root is two levels up.
(def repo-root "../..")
(def cljdoc-edn (str repo-root "/doc/cljdoc.edn"))

(defn clean [_]
  (b/delete {:path "target"}))

(defn jar [_]
  (b/write-pom {:class-dir class-dir
                :lib       lib
                :version   version
                :basis     @basis
                :src-dirs  ["src"]
                :scm       {:url                 "https://github.com/tsirysndr/rockboxd"
                            :connection          "scm:git:git://github.com/tsirysndr/rockboxd.git"
                            :developerConnection "scm:git:ssh://git@github.com/tsirysndr/rockboxd.git"
                            :tag                 tag}
                :pom-data  [[:description "Idiomatic Clojure SDK for Rockbox — GraphQL client with WebSocket subscriptions and a tiny plugin system."]
                            [:url "https://github.com/tsirysndr/rockboxd"]
                            [:licenses
                             [:license
                              [:name "MIT License"]
                              [:url "https://opensource.org/licenses/MIT"]]]]})
  (b/copy-dir {:src-dirs   ["src"]
               :target-dir class-dir})
  (b/jar {:class-dir class-dir
          :jar-file  jar-file}))

(defn install [_]
  (clean nil)
  (jar nil)
  (b/install {:basis     @basis
              :lib       lib
              :version   version
              :jar-file  jar-file
              :class-dir class-dir}))

(defn deploy [_]
  (clean nil)
  (jar nil)
  (dd/deploy {:installer :remote
              :artifact  (b/resolve-path jar-file)
              :pom-file  (b/pom-path {:lib lib :class-dir class-dir})}))

;; ---- cljdoc (repo-root config + build trigger) ------------------------

(defn stamp-cljdoc
  "Point the repo-root doc/cljdoc.edn at THIS SDK's README + examples, so cljdoc
  renders them for the clojure-v* release tag. `release` runs this before
  tagging; the rockbox-ffi binding stamps its own README for clojure-ffi-v* tags."
  [_]
  (io/make-parents cljdoc-edn)
  (spit cljdoc-edn
        (str ";; Generated at release time by sdk/clojure/build.clj.\n"
             ";; Scopes cljdoc's Readme to this SDK for its clojure-v* tag;\n"
             ";; the rockbox-ffi binding re-stamps its own README for clojure-ffi-v* tags.\n"
             "{:cljdoc.doc/tree [[\"Readme\"   {:file \"sdk/clojure/README.md\"}]\n"
             "                   [\"Examples\" {:file \"sdk/clojure/examples/README.md\"}]]}\n"))
  (println "stamped" cljdoc-edn "-> sdk/clojure/README.md"))

(defn- git [& args]
  (b/git-process {:dir repo-root :git-args (vec args)}))

(defn request-cljdoc
  "Ask cljdoc.org to (re)build docs for this project + version."
  [_]
  (let [body (str "project=" lib "&version=" version)
        req  (-> (java.net.http.HttpRequest/newBuilder
                  (java.net.URI/create "https://cljdoc.org/api/request-build2"))
                 (.header "Content-Type" "application/x-www-form-urlencoded")
                 (.POST (java.net.http.HttpRequest$BodyPublishers/ofString body))
                 (.build))
        resp (.send (java.net.http.HttpClient/newHttpClient)
                    req (java.net.http.HttpResponse$BodyHandlers/ofString))]
    (println "cljdoc build requested:" (.statusCode resp))))

(defn release
  "Full release: stamp the cljdoc config, commit + tag clojure-v<version>, deploy
  to Clojars, push the tag, then request a cljdoc build. Requires a clean working
  tree and CLOJARS_USERNAME/PASSWORD."
  [_]
  (when (seq (str (git "status" "--porcelain")))
    (throw (ex-info "working tree not clean — commit or stash first" {})))
  (stamp-cljdoc nil)
  (git "add" "doc/cljdoc.edn")
  (git "commit" "-m" (str "docs(cljdoc): " tag " -> sdk/clojure/README.md"))
  (git "tag" tag)
  (deploy nil)
  (git "push" "origin" tag)
  (request-cljdoc nil)
  (println "released" (str lib) version "as" tag))
