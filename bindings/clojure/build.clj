(ns build
  "Build + deploy the rockbox-ffi Clojure jar to Clojars.

  The jar bundles the prebuilt librockbox_ffi for every OS/arch under
  resources/native/<target>/ (staged by ../scripts/fetch-libs.sh from the GitHub
  release), so consumers need no Rust toolchain and no separate shared library.

  Group `io.github.tsirysndr` must be verified on Clojars (Verified Groups ->
  GitHub), or swap `lib` to `org.clojars.tsirysndr/rockbox-ffi`. Versions are
  always deployed as -SNAPSHOT (Clojars release versions are immutable).

  Usage:
    clojure -T:build jar
    CLOJARS_USERNAME=<user> CLOJARS_PASSWORD=<deploy-token> clojure -T:build deploy"
  (:require [clojure.tools.build.api :as b]
            [deps-deploy.deps-deploy :as dd]))

(def lib 'io.github.tsirysndr/rockbox-ffi)

;; Clojars: always deploy a -SNAPSHOT. Release versions are immutable on
;; Clojars (a re-deploy is rejected); SNAPSHOTs can be overwritten freely.
(def version
  (let [v (or (System/getenv "ROCKBOX_VERSION") "0.1.0")]
    (cond-> v (not (.endsWith v "-SNAPSHOT")) (str "-SNAPSHOT"))))
(def class-dir "target/classes")
(def basis (delay (b/create-basis {:project "deps.edn"})))
(def jar-file (format "target/%s-%s.jar" (name lib) version))

(defn clean [_]
  (b/delete {:path "target"}))

(defn jar
  "Write the pom + jar (source + bundled native libs) into target/."
  [_]
  (clean nil)
  (b/write-pom
   {:class-dir class-dir
    :lib lib
    :version version
    :basis @basis
    :src-dirs ["src"]
    :pom-data [[:description
                (str "Clojure bindings for the Rockbox DSP, metadata, and playback "
                     "engine (Java FFM over the shared rockbox-ffi C ABI).")]
               [:url "https://github.com/tsirysndr/rockboxd"]
               [:licenses
                [:license
                 [:name "GPL-2.0-or-later"]
                 [:url "https://www.gnu.org/licenses/old-licenses/gpl-2.0.html"]]]
               [:developers
                [:developer
                 [:id "tsirysndr"]
                 [:name "Tsiry Sandratraina"]
                 [:url "https://github.com/tsirysndr"]]]
               [:scm
                [:url "https://github.com/tsirysndr/rockboxd"]
                [:connection "scm:git:git://github.com/tsirysndr/rockboxd.git"]
                [:developerConnection "scm:git:ssh://git@github.com/tsirysndr/rockboxd.git"]]]})
  ;; src (the .clj namespaces) + resources/native/<target>/librockbox_ffi.*
  (b/copy-dir {:src-dirs ["src" "resources"] :target-dir class-dir})
  (b/jar {:class-dir class-dir :jar-file jar-file})
  (println "wrote" jar-file))

(defn deploy
  "Build the jar, then push it to Clojars (needs CLOJARS_USERNAME/PASSWORD)."
  [_]
  (jar nil)
  (dd/deploy {:installer :remote
              :artifact jar-file
              :pom-file (b/pom-path {:lib lib :class-dir class-dir})}))
