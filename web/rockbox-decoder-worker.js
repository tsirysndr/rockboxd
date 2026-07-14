/**
 * Rockbox decoder Worker.
 *
 * Owns the WebAssembly module (rockbox-core.wasm: rockbox-codecs + rockbox-dsp
 * + rockbox-metadata) and drives the whole "player" that the C firmware used
 * to provide — queue, transport, per-track decode, DSP, resampling — but in
 * plain JavaScript. Decoded + DSP-processed S16LE PCM is written into a shared
 * ring buffer that the AudioWorklet plays.
 *
 * Two source kinds:
 *   - Finite file (has Content-Length): buffered whole into MEMFS and decoded
 *     with one file decoder → full metadata, duration, seeking.
 *   - Live/infinite stream (no Content-Length, e.g. Icecast/SHOUTcast radio):
 *     the loop lives here in JS — read the network, slice it into segments,
 *     and decode each segment with a fresh file decoder, forwarding the PCM to
 *     the ring. No codec-side streaming, so nothing can park forever.
 *
 * Why a Worker: the codec decodes on its own pthread and next_chunk blocks on
 * a Condvar (Atomics.wait), which throws on the main browser thread — so all
 * decode work happens off it. The module is built with -pthread, so WASM
 * memory is a SharedArrayBuffer (page needs COOP/COEP — see the dev server).
 */

/* global RockboxModule */

// Shared control indices — keep in sync with rockbox-audio-worklet.js.
const CTRL_WRITE  = 0;
const CTRL_READ   = 1;
const CTRL_PAUSED = 2;
const CTRL_PLAYED = 3;
const CTRL_GEN    = 4;

// Live radio: decode the stream in segments of this many encoded bytes. Bigger
// = fewer decoder restarts (fewer boundary artifacts) but higher start latency.
const LIVE_SEGMENT = 32 * 1024;
// Buffer this many seconds of decoded audio before starting live playback, so
// segment-boundary timing jitter doesn't underrun the ring.
const LIVE_PREBUFFER_SEC = 2.5;

let Module     = null;
let ctrl       = null;  // Int32Array over controlSab
let ring       = null;  // Int16Array over audioSab
let ringFrames = 0;
let sampleRate = 44100; // AudioContext output rate; DSP resamples to it

let dsp     = null;     // *Dsp (created once, reused across tracks)
let dec     = null;     // *RbDecoder currently open (file, or a live segment)
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
let trackDecoded = false; // finite file hit EOT; draining the ring before advancing
let curInRate  = 0;     // last input rate handed to the DSP
let curMeta    = null;  // metadata JSON of the current track
let loadToken  = 0;     // bumped on every track change / stop — cancels stale async work
let pumpTimer  = null;
let live       = false; // current track is an unbounded live stream
let liveSeg    = 0;     // MEMFS filename counter for live segments

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

/** Run one decoded chunk (HEAP16[pcmSrc…], `len` i16 samples at `rate`) through
 *  the DSP and into the ring. */
function pushChunk(pcmPtr, len, rate) {
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
}

// ── Finite-file decode pump (setTimeout-driven) ──────────────────────────────
function schedulePump(ms) {
  clearTimeout(pumpTimer);
  pumpTimer = setTimeout(pump, ms);
}

function pump() {
  if (!playing || !dec || live) return;

  if (trackDecoded) {
    // Track fully decoded: let the ring drain, then advance for a clean cut.
    if (occupied() === 0) advanceAfterEnd();
    else schedulePump(40);
    return;
  }

  // Keep the ring at most ~half full (a couple of seconds of look-ahead). The
  // whole file is in MEMFS, so next_chunk only blocks briefly (never for real
  // time), which keeps the module main thread responsive.
  if (freeFrames() < (ringFrames >> 1)) { schedulePump(15); return; }

  const pcmPtr = Module._rb_decoder_next_chunk(dec, outLenPtr, outRatePtr);
  if (!pcmPtr) { trackDecoded = true; schedulePump(20); return; } // end of track

  const len  = Module.HEAPU32[outLenPtr  >> 2];
  const rate = Module.HEAPU32[outRatePtr >> 2];
  pushChunk(pcmPtr, len, rate);
  Module._rb_buffer_free(pcmPtr, len);

  schedulePump(0);
}

