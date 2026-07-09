(ns build
  "Build + deploy the rockbox-ffi Clojure jar to Clojars.

  Mirrors the sibling org.clojars.tsiry/rockbox-clj SDK's build (plain <scm>
  url + a version-derived <tag>) — that config builds cleanly on cljdoc.org.

  The jar bundles the prebuilt librockbox_ffi for every OS/arch under
  resources/native/<target>/ (staged by ../scripts/fetch-libs.sh from the GitHub
  release), so consumers need no Rust toolchain and no separate shared library.

  cljdoc renders ONE repo-root doc/cljdoc.edn per checked-out tag (it has no
  per-subdir config). The two Clojure artifacts in this monorepo each release
  from their own tag (clojure-ffi-v* here, clojure-v* for the SDK), so `release`
  stamps the repo-root doc/cljdoc.edn to point at THIS binding's README before
  tagging — the SDK's build does the same for its README. No cross-contamination.

  Group `io.github.tsirysndr` must be verified on Clojars (Verified Groups ->
  GitHub), or swap `lib` to `org.clojars.tsirysndr/rockbox-clj-ffi`.

  Usage:
    clojure -T:build jar
    VERSION=0.1.2 clojure -T:build install                 # -> local ~/.m2
    VERSION=0.1.2 CLOJARS_USERNAME=<user> CLOJARS_PASSWORD=<token> \\
      clojure -T:build release                             # stamp+tag+deploy+cljdoc
  Lower-level: `stamp-cljdoc`, `deploy` (Clojars only), `request-cljdoc`."
  (:require [clojure.tools.build.api :as b]
            [clojure.java.io :as io]
            [deps-deploy.deps-deploy :as dd]))

(def lib 'io.github.tsirysndr/rockbox-clj-ffi)
(def version (or (System/getenv "VERSION") "0.1.2"))
(def tag (str "clojure-ffi-v" version))
(def class-dir "target/classes")
(def basis (delay (b/create-basis {:project "deps.edn"})))
(def jar-file (format "target/%s-%s.jar" (name lib) version))

;; build.clj runs from bindings/clojure/, so the git root is two levels up.
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
                ;; Own release tag: clojure-ffi-v<version> — distinct from the
                ;; SDK's clojure-v* and the shared bindings-v*.
                :scm       {:url                 "https://github.com/tsirysndr/rockboxd"
                            :connection          "scm:git:git://github.com/tsirysndr/rockboxd.git"
                            :developerConnection "scm:git:ssh://git@github.com/tsirysndr/rockboxd.git"
                            :tag                 tag}
                :pom-data  [[:description "Clojure bindings for the Rockbox DSP, metadata, and playback engine (Java FFM over the shared rockbox-ffi C ABI)."]
                            [:url "https://github.com/tsirysndr/rockboxd"]
                            [:licenses
                             [:license
                              [:name "GPL-2.0-or-later"]
                              [:url "https://www.gnu.org/licenses/old-licenses/gpl-2.0.html"]]]]})
  ;; src (the .clj namespaces) + resources/native/<target>/librockbox_ffi.*
  (b/copy-dir {:src-dirs   ["src" "resources"]
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
  "Point the repo-root doc/cljdoc.edn at THIS binding's README, so cljdoc renders
  it for the clojure-ffi-v* release tag. `release` runs this before tagging; the
  SDK's build stamps its own README for its clojure-v* tags."
  [_]
  (io/make-parents cljdoc-edn)
  (spit cljdoc-edn
        (str ";; Generated at release time by bindings/clojure/build.clj.\n"
             ";; Scopes cljdoc's Readme to this binding for its clojure-ffi-v* tag;\n"
             ";; the SDK's build re-stamps sdk/clojure/README.md for its own tags.\n"
             "{:cljdoc.doc/tree [[\"Readme\"   {:file \"bindings/clojure/README.md\"}]\n"
             "                   [\"Examples\" {:file \"bindings/clojure/examples/README.md\"}]]}\n"))
  (println "stamped" cljdoc-edn "-> bindings/clojure/README.md"))

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
  "Full release: stamp the cljdoc config, commit + tag clojure-ffi-v<version>,
  deploy to Clojars, push the tag, then request a cljdoc build. Requires a clean
  working tree and CLOJARS_USERNAME/PASSWORD."
  [_]
  (when (seq (str (git "status" "--porcelain")))
    (throw (ex-info "working tree not clean — commit or stash first" {})))
  (stamp-cljdoc nil)
  (git "add" "doc/cljdoc.edn")
  (git "commit" "-m" (str "docs(cljdoc): " tag " -> bindings/clojure/README.md"))
  (git "tag" tag)
  (deploy nil)
  (git "push" "origin" tag)
  (request-cljdoc nil)
  (println "released" (str lib) version "as" tag))
