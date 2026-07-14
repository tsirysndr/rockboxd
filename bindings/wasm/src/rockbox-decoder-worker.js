/**
 * Rockbox decoder Worker (single-threaded build).
 *
 * Owns the WebAssembly module (rockbox-core: rockbox-codecs + rockbox-dsp +
 * rockbox-metadata) and drives the whole player in plain JS: queue, transport,
 * decode, DSP, resampling. Decoding is fully synchronous (rb_decode_file /
 * rb_decode_packet) — no wasm threads — so the module needs no SharedArrayBuffer
 * and the page needs no COOP/COEP headers.
 *
 * Decoded, DSP-processed S16 PCM is posted to the AudioWorklet over a
 * MessagePort; the worklet queues and plays it and reports back how much it has
 * consumed / has queued so we can pace decoding and report elapsed time.
 *
 *   - Finite file (has Content-Length): fetched whole, tags via rb_meta_read_json,
 *     decoded via rb_decode_file → full metadata, duration, seeking.
 *   - Live stream (no Content-Length): read the network in ~32 KB segments,
 *     decode each with rb_decode_packet, stream the PCM out. ICY StreamTitle is
 *     demuxed for now-playing metadata.
 */

/* global RockboxModule */

const LIVE_SEGMENT = 32 * 1024;        // encoded bytes per live-radio decode
const LIVE_PREBUFFER_SEC = 2.5;        // buffer before starting live playback
const CHUNK = 8192;                    // i16 samples per PCM post (4096 frames)

let Module     = null;
let sampleRate = 44100;                // AudioContext output rate; DSP resamples to it
let dsp        = null;                  // *Dsp (created once, reused)

let pcmPort    = null;                  // MessagePort → AudioWorklet
let wlConsumed = 0;                     // frames the worklet has played since last flush
let wlQueued   = 0;                     // frames buffered in the worklet
let seekBaseMs = 0;                     // elapsed offset (survives a worklet flush)

// scratch out-param cells + heap buffers (allocated once)
let outLenPtr = 0, outRatePtr = 0, procLenPtr = 0, pathPtr = 0, pathCap = 0, pktPtr = 0, pktCap = 0;

// ── Player state ──────────────────────────────────────────────────────────
let queue      = [];
let index      = -1;
let playing    = false;
let userPaused = false;
let repeat     = 0;    // 0 off, 1 one, 2 all
let shuffle    = false;
let curMeta    = null;
let live       = false;
let loadToken  = 0;    // bumped on every track change / stop
let curInRate  = 0;

// Finite-track decoded PCM, kept alive so we can seek within it.
let rawPtr = 0, rawLen = 0, rawRate = 0, finitePos = 0;

// ── Boot ────────────────────────────────────────────────────────────────────
const CORE_URL = new URLSearchParams(self.location.search).get('core')
  || new URL('rockbox-core.js', self.location.href).href;
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
    case 'pcmport':    return onPcmPort(msg.port);
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
  sampleRate = msg.sampleRate;
  dsp = Module._rb_dsp_new(sampleRate);
  setInterval(emitProgress, 200);
  emitStatus();
}

function onPcmPort(port) {
  pcmPort = port;
  pcmPort.onmessage = (e) => {
    const m = e.data;
    if (m.type === 'level') { wlConsumed = m.consumed; wlQueued = m.queued; }
  };
}

// ── Worklet transport ─────────────────────────────────────────────────────
function flushWorklet() { pcmPort && pcmPort.postMessage({ type: 'flush' }); wlConsumed = 0; wlQueued = 0; }
function setWorkletPaused(v) { pcmPort && pcmPort.postMessage({ type: 'paused', value: v }); }

/** Copy `procLen` i16 samples at HEAP16[procPtr…] out of the heap, free the
 *  heap buffer, and transfer the copy to the worklet. */
