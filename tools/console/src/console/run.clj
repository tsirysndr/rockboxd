(ns console.run
  "Run the built rockboxd daemon and common dev variants.

      (run/daemon)          ;; ./zig/zig-out/bin/rockboxd
      (run/debug)           ;; RUST_LOG=debug rockboxd
      (run/debug \"rockbox_airplay=debug,info\")
      (run/pipe)            ;; FIFO stdout -> ffplay (raw s16le 44100 stereo)

  These run in the foreground so you see logs and Ctrl-C stops them. Wrap
  with `console.shell/sh*` if you want the daemon backgrounded in your REPL."
  (:require [console.shell :as sh]))

(def ^:private bin "zig/zig-out/bin/rockboxd")

(defn daemon
  "Run the daemon in the foreground with inherited stdio.
  Extra args are passed through verbatim."
  [& args]
  (sh/sh (into [(sh/in bin)] (map str args))))

(defn debug
  "Run the daemon with `RUST_LOG` set (defaults to `debug`).

      (run/debug)
      (run/debug \"rockbox_airplay=debug,info\")"
  ([] (debug "debug"))
  ([rust-log & args]
   (sh/sh (into [(sh/in bin)] (map str args))
          {:extra-env {"RUST_LOG" rust-log}})))

(defn pipe
  "Pipe the daemon's stdout PCM stream into ffplay — for FIFO/`-` output
  mode. Assumes `audio_output = \"fifo\"` with `fifo_path = \"-\"` in
  settings.toml. Uses a shell so the `|` is honored."
  []
  (sh/sh ["sh" "-c"
          (str (sh/in bin) " | ffplay -f s16le -ar 44100 -ac 2 -")]))
