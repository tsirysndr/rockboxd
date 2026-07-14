/**
 * Rockbox decoder Worker.
 *
 * Owns the WebAssembly module (rockbox-core.wasm: rockbox-codecs + rockbox-dsp
 * + rockbox-metadata) and drives the whole "player" that the C firmware used
 * to provide — queue, transport, per-track decode, DSP, resampling — but in
 * plain JavaScript. Decoded + DSP-processed S16LE PCM is written into a shared
 * ring buffer that the AudioWorklet plays.
 *
 * Why a Worker: rockbox-codecs' Decoder runs the codec on its own pthread and
 * blocks on a Condvar (Atomics.wait) for each chunk. Atomics.wait throws on
 * the main browser thread, so all decode work has to happen off it.
 *
 * The module is built with -pthread, so WASM memory is a SharedArrayBuffer
 * (the page needs COOP/COEP headers — see scripts/wasm-dev-server.mjs).
 */

/* global RockboxModule */

// Shared control indices — keep in sync with rockbox-audio-worklet.js.
const CTRL_WRITE  = 0;
const CTRL_READ   = 1;
const CTRL_PAUSED = 2;
const CTRL_PLAYED = 3;
const CTRL_GEN    = 4;

let Module     = null;
let ctrl       = null;  // Int32Array over controlSab
let ring       = null;  // Int16Array over audioSab
let ringFrames = 0;
let sampleRate = 44100; // AudioContext output rate; DSP resamples to it

let dsp    = null;      // *Dsp (created once, reused across tracks)
let dec    = null;      // *RbDecoder for the current track
let decPath = null;     // MEMFS path of the current file

// scratch out-param cells (allocated once)
let outLenPtr = 0, outRatePtr = 0, procLenPtr = 0, pathPtr = 0, pathCap = 0;

// ── Player state ──────────────────────────────────────────────────────────
let queue      = [];    // array of URL strings
let index      = -1;    // current queue index
let playing    = false; // engine should be producing audio
let userPaused = false;
let repeat     = 0;     // 0 off, 1 one, 2 all
let shuffle    = false;
let trackDecoded = false; // decoder hit EOT; draining the ring before advancing
let curInRate  = 0;     // last input rate handed to the DSP
let curMeta    = null;  // metadata JSON of the current track
let loadToken  = 0;     // cancels stale async track loads
let pumpTimer  = null;

// ── Boot ────────────────────────────────────────────────────────────────────
// Resolve the core module URL against THIS worker so emscripten spawns its
// codec pthreads from rockbox-core.js (not from this decoder worker script).
const CORE_URL = new URL('rockbox-core.js', self.location.href).href;
importScripts(CORE_URL);
RockboxModule({ mainScriptUrlOrBlob: CORE_URL }).then((m) => {
  Module = m;
  outLenPtr  = m._malloc(4);
  outRatePtr = m._malloc(4);
  procLenPtr = m._malloc(4);
  postMessage({ type: 'ready' });
});

onmessage = (e) => {
  const msg = e.data;
  switch (msg.cmd) {
    case 'init':       return onInit(msg);
    case 'setQueue':   return setQueue(msg.urls, msg.autoplay);
    case 'enqueue':    return enqueue(msg.url);
    case 'clearQueue': return clearQueue();
    case 'play':       return play();
    case 'pause':      return pause();
    case 'toggle':     return userPaused || !playing ? play() : pause();
    case 'stop':       return stop();
    case 'next':       return skip(+1);
    case 'prev':       return skip(-1);
    case 'skipTo':     return startTrack(msg.index, 0, true);
    case 'seek':       return seek(msg.ms);
    case 'shuffle':    shuffle = !!msg.enabled; return emitStatus();
    case 'repeat':     repeat  = msg.mode | 0;  return emitStatus();
    case 'dsp':        return applyDsp(msg.name, msg.args);
  }
};

function onInit(msg) {
  ctrl       = new Int32Array(msg.controlSab);
  ring       = new Int16Array(msg.audioSab);
  ringFrames = msg.ringFrames;
  sampleRate = msg.sampleRate;
  // One DSP for the whole session; output at the AudioContext rate so every
  // track is resampled to it.
  dsp = Module._rb_dsp_new(sampleRate);
  setInterval(emitProgress, 200);
  emitStatus();
}

// ── Ring helpers ──────────────────────────────────────────────────────────
function occupied() {
  const w = Atomics.load(ctrl, CTRL_WRITE);
  const r = Atomics.load(ctrl, CTRL_READ);
  return (w - r + ringFrames) % ringFrames;
}
function freeFrames() { return ringFrames - occupied() - 1; }

