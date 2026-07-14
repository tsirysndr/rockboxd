/**
 * Minimal HTTP dev server for the Rockbox WASM decode + DSP core.
 *
 * Serves the web/ directory with the COOP/COEP headers that SharedArrayBuffer
 * (and therefore Emscripten pthreads) require in the browser. COEP is
 * `credentialless` rather than `require-corp` so the page stays
 * crossOriginIsolated while still being able to pull cross-origin webfonts
 * (Outfit / Lexend / a monospace) from Google Fonts.
 *
 * Usage:
 *   node scripts/wasm-dev-server.mjs          # port 8080
 *   PORT=3000 node scripts/wasm-dev-server.mjs
 */

import http from 'node:http';
import fs   from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const PORT    = parseInt(process.env.PORT ?? '8090', 10);
const ROOTDIR = path.resolve(fileURLToPath(import.meta.url), '../../web');

const MIME = {
  '.html': 'text/html',
  '.js':   'application/javascript',
  '.mjs':  'application/javascript',
  '.wasm': 'application/wasm',
  '.css':  'text/css',
  '.json': 'application/json',
};

const server = http.createServer((req, res) => {
  let urlPath = req.url.split('?')[0];
  if (urlPath === '/') urlPath = '/index.html';

  const file = path.join(ROOTDIR, urlPath);
  const ext  = path.extname(file);

  if (!file.startsWith(ROOTDIR)) {
    res.writeHead(403); res.end('Forbidden'); return;
  }

  fs.readFile(file, (err, data) => {
    if (err) {
      res.writeHead(err.code === 'ENOENT' ? 404 : 500);
      res.end(err.message);
      return;
    }
    res.writeHead(200, {
      'Content-Type':                MIME[ext] ?? 'application/octet-stream',
      'Cross-Origin-Opener-Policy':  'same-origin',
      'Cross-Origin-Embedder-Policy':'credentialless',
      'Cache-Control':               'no-store',
    });
    res.end(data);
  });
});

server.listen(PORT, () => {
  console.log(`[wasm-dev-server] http://localhost:${PORT}`);
  console.log(`  Serving: ${ROOTDIR}`);
  console.log('  COOP/COEP headers active — SharedArrayBuffer enabled');
});
