# rockbox-ffi — Clojure examples

Small, runnable programs showing each surface of the binding. Build the shared
library first (`cargo build --release -p rockbox-ffi` from the repo root, or set
`ROCKBOX_FFI_LIB`), then run any example with the `:examples` alias — it adds
`--enable-native-access=ALL-UNNAMED` for the Foreign Function & Memory API:

```sh
mise exec -- clojure -M:examples examples/metadata.clj /path/to/song.flac
mise exec -- clojure -M:examples examples/dsp_replaygain.clj
mise exec -- clojure -M:examples examples/dsp_eq.clj
mise exec -- clojure -M:examples examples/player.clj /path/to/song.flac
```

| File                                       | Shows                                                              |
| ------------------------------------------ | ----------------------------------------------------------------- |
| [`metadata.clj`](metadata.clj)             | `metadata/read` (tags, duration, ReplayGain) and `metadata/probe` |
| [`dsp_replaygain.clj`](dsp_replaygain.clj) | A −6.02 dB track gain halving a 1 kHz sine through the DSP         |
| [`dsp_eq.clj`](dsp_eq.clj)                 | Enabling the 10-band EQ and setting shelf bands                   |
| [`player.clj`](player.clj)                 | Queue + transport + live `player/status` on the output device     |

## The three surfaces

```clojure
(require '[rockbox.ffi.metadata :as metadata]
         '[rockbox.ffi.dsp :as dsp]
         '[rockbox.ffi.player :as player])

;; metadata — parse tags without an output device
(metadata/read "/music/song.flac")      ; => {:title … :codec "FLAC" …}
(metadata/probe "song.flac")            ; => "FLAC"

;; dsp — process interleaved stereo int16 (a short-array); handle is a singleton
(dsp/with-dsp [d 44100]
  (dsp/eq-enable d true)
  (dsp/set-eq-band d 0 100 0.7 3.0)
  (dsp/process d samples))              ; => short-array

;; player — queue + transport; owns a live output device
(player/with-player [p {:volume 0.8}]
  (player/set-queue p ["/music/a.flac"])
  (player/play p)
  (:state (player/status p)))
```

`with-dsp` / `with-player` free the native handle on exit; every `char*` /
`int16*` the ABI returns is freed inside the binding. Enum arguments accept a
keyword or the raw int — note the two ReplayGain encodings:
`dsp-replaygain-mode` (`:track` 0, `:album` 1, `:shuffle` 2, `:off` 3) for the
DSP, `replaygain-mode` (`:off` 0, `:track` 1, `:album` 2) for the player.