function setPaused(v) { Atomics.store(ctrl, CTRL_PAUSED, v ? 1 : 0); }

/** Drop all buffered audio and reset the play cursor (worklet stays quiet). */
function flushRing(playedFrames = 0) {
  setPaused(true);
  Atomics.store(ctrl, CTRL_WRITE,  0);
  Atomics.store(ctrl, CTRL_READ,   0);
  Atomics.store(ctrl, CTRL_PLAYED, playedFrames);
  Atomics.add(ctrl, CTRL_GEN, 1);
}

/** Copy `frames` interleaved-stereo frames from HEAP16[srcIdx…] into the ring. */
function writeFrames(srcIdx, frames) {
  const heap = Module.HEAP16;
  let wi = Atomics.load(ctrl, CTRL_WRITE);
  for (let f = 0; f < frames; f++) {
    const dst = ((wi + f) % ringFrames) * 2;
    const src = srcIdx + f * 2;
    ring[dst]     = heap[src];
    ring[dst + 1] = heap[src + 1];
  }
  Atomics.store(ctrl, CTRL_WRITE, (wi + frames) % ringFrames);
}

// ── Decode pump ─────────────────────────────────────────────────────────────
function schedulePump(ms) {
  clearTimeout(pumpTimer);
  pumpTimer = setTimeout(pump, ms);
}

function pump() {
  if (!playing || !dec) return;

  if (trackDecoded) {
    // Track fully decoded: let the ring drain, then advance for a clean cut.
    if (occupied() === 0) advanceAfterEnd();
    else schedulePump(40);
    return;
  }

  // Keep the ring at most ~half full (a couple of seconds of look-ahead).
  if (freeFrames() < (ringFrames >> 1)) { schedulePump(15); return; }

  const pcmPtr = Module._rb_decoder_next_chunk(dec, outLenPtr, outRatePtr);
  if (!pcmPtr) { trackDecoded = true; schedulePump(20); return; } // end of track

  const len  = Module.HEAPU32[outLenPtr  >> 2];
  const rate = Module.HEAPU32[outRatePtr >> 2];
  if (rate && rate !== curInRate) {
    Module._rb_dsp_set_input_frequency(dsp, rate);
    curInRate = rate;
  }

  const procPtr = Module._rb_dsp_process(dsp, pcmPtr, len, procLenPtr);
  const procLen = Module.HEAPU32[procLenPtr >> 2];
  if (procPtr) {
    if (procLen >= 2) writeFrames(procPtr >> 1, procLen >> 1);
    Module._rb_buffer_free(procPtr, procLen);
  }
  Module._rb_buffer_free(pcmPtr, len);

  schedulePump(0);
}

// ── Track lifecycle ─────────────────────────────────────────────────────────
function closeDecoder() {
  if (dec) { Module._rb_decoder_free(dec); dec = null; }
  if (decPath) { try { Module.FS.unlink(decPath); } catch (_) {} decPath = null; }
  trackDecoded = false;
  curInRate = 0;
}

