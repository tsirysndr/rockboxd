import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// No special headers needed — rockbox-wasm's single-threaded build uses no
// SharedArrayBuffer, so no cross-origin isolation (COOP/COEP) is required.
export default defineConfig({
  plugins: [react(), tailwindcss()],
});
