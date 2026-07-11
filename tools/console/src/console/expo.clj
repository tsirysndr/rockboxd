(ns console.expo
  "Expo mobile app (`expo/`) + its native gRPC module
  (`expo/modules/rockbox-rpc`).

  App dev:
      (expo/install)      ;; bun install
      (expo/start)        ;; bun run start (Metro / expo-router)
      (expo/typecheck)    ;; bunx tsc --noEmit
      (expo/lint)         ;; bunx expo lint
      (expo/export-web)   ;; bunx expo export --platform web (smoke test)

  Native libs (rebuild after changing crates/expo — Metro doesn't pick up
  native changes):
      (expo/ios)          ;; RockboxExpo.xcframework
      (expo/android)      ;; librockbox_expo.so per ABI (embedded-daemon)
      (expo/prebuild)     ;; bunx expo prebuild
      (expo/run-ios)      ;; bunx expo run:ios
      (expo/run-android)  ;; bunx expo run:android"
  (:require [console.shell :as sh]))

(def ^:private module "expo/modules/rockbox-rpc")

;; ── JS app ───────────────────────────────────────────────────────────

(defn install   [] (sh/bun "expo" "install"))
(defn start     [] (sh/bun "expo" "run" "start"))
(defn typecheck [] (sh/bunx "expo" "tsc" "--noEmit"))
(defn lint      [] (sh/bunx "expo" "expo" "lint"))

(defn export-web
  "Bundle the web target — catches NativeWind transform issues."
  []
  (sh/bunx "expo" "expo" "export" "--platform" "web"))

;; ── native module ────────────────────────────────────────────────────

(defn ios
  "Build the iOS xcframework (expo/modules/rockbox-rpc/scripts/build-ios.sh)."
  []
  (sh/bun module "run" "build:ios"))

(defn android
  "Build the Android cdylib with the full embedded daemon
  (embedded-daemon feature, API 26). Drops librockbox_expo.so per ABI into
  android/src/main/jniLibs. Pass a profile via the PROFILE env var."
  []
  (sh/bash (str module "/scripts/build-android.sh")))

;; ── prebuild + native run ────────────────────────────────────────────

(defn prebuild   [] (sh/bunx "expo" "expo" "prebuild"))
(defn run-ios     [] (sh/bunx "expo" "expo" "run:ios"))
(defn run-android [] (sh/bunx "expo" "expo" "run:android"))
