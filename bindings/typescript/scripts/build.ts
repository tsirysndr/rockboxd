#!/usr/bin/env bun
// Build script (run with Bun): produces the publishable `dist/` for npm.
//
//   bun run build      # -> dist/*.js  + dist/*.d.ts
//
// Bundles the four runtime entry points (index / bun / deno / node) with
// Bun's bundler, keeping the FFI-specific imports external, then emits type
// declarations with tsc.

import { rm } from "node:fs/promises";
import { $ } from "bun";

const ROOT = new URL("..", import.meta.url).pathname;
const OUT = `${ROOT}dist`;

console.log("• cleaning dist/");
await rm(OUT, { recursive: true, force: true });

console.log("• bundling JS with Bun.build");
const result = await Bun.build({
  entrypoints: [
    `${ROOT}src/index.ts`,
    `${ROOT}src/bun.ts`,
    `${ROOT}src/deno.ts`,
    `${ROOT}src/node.ts`,
  ],
  outdir: OUT,
  target: "node", // node builtins stay external; works for Bun too
  format: "esm",
  splitting: true, // share the api/ffi/enums chunks between entries
  // `bun:ffi` (Bun-only) and `koffi` (Node-only) must not be bundled.
  external: ["bun:ffi", "koffi"],
  naming: "[dir]/[name].[ext]",
  sourcemap: "external",
});

if (!result.success) {
  console.error("Bun.build failed:");
  for (const log of result.logs) console.error(log);
  process.exit(1);
}
for (const o of result.outputs) {
  console.log(`  ${o.path.replace(ROOT, "")}`);
}

console.log("• emitting type declarations with tsc");
await $`bunx tsc -p tsconfig.build.json`.cwd(ROOT);

console.log("✔ build complete -> dist/");