function postPcm(procPtr, procLen) {
  const start = procPtr >> 1;
  const copy = Module.HEAP16.slice(start, start + procLen); // detached copy
  Module._rb_buffer_free(procPtr, procLen);
  pcmPort.postMessage({ pcm: copy.buffer }, [copy.buffer]);
}

const highWater = () => 3 * sampleRate; // keep ≤ ~3 s buffered in the worklet
function sleep(ms) { return new Promise((r) => setTimeout(r, ms)); }

/**
 * Stream a raw interleaved-S16 PCM buffer in the wasm heap (`ptr`/`len` i16 at
 * `rate` Hz) to the worklet: DSP + resample each chunk and post it, pausing for
 * backpressure. `getPos`/`setPos` track the read cursor (in i16 samples) so a
 * finite track can seek by moving it. Returns true when fully streamed, false
 * if the load was superseded.
 */
async function streamRaw(ptr, len, rate, getPos, setPos, token, onProgress) {
  if (rate && rate !== curInRate) { Module._rb_dsp_set_input_frequency(dsp, rate); curInRate = rate; }
  for (;;) {
    if (token !== loadToken) return false;
    if (userPaused) { await sleep(30); continue; }
    const pos = getPos();
    if (pos >= len) return true;
    if (wlQueued > highWater()) { await sleep(20); continue; }
    const chunkLen = Math.min(CHUNK, len - pos);
    const procPtr = Module._rb_dsp_process(dsp, ptr + pos * 2, chunkLen, procLenPtr);
    const procLen = Module.HEAPU32[procLenPtr >> 2];
    setPos(pos + chunkLen);
    if (procPtr && procLen >= 2) postPcm(procPtr, procLen);
    else if (procPtr) Module._rb_buffer_free(procPtr, procLen);
    if (onProgress) onProgress();
  }
}

// ── Track lifecycle ─────────────────────────────────────────────────────────
function freeRaw() {
  if (rawPtr) { Module._rb_buffer_free(rawPtr, rawLen); rawPtr = 0; rawLen = 0; }
  finitePos = 0;
}

