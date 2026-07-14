# rockbox-wasm — React + TypeScript + Vite example

A minimal player UI (now-playing, transport, seek, volume, 10-band EQ, live
radio) built with React + Vite on top of [`rockbox-wasm`](../), installed as a
relative `file:..` dependency.

## Run it

```sh
# 1. Build the package once (needs Emscripten + the wasm Rust target):
(cd .. && bash scripts/build.sh)

# 2. Install + start the dev server:
npm install
npm run dev            # → http://localhost:5173
```

`npm install` links the parent package via `"rockbox-wasm": "file:.."`. A
`copy-wasm` step (run automatically before `dev`/`build`) copies the built
`dist/` into `public/rockbox`, and the app points at it with
`new RockboxPlayer({ baseUrl: "/rockbox" })`.

No COOP/COEP headers are required — the build is single-threaded.

## Files

- `src/App.tsx` — the UI
- `src/useRockbox.ts` — a hook that owns a `RockboxPlayer` and mirrors its
  events into React state
- `scripts/copy-wasm.mjs` — copies the package's `dist/` into `public/rockbox`
- `vite.config.ts` — React + Tailwind v4 plugins
