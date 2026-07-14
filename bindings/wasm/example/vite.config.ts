import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath } from "node:url";

// This example lives inside the package, so it resolves `rockbox-wasm` to the
// local build (../dist) via an alias rather than an npm install. A real
// consumer would `bun add rockbox-wasm` and drop this alias.
//
// No special headers needed — the single-threaded build uses no
// SharedArrayBuffer, so no cross-origin isolation (COOP/COEP) is required.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "rockbox-wasm": fileURLToPath(new URL("../dist/rockbox.js", import.meta.url)),
    },
  },
  server: { fs: { allow: [".."] } }, // allow importing from ../dist
});