function extOf(url) {
  const m = /\.([A-Za-z0-9]{1,5})(?:[?#]|$)/.exec(url);
  return m ? m[1].toLowerCase() : 'bin';
}

/** Fetch, open and (optionally) start playing queue entry `i` from `seekMs`. */
async function startTrack(i, seekMs, autoplay) {
  if (i < 0 || i >= queue.length) { stop(); return; }
  const token = ++loadToken;
  const url = queue[i];

  clearTimeout(pumpTimer);
  closeDecoder();
  index = i;
  flushRing(Math.round((seekMs || 0) * sampleRate / 1000));

  let bytes;
  try {
    const resp = await fetch(url);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    bytes = new Uint8Array(await resp.arrayBuffer());
  } catch (err) {
    postMessage({ type: 'error', message: `fetch failed: ${url} (${err})`, index: i });
    if (token === loadToken) skipAfterError(i);
    return;
  }
  if (token !== loadToken) return; // a newer load superseded this one

  const path = `/track_${token}.${extOf(url)}`;
  Module.FS.writeFile(path, bytes);
  const p = allocPath(path);
  dec = Module._rb_decoder_open(p);
  if (!dec) {
    try { Module.FS.unlink(path); } catch (_) {}
    postMessage({ type: 'error', message: `cannot decode: ${url}`, index: i });
    skipAfterError(i);
    return;
  }
  decPath = path;

  curMeta = readMetadata();
  applyTrackReplaygain(curMeta);
  Module._rb_dsp_flush(dsp);

  if (seekMs) Module._rb_decoder_seek_ms(dec, seekMs);

  postMessage({ type: 'track', index: i, url, metadata: curMeta });

  if (autoplay || playing) {
    playing    = true;
    userPaused = false;
    setPaused(false);
    schedulePump(0);
  }
  emitStatus();
}

function skipAfterError(i) {
  // Advance past an unplayable track so a bad URL doesn't wedge the queue.
  const next = i + 1;
  if (next < queue.length) startTrack(next, 0, true);
  else stop();
}

function readMetadata() {
  const ptr = Module._rb_decoder_metadata_json(dec);
  if (!ptr) return null;
  const json = Module.UTF8ToString(ptr);
  Module._rb_string_free(ptr);
  try { return JSON.parse(json); } catch (_) { return null; }
}

function applyTrackReplaygain(meta) {
  const rg = meta && meta.replaygain;
  if (!rg) return;
  // dB gains + linear peaks from the tag parser; NaN = "tag absent". These
  // only take effect once ReplayGain mode is enabled from the UI.
  const v = (x) => (x == null ? NaN : x);
  Module._rb_dsp_set_replaygain_gains(
    dsp,
    v(rg.track_gain_db), v(rg.album_gain_db),
    v(rg.track_peak),    v(rg.album_peak),
  );
}

function advanceAfterEnd() {
  if (repeat === 1) { startTrack(index, 0, true); return; } // repeat one
  const next = index + 1;
  if (next < queue.length) startTrack(next, 0, true);
  else if (repeat === 2) startTrack(0, 0, true);           // repeat all → wrap
  else stop();
}

// ── Transport commands ──────────────────────────────────────────────────────
function play() {
  if (queue.length === 0) return;
  if (!dec) { startTrack(index >= 0 ? index : 0, 0, true); return; }
  playing = true; userPaused = false; setPaused(false);
  schedulePump(0);
  emitStatus();
}
function pause() {
  userPaused = true; setPaused(true);
  emitStatus();
}
function stop() {
  playing = false; userPaused = false;
  clearTimeout(pumpTimer);
  closeDecoder();
  flushRing(0);
  setPaused(false);
  curMeta = null;
  emitStatus();
}
function skip(dir) {
  if (queue.length === 0) return;
  let next;
  if (shuffle && queue.length > 1) {
    do { next = Math.floor(Math.random() * queue.length); } while (next === index);
  } else {
    next = index + dir;
    if (next >= queue.length) next = repeat === 2 ? 0 : queue.length - 1;
    if (next < 0) next = 0;
  }
  startTrack(next, 0, true);
}
function seek(ms) {
  if (!dec) return;
  startTrack(index, ms, playing);
}

function setQueue(urls, autoplay) {
  queue = Array.isArray(urls) ? urls.slice() : [];
  index = -1;
  emitQueue();
  if (queue.length && autoplay) startTrack(0, 0, true);
  else { stop(); }
}
function enqueue(url) {
  queue.push(url);
  emitQueue();
  if (playing && !dec) startTrack(index >= 0 ? index : 0, 0, true);
}
function clearQueue() { queue = []; index = -1; stop(); emitQueue(); }

// ── DSP passthrough ─────────────────────────────────────────────────────────
// Main thread sends {cmd:'dsp', name:'set_eq_band', args:[...]}; we prepend the
// DSP handle and call the matching rockbox-ffi export.
function applyDsp(name, args) {
  if (!dsp) return;
  const fn = Module['_rb_dsp_' + name];
  if (typeof fn !== 'function') return;
  fn(dsp, ...args);
}

// ── Events to the main thread ───────────────────────────────────────────────
function stateName() {
  if (!playing && !dec) return 'stopped';
  return userPaused ? 'paused' : 'playing';
}
function emitStatus() {
  postMessage({ type: 'status', state: stateName(), index, queue_len: queue.length,
                shuffle, repeat });
}
function emitQueue() {
  postMessage({ type: 'queue', urls: queue, index });
}
function emitProgress() {
  if (!ctrl) return;
  const played = Atomics.load(ctrl, CTRL_PLAYED);
  postMessage({
    type: 'progress',
    state: stateName(),
    index,
    elapsed_ms:  Math.round(played * 1000 / sampleRate),
    duration_ms: curMeta ? (curMeta.duration_ms | 0) : 0,
    metadata: curMeta,
  });
}

// ── misc ────────────────────────────────────────────────────────────────────
function allocPath(str) {
  const need = Module.lengthBytesUTF8(str) + 1;
  if (need > pathCap) {
    if (pathPtr) Module._free(pathPtr);
    pathPtr = Module._malloc(need);
    pathCap = need;
  }
  Module.stringToUTF8(str, pathPtr, pathCap);
  return pathPtr;
}