// ── Track lifecycle ─────────────────────────────────────────────────────────
function closeDecoder() {
  if (dec) { Module._rb_decoder_free(dec); dec = null; }
  if (decPath) { try { Module.FS.unlink(decPath); } catch (_) {} decPath = null; }
  trackDecoded = false;
  live = false;
  curInRate = 0;
}

function extOf(url) {
  const m = /\.([A-Za-z0-9]{1,5})(?:[?#]|$)/.exec(url);
  return m ? m[1].toLowerCase() : 'bin';
}

/** Codec/container hint for a live stream, from Content-Type then URL. */
function formatExt(contentType, url) {
  const ct = (contentType || '').toLowerCase();
  if (ct.includes('mpeg') || ct.includes('mp3')) return 'mp3';
  if (ct.includes('aac') || ct.includes('aacp')) return 'aac';
  if (ct.includes('ogg') || ct.includes('opus') || ct.includes('vorbis')) return 'ogg';
  if (ct.includes('flac')) return 'flac';
  if (ct.includes('wav')) return 'wav';
  const e = extOf(url);
  return e === 'bin' ? 'mp3' : e; // radio URLs often have no extension → assume mp3
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

  let resp;
  try {
    resp = await fetch(url);
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
  } catch (err) {
    postMessage({ type: 'error', message: `fetch failed: ${url} (${err})`, index: i });
    if (token === loadToken) skipAfterError(i);
    return;
  }
  if (token !== loadToken) return; // a newer load superseded this one

  // A Content-Length means a finite, seekable file. No length — chunked /
  // Icecast / SHOUTcast — means an unbounded live stream. `icy-*` headers
  // (when CORS-exposed) are a hard live signal.
  const hasLength = resp.headers.get('content-length') != null;
  const icy = resp.headers.get('icy-metaint') != null ||
              resp.headers.get('icy-name')    != null;
  const isLive = icy || !hasLength;

  if (isLive) return playLiveStream(resp, url, i, token, autoplay);
  return openFiniteFile(resp, url, i, token, seekMs, autoplay);
}

/** Finite file: buffer it whole into MEMFS → full metadata + seeking. */
async function openFiniteFile(resp, url, i, token, seekMs, autoplay) {
  let bytes;
  try {
    bytes = new Uint8Array(await resp.arrayBuffer());
  } catch (err) {
    postMessage({ type: 'error', message: `fetch failed: ${url} (${err})`, index: i });
    if (token === loadToken) skipAfterError(i);
    return;
  }
  if (token !== loadToken) return;

  const path = `/track_${token}.${extOf(url)}`;
  Module.FS.writeFile(path, bytes);
  dec = Module._rb_decoder_open(allocPath(path));
  if (!dec) {
    try { Module.FS.unlink(path); } catch (_) {}
    postMessage({ type: 'error', message: `cannot decode: ${url}`, index: i });
    skipAfterError(i);
    return;
  }
  decPath = path;
  live = false;

  curMeta = readMetadata();
  applyTrackReplaygain(curMeta);
  Module._rb_dsp_flush(dsp);
  if (seekMs) Module._rb_decoder_seek_ms(dec, seekMs);

  postMessage({ type: 'track', index: i, url, live: false, metadata: curMeta });
  if (autoplay || playing) { playing = true; userPaused = false; setPaused(false); schedulePump(0); }
  emitStatus();
}

/**
 * Live/infinite stream: the "player loop" in JS. Read the network, slice it
 * into segments, and decode each segment with a throwaway file decoder,
 * forwarding PCM to the ring. Runs until the stream ends or `token` is
 * superseded (a new track / stop bumps loadToken).
 */
async function playLiveStream(resp, url, i, token, autoplay) {
  live = true;
  playing = true; userPaused = false; // worklet stays paused (buffering) until prebuffer
  const ext = formatExt(resp.headers.get('content-type'), url);

  // Try to upgrade to an ICY-metadata connection so we can read StreamTitle
  // (current song). This needs the server to honour the `Icy-MetaData` request
  // and expose `icy-metaint` over CORS — many public stations don't, so we
  // fall back silently to the plain audio stream.
  let metaint = 0;
  let station = resp.headers.get('icy-name') || '';
  let icyBr   = parseInt(resp.headers.get('icy-br') || '0', 10);
  try {
    const r = await fetch(url, { headers: { 'Icy-MetaData': '1' } });
    if (token !== loadToken) { cancelBody(r); cancelBody(resp); return; }
    const mi = r.ok ? parseInt(r.headers.get('icy-metaint') || '0', 10) : 0;
    if (mi > 0) {
      cancelBody(resp);                    // drop the non-ICY connection
      resp = r; metaint = mi;
      station = r.headers.get('icy-name') || station;
      icyBr   = parseInt(r.headers.get('icy-br') || '0', 10) || icyBr;
    } else {
      cancelBody(r);                       // ICY unavailable — keep the original
    }
  } catch (_) { /* CORS / preflight blocked the ICY request — no metadata */ }
  if (token !== loadToken) { cancelBody(resp); return; }

  curMeta = { codec: ext, duration_ms: 0 };
  if (station) curMeta.station = station;
  if (icyBr)   curMeta.bitrate = icyBr; // station bitrate until a segment decodes
  postMessage({ type: 'track', index: i, url, live: true, metadata: curMeta });
  emitStatus();

  const demux = metaint > 0 ? new IcyDemux(metaint, onIcyTitle) : null;
  const reader = resp.body.getReader();
  const prebufFrames = Math.round(LIVE_PREBUFFER_SEC * sampleRate);
  let pending = new Uint8Array(0);
  let gotMeta = false;
  let started = false;

  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (token !== loadToken) { await reader.cancel().catch(() => {}); return; }
      if (value && value.length) {
        const audio = demux ? demux.push(value) : value; // strip ICY metadata blocks
        if (audio.length) pending = concatBytes(pending, audio);
      }

      // Decode as many whole segments as we have buffered.
      while (token === loadToken && pending.length >= LIVE_SEGMENT) {
        const seg = pending.slice(0, LIVE_SEGMENT);
        pending  = pending.slice(LIVE_SEGMENT);
        await decodeSegment(seg, ext, token, !gotMeta);
        gotMeta = true;
        if (token !== loadToken) return;
        // Start playback once enough is buffered to ride out boundary jitter.
        if (!started && !userPaused && occupied() >= prebufFrames) { started = true; setPaused(false); }
      }
      if (done) break;
    }
    // Stream ended: decode the tail, then advance the queue.
    if (token === loadToken && pending.length) await decodeSegment(pending, ext, token, !gotMeta);
    if (token === loadToken) { if (!started && !userPaused) setPaused(false); advanceAfterEnd(); }
  } catch (err) {
    if (token === loadToken) {
      postMessage({ type: 'error', message: `stream error: ${url} (${err})`, index: i });
      advanceAfterEnd();
    }
  }
}

