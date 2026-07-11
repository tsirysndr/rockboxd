# Rockbox Console

A REPL-driven, single entry point for **every build / dev / ops command** in
the rockboxd monorepo. Instead of remembering the exact `make`, `cargo`,
`zig`, `bun`, and `bash scripts/*.sh` incantations scattered across
`CLAUDE.md`, you get one Clojure namespace tree you can drive from a REPL or
one-shot [babashka](https://babashka.org) tasks.

```
tools/console/
├── deps.edn                 Clojure deps + :dev (nREPL) / :rebel aliases
├── bb.edn                   babashka task surface (bb <task>)
├── .mise.toml               pinned java / clojure / babashka versions
├── dev/user.clj             auto-loaded REPL helpers (aliases + .env load)
└── src/console/
    ├── path.clj             repo-root discovery (Cargo.toml + zig/build.zig)
    ├── shell.clj            sh / sh! / sh* + make/cargo/zig/bun/bash shims
    ├── env.clj              dotenv loader; injected into every subprocess
    ├── core.clj             command registry, (help)/(ls)/dispatch
    ├── build.clj            firmware → crates → zig → rockboxd pipeline
    ├── make.clj             raw make + tools/configure escape hatches
    ├── run.clj              run the daemon (plain / RUST_LOG / ffplay pipe)
    ├── dev.clj              cargo fmt / clippy / test / check
    ├── wasm.clj             WASM build + dev server
    ├── expo.clj             mobile app + native gRPC module builds
    ├── bindings.clj         rockbox-ffi + multi-language publish
    ├── verify.clj           nm / ar / stale-binary sanity checks
    └── scripts.clj          auto-discovers & runs every scripts/*.sh
```

## Setup

Versions are pinned in `.mise.toml` (Java 21 / Clojure / babashka). With
[mise](https://mise.jdx.dev):

```sh
cd tools/console
mise install          # installs java, clojure, babashka
```

Or install `clojure` + `bb` however you like.

## Two ways to run

### 1. One-shot tasks (`bb <task>`)

```sh
cd tools/console

bb help                 # pretty command tour
bb tasks                # raw babashka task list

bb build:all            # firmware → crates → zig → rockboxd
bb run                  # ./zig/zig-out/bin/rockboxd
bb run:debug            # RUST_LOG=debug rockboxd
bb wasm:build           # bash scripts/build-wasm.sh
bb verify:stale         # binary vs .a timestamps
bb scripts              # list every scripts/*.sh
bb script build-armhf   # run one of them
```

### 2. REPL (recommended for anything iterative)

```sh
cd tools/console
clj -M:rebel            # rich terminal REPL
# or: clj -M:dev        # nREPL for your editor (CIDER / Calva)
```

`dev/user.clj` loads on start, drops every namespace into scope under short
aliases, and auto-loads `<repo>/.env`:

```clojure
user=> (help)                 ; command tour
user=> (build/all)            ; full rebuild
user=> (run/daemon)           ; launch rockboxd
user=> (run/debug "rockbox_airplay=debug,info")
user=> (verify/stale)         ; check the stale-binary pitfall
user=> (scripts/build-wasm)   ; every scripts/*.sh is a fn
user=> (env/set! "RUST_LOG" "debug")  ; propagates to later subprocesses
```

## Command groups

| Group      | Namespace          | What it wraps                                             |
| ---------- | ------------------ | -------------------------------------------------------- |
| `build`    | `console.build`    | The C→Rust→Zig pipeline into `rockboxd` / `librockboxd.a`|
| `make`     | `console.make`     | Raw `make` in a build dir + `tools/configure`            |
| `run`      | `console.run`      | Run the daemon (plain / `RUST_LOG` / ffplay pipe)        |
| `dev`      | `console.dev`      | `cargo fmt` / `clippy` / `test` / `check`                |
| `wasm`     | `console.wasm`     | `scripts/build-wasm.sh` + the COOP/COEP dev server       |
| `expo`     | `console.expo`     | Mobile app dev + iOS/Android native gRPC module builds   |
| `bindings` | `console.bindings` | `rockbox-ffi` + npm/python/ruby publish scripts          |
| `verify`   | `console.verify`   | `nm` / `ar` / timestamp sanity checks from `CLAUDE.md`   |
| `scripts`  | `console.scripts`  | Auto-discovered `scripts/*.sh` (no hand-maintained list) |
| `env`      | `console.env`      | dotenv loader injected into every spawned subprocess     |

## How it works

- **Repo-root aware.** `console.path/repo-root` walks up from the cwd looking
  for the workspace `Cargo.toml` next to `zig/build.zig`, so commands run from
  the right directory no matter where you invoke them.
- **Env injection.** Whatever you load with `(env/load!)` / `(env/set! …)` is
  merged into every subprocess's environment via `console.shell` — handy for
  `RUST_LOG`, `ROCKBOX_LIBRARY`, `ANDROID_NDK_HOME`, etc.
- **Scripts are free.** Drop a new `scripts/foo.sh` in the repo and it shows up
  in `(scripts/ls)` / `bb scripts` and becomes callable as `(scripts/foo)` —
  no edit here required.

## Adding a command

1. Add a function to the relevant `console.<group>` namespace (or create a new
   one). Use the `console.shell` helpers (`sh`, `make`, `cargo`, `zig`, `bun`,
   `bash`) so it inherits the repo-root dir and loaded env.
2. Add a one-liner to the `registry` in `console.core` so `(help)`/`(ls)` list
   it.
3. (Optional) Add a matching `bb.edn` task for shell-only use.

Pure `scripts/*.sh` need none of this — they are picked up automatically.
