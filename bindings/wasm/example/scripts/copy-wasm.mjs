// Copy the built rockbox-wasm dist/ into public/rockbox so Vite serves the
// core + worker + worklet at /rockbox/* (used via `baseUrl: "/rockbox"`).
//
// Runs automatically before `dev` and `build` (see package.json). The package
// must be built first: `bash ../scripts/build.sh` from bindings/wasm.

import { cp, mkdir, access } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const here = dirname(fileURLToPath(import.meta.url));

// Resolve the installed package's entry → its dist/ directory (works whether
// the file: dependency is symlinked or copied into node_modules).
const distDir = dirname(require.resolve("rockbox-wasm"));
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