function cancelBody(r) { try { if (r && r.body) r.body.cancel().catch(() => {}); } catch (_) {} }

/** Update the live track's metadata (StreamTitle etc.) and notify the UI. */
function updateLiveMeta(fields) {
  curMeta = { ...(curMeta || {}), ...fields, duration_ms: 0 };
  postMessage({ type: 'track', index, url: queue[index], live: true, metadata: curMeta });
}

/** ICY StreamTitle is usually "Artist - Song". */
function onIcyTitle(title) {
  const t = (title || '').trim();
  if (!t) return;
  const dash = t.indexOf(' - ');
  updateLiveMeta(dash > 0 ? { artist: t.slice(0, dash), title: t.slice(dash + 3) } : { title: t });
}

/**
 * SHOUTcast/Icecast ICY metadata demuxer. The audio is interleaved with
 * metadata: every `metaint` audio bytes comes a length byte (× 16) then that
 * many bytes of `StreamTitle='…';…`. `push` returns just the audio bytes and
 * fires `onTitle` whenever a new StreamTitle arrives.
 */
class IcyDemux {
  constructor(metaint, onTitle) {
    this.metaint  = metaint;
    this.onTitle  = onTitle;
    this.audioLeft = metaint;
    this.expectLen = false;
    this.metaLeft = 0;
    this.metaBuf  = null;
    this.metaPos  = 0;
    this.decoder  = new TextDecoder('utf-8', { fatal: false });
  }
  push(bytes) {
    const audio = new Uint8Array(bytes.length); // audio ≤ input (metadata removed)
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
    let s;
    try { s = this.decoder.decode(buf); } catch (_) { return; }
    const m = /StreamTitle='(.*?)';/.exec(s);
    if (m && this.onTitle) this.onTitle(m[1]);
  }
}