function extOf(url) {
  const m = /\.([A-Za-z0-9]{1,5})(?:[?#]|$)/.exec(url);
  return m ? m[1].toLowerCase() : 'bin';
}
function formatExt(contentType, url) {
  const ct = (contentType || '').toLowerCase();
  if (ct.includes('mpeg') || ct.includes('mp3')) return 'mp3';
  if (ct.includes('aac')) return 'aac';
  if (ct.includes('ogg') || ct.includes('opus') || ct.includes('vorbis')) return 'ogg';
  if (ct.includes('flac')) return 'flac';
  if (ct.includes('wav')) return 'wav';
  const e = extOf(url);
  return e === 'bin' ? 'mp3' : e;
}

async function startTrack(i, seekMs, autoplay) {
  if (i < 0 || i >= queue.length) { stop(); return; }
  const token = ++loadToken;
  const url = queue[i];

  freeRaw();
  live = false;
  index = i;
  curInRate = 0;
  seekBaseMs = seekMs || 0;
  flushWorklet();
  Module._rb_dsp_flush(dsp);

  let resp;
  try {
    resp = await fetch(url);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
  } catch (err) {
    postMessage({ type: 'error', message: `fetch failed: ${url} (${err})`, index: i });
    if (token === loadToken) skipAfterError(i);
    return;
  }
  if (token !== loadToken) return;

  const hasLength = resp.headers.get('content-length') != null;
  const icy = resp.headers.get('icy-metaint') != null || resp.headers.get('icy-name') != null;
  if (icy || !hasLength) return playLiveStream(resp, url, i, token, autoplay);
  return playFinite(resp, url, i, token, seekMs, autoplay);
}

/** Finite file: buffer whole → tags + full decode → seekable playback. */
async function playFinite(resp, url, i, token, seekMs, autoplay) {
  let bytes;
  try { bytes = new Uint8Array(await resp.arrayBuffer()); }
  catch (err) {
    postMessage({ type: 'error', message: `fetch failed: ${url} (${err})`, index: i });
    if (token === loadToken) skipAfterError(i);
    return;
  }
  if (token !== loadToken) return;

  const path = `/track.${extOf(url)}`;
  Module.FS.writeFile(path, bytes);
  const p = allocPath(path);
  curMeta = readMetaJson(p);
  applyTrackReplaygain(curMeta);
  rawPtr = Module._rb_decode_file(p, outLenPtr, outRatePtr);
  rawLen = Module.HEAPU32[outLenPtr >> 2];
  rawRate = Module.HEAPU32[outRatePtr >> 2] || (curMeta && curMeta.sample_rate) || sampleRate;
  try { Module.FS.unlink(path); } catch (_) {}
  if (!rawPtr || rawLen < 2) {
    postMessage({ type: 'error', message: `cannot decode: ${url}`, index: i });
    skipAfterError(i);
    return;
  }
  finitePos = Math.min(rawLen, Math.floor((seekMs || 0) * rawRate / 1000) * 2);

  postMessage({ type: 'track', index: i, url, live: false, metadata: curMeta });
  if (autoplay || playing) { playing = true; userPaused = false; setWorkletPaused(false); }
  emitStatus();

  const done = await streamRaw(rawPtr, rawLen, rawRate, () => finitePos, (v) => (finitePos = v), token);
  if (done && token === loadToken) { freeRaw(); advanceAfterEnd(); }
}

/** Live stream: read the network, decode ~32 KB segments, stream the PCM. */
async function playLiveStream(resp, url, i, token, autoplay) {
  live = true;
  playing = true; userPaused = false;
  const ext = formatExt(resp.headers.get('content-type'), url);

  let metaint = 0;
  let station = resp.headers.get('icy-name') || '';
  let icyBr = parseInt(resp.headers.get('icy-br') || '0', 10);
  try {
    const r = await fetch(url, { headers: { 'Icy-MetaData': '1' } });
    if (token !== loadToken) { cancelBody(r); cancelBody(resp); return; }
    const mi = r.ok ? parseInt(r.headers.get('icy-metaint') || '0', 10) : 0;
    if (mi > 0) {
      cancelBody(resp); resp = r; metaint = mi;
      station = r.headers.get('icy-name') || station;
      icyBr = parseInt(r.headers.get('icy-br') || '0', 10) || icyBr;
    } else cancelBody(r);
  } catch (_) { /* CORS blocked ICY — plain audio only */ }
  if (token !== loadToken) { cancelBody(resp); return; }

  curMeta = { codec: ext, duration_ms: 0 };
  if (station) curMeta.station = station;
  if (icyBr) curMeta.bitrate = icyBr;
  postMessage({ type: 'track', index: i, url, live: true, metadata: curMeta });
  emitStatus();

  const demux = metaint > 0 ? new IcyDemux(metaint, onIcyTitle) : null;
  const reader = resp.body.getReader();
  const prebuf = Math.round(LIVE_PREBUFFER_SEC * sampleRate);
  let pending = new Uint8Array(0);
  let gotMeta = false;
  let started = false;

  const maybeStart = () => {
    if (!started && !userPaused && wlQueued >= prebuf) { started = true; setWorkletPaused(false); }
  };

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (token !== loadToken) { await reader.cancel().catch(() => {}); return; }
      if (value && value.length) {
        const audio = demux ? demux.push(value) : value;
        if (audio.length) pending = concatBytes(pending, audio);
      }
      while (token === loadToken && pending.length >= LIVE_SEGMENT) {
        const seg = pending.slice(0, LIVE_SEGMENT);
        pending = pending.slice(LIVE_SEGMENT);
        await decodeSegment(seg, ext, token, !gotMeta, maybeStart);
        gotMeta = true;
        if (token !== loadToken) return;
      }
      if (done) break;
    }
    if (token === loadToken && pending.length) await decodeSegment(pending, ext, token, !gotMeta, maybeStart);
    if (token === loadToken) { if (!started && !userPaused) setWorkletPaused(false); advanceAfterEnd(); }
  } catch (err) {
    if (token === loadToken) { postMessage({ type: 'error', message: `stream error: ${url} (${err})`, index: i }); advanceAfterEnd(); }
  }
}

