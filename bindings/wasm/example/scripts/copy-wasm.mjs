// Copy the built rockbox-wasm dist/ into public/rockbox so Vite serves the
// core + worker + worklet at /rockbox/* (used via `baseUrl: "/rockbox"`).
//
// Runs automatically before `dev` and `build` (see package.json). The package
// must be built first: `bash ../scripts/build.sh` from bindings/wasm.

import { cp, mkdir, access } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));

// This example lives inside the package, so its built dist/ is two levels up.
const distDir = resolve(here, "../../dist");
const dest = resolve(here, "../public/rockbox");

try {
  await access(resolve(distDir, "rockbox-core.js"));
} catch {
  console.error(
    `\n✗ rockbox-wasm isn't built yet (${distDir}/rockbox-core.js missing).\n` +
      `  Build it first:  (cd .. && bash scripts/build.sh)\n`,
  );
  process.exit(1);
}

await mkdir(dest, { recursive: true });
await cp(distDir, dest, { recursive: true });
console.log(`✔ copied rockbox-wasm dist → public/rockbox`);