/**
 * Decode one self-contained encoded buffer into the ring. Uses `dec` (the
 * global handle) so a track change's closeDecoder() can free it and release
 * the codec gate; on abort we return WITHOUT freeing (closeDecoder owns it).
 * Every `dec` use is preceded by a token check with no `await` in between.
 */
async function decodeSegment(bytes, ext, token, readMeta) {
  const path = `/live_${token}_${liveSeg++}.${ext}`;
  Module.FS.writeFile(path, bytes);
  dec = Module._rb_decoder_open(allocPath(path));
  if (!dec) { try { Module.FS.unlink(path); } catch (_) {} return; } // skip undecodable slice
  decPath = path;
  curInRate = 0;
  Module._rb_dsp_flush(dsp);

  if (readMeta) {
    // Merge the decoded codec/rate into curMeta without clobbering ICY fields
    // (station / StreamTitle) that may already be set.
    const m = readMetadata();
    if (m) updateLiveMeta({ codec: m.codec, sample_rate: m.sample_rate,
                            bitrate: m.bitrate || (curMeta && curMeta.bitrate) || 0 });
  }

  for (;;) {
    if (token !== loadToken) return;              // aborted — closeDecoder owns `dec`
    if (userPaused) { await sleep(30); continue; }
    if (freeFrames() < (ringFrames >> 2)) { await sleep(15); continue; } // ring backpressure

    const pcmPtr = Module._rb_decoder_next_chunk(dec, outLenPtr, outRatePtr);
    if (!pcmPtr) break;                            // segment fully decoded
    const len  = Module.HEAPU32[outLenPtr  >> 2];
    const rate = Module.HEAPU32[outRatePtr >> 2];
    pushChunk(pcmPtr, len, rate);
    Module._rb_buffer_free(pcmPtr, len);
  }

  if (token === loadToken && dec) {
    Module._rb_decoder_free(dec); dec = null;
    try { Module.FS.unlink(path); } catch (_) {}
    decPath = null;
  }
}

function concatBytes(a, b) {
  if (a.length === 0) return b.slice();
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}
function sleep(ms) { return new Promise((r) => setTimeout(r, ms)); }

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
  if (!dec && !live) { startTrack(index >= 0 ? index : 0, 0, true); return; }
  playing = true; userPaused = false; setPaused(false);
  if (!live) schedulePump(0); // the live loop self-drives; it only checks userPaused
  emitStatus();
}
function pause() {
  userPaused = true; setPaused(true);
  emitStatus();
}
function stop() {
  loadToken++;               // cancel any running live loop / pending track load
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
  if (!dec || live) return; // live streams aren't seekable
  startTrack(index, ms, playing);
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
  if (playing && !dec && !live) startTrack(index >= 0 ? index : 0, 0, true);
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
  if (!playing && !dec && !live) return 'stopped';
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
    live,
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