/** Decode one self-contained encoded packet in memory and stream its PCM. */
async function decodeSegment(bytes, ext, token, readMeta, maybeStart) {
  const dataPtr = copyPacket(bytes);
  const pcmPtr = Module._rb_decode_packet(dataPtr, bytes.length, allocPath(ext), outLenPtr, outRatePtr);
  const len = Module.HEAPU32[outLenPtr >> 2];
  const rate = Module.HEAPU32[outRatePtr >> 2];
  if (!pcmPtr || len < 2) { if (pcmPtr) Module._rb_buffer_free(pcmPtr, len); return; }
  if (readMeta) updateLiveMeta({ codec: ext, sample_rate: rate });

  let pos = 0;
  await streamRaw(pcmPtr, len, rate, () => pos, (v) => (pos = v), token, maybeStart);
  Module._rb_buffer_free(pcmPtr, len);
}

function skipAfterError(i) {
  const next = i + 1;
  if (next < queue.length) startTrack(next, 0, true);
  else stop();
}

function readMetaJson(pathPtrArg) {
  const ptr = Module._rb_meta_read_json(pathPtrArg);
  if (!ptr) return { codec: '', duration_ms: 0 };
  const json = Module.UTF8ToString(ptr);
  Module._rb_string_free(ptr);
  try { return JSON.parse(json); } catch (_) { return { codec: '', duration_ms: 0 }; }
}

function applyTrackReplaygain(meta) {
  const rg = meta && meta.replaygain;
  if (!rg) return;
  const v = (x) => (x == null ? NaN : x);
  Module._rb_dsp_set_replaygain_gains(dsp, v(rg.track_gain_db), v(rg.album_gain_db), v(rg.track_peak), v(rg.album_peak));
}

function advanceAfterEnd() {
  if (repeat === 1) { startTrack(index, 0, true); return; }
  const next = index + 1;
  if (next < queue.length) startTrack(next, 0, true);
  else if (repeat === 2) startTrack(0, 0, true);
  else stop();
}

// ── Transport ───────────────────────────────────────────────────────────────
function play() {
  if (queue.length === 0) return;
  if (!live && !rawPtr) { startTrack(index >= 0 ? index : 0, 0, true); return; }
  playing = true; userPaused = false; setWorkletPaused(false);
  emitStatus();
}
function pause() { userPaused = true; setWorkletPaused(true); emitStatus(); }
function stop() {
  loadToken++;
  playing = false; userPaused = false; live = false;
  freeRaw();
  flushWorklet();
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
  if (live || !rawPtr) return; // live isn't seekable
  seekBaseMs = ms;
  flushWorklet();
  Module._rb_dsp_flush(dsp);
  curInRate = 0;
  finitePos = Math.min(rawLen, Math.floor(ms * rawRate / 1000) * 2);
}

function setQueue(urls, autoplay) {
  queue = Array.isArray(urls) ? urls.slice() : [];
  index = -1;
  emitQueue();
  if (queue.length && autoplay) startTrack(0, 0, true);
  else stop();
}
function enqueue(url) {
  queue.push(url);
  emitQueue();
  if (playing && !live && !rawPtr) startTrack(index >= 0 ? index : 0, 0, true);
}
function clearQueue() { queue = []; index = -1; stop(); emitQueue(); }

// ── DSP passthrough ─────────────────────────────────────────────────────────
function applyDsp(name, args) {
  if (!dsp) return;
  const fn = Module['_rb_dsp_' + name];
  if (typeof fn === 'function') fn(dsp, ...args);
}

// ── ICY metadata ────────────────────────────────────────────────────────────
function updateLiveMeta(fields) {
  curMeta = { ...(curMeta || {}), ...fields, duration_ms: 0 };
  postMessage({ type: 'track', index, url: queue[index], live: true, metadata: curMeta });
}
function onIcyTitle(title) {
  const t = (title || '').trim();
  if (!t) return;
  const dash = t.indexOf(' - ');
  updateLiveMeta(dash > 0 ? { artist: t.slice(0, dash), title: t.slice(dash + 3) } : { title: t });
}
class IcyDemux {
  constructor(metaint, onTitle) {
    this.metaint = metaint; this.onTitle = onTitle;
    this.audioLeft = metaint; this.expectLen = false;
    this.metaLeft = 0; this.metaBuf = null; this.metaPos = 0;
    this.decoder = new TextDecoder('utf-8', { fatal: false });
  }
  push(bytes) {
    const audio = new Uint8Array(bytes.length);
    let ap = 0, i = 0;
    while (i < bytes.length) {
      if (this.audioLeft > 0) {
        const take = Math.min(this.audioLeft, bytes.length - i);
        audio.set(bytes.subarray(i, i + take), ap);
        ap += take; i += take; this.audioLeft -= take;
        if (this.audioLeft === 0) this.expectLen = true;
      } else if (this.expectLen) {
        this.metaLeft = bytes[i] * 16; i++; this.expectLen = false;
        if (this.metaLeft === 0) this.audioLeft = this.metaint;
        else { this.metaBuf = new Uint8Array(this.metaLeft); this.metaPos = 0; }
      } else {
        const take = Math.min(this.metaLeft, bytes.length - i);
        this.metaBuf.set(bytes.subarray(i, i + take), this.metaPos);
        this.metaPos += take; i += take; this.metaLeft -= take;
        if (this.metaLeft === 0) { this._emit(this.metaBuf); this.metaBuf = null; this.audioLeft = this.metaint; }
      }
    }
    return audio.subarray(0, ap);
  }
  _emit(buf) {
    let s; try { s = this.decoder.decode(buf); } catch (_) { return; }
    const m = /StreamTitle='(.*?)';/.exec(s);
    if (m && this.onTitle) this.onTitle(m[1]);
  }
}
function cancelBody(r) { try { if (r && r.body) r.body.cancel().catch(() => {}); } catch (_) {} }
function concatBytes(a, b) {
  if (a.length === 0) return b.slice();
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0); out.set(b, a.length);
  return out;
}

// ── Events to the main thread ───────────────────────────────────────────────
function stateName() {
  if (!playing && !live && !rawPtr) return 'stopped';
  return userPaused ? 'paused' : 'playing';
}
function emitStatus() {
  postMessage({ type: 'status', state: stateName(), index, queue_len: queue.length, shuffle, repeat });
}
function emitQueue() { postMessage({ type: 'queue', urls: queue, index }); }
function emitProgress() {
  postMessage({
    type: 'progress',
    state: stateName(),
    index,
    live,
    elapsed_ms: seekBaseMs + Math.round(wlConsumed * 1000 / sampleRate),
    duration_ms: curMeta ? (curMeta.duration_ms | 0) : 0,
    metadata: curMeta,
  });
}

// ── heap scratch helpers ─────────────────────────────────────────────────────
function allocPath(str) {
  const need = Module.lengthBytesUTF8(str) + 1;
  if (need > pathCap) { if (pathPtr) Module._free(pathPtr); pathPtr = Module._malloc(need); pathCap = need; }
  Module.stringToUTF8(str, pathPtr, pathCap);
  return pathPtr;
}
function copyPacket(bytes) {
  const n = bytes.length;
  if (n > pktCap) { if (pktPtr) Module._free(pktPtr); pktPtr = Module._malloc(n); pktCap = n; }
  Module.HEAPU8.set(bytes, pktPtr);
  return pktPtr;
}
