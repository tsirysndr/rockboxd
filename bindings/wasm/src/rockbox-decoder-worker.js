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
 *   - Finite MP3/AAC: streamed progressively as it downloads (~32 KB segments
 *     via rb_decode_packet) so playback starts fast and a big file is never
 *     held whole. Not seekable in this mode.
 *   - Other finite files (FLAC/Ogg/ALAC/…): a mid-file chunk has no header, so
 *     the whole stream is drained then decoded with rb_decode_file → tags,
 *     duration, seeking.
 *   - Live stream (no Content-Length): same ~32 KB segment path; ICY StreamTitle
 *     is demuxed for now-playing metadata.
 *
 * Format is detected from the file's magic bytes, then Content-Type, then the
 * URL extension (so extension-less URLs like /tracks/<id> work).
 */

/* global RockboxModule */

const LIVE_SEGMENT = 32 * 1024;        // encoded bytes per live-radio decode
const LIVE_PREBUFFER_SEC = 3;          // buffer before starting live playback
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
let streaming  = false; // a progressive segment loop (live or streamed-finite) is active
let loadToken  = 0;    // bumped on every track change / stop
let curInRate  = 0;

// ── Crossfade (Rockbox pcmbuf port — see crates/rockbox-playback/src/crossfade.rs)
// Settings in seconds, mirroring apps/settings_list.c ranges & defaults.
let xfadeCfg = { mode: 'off', foDelay: 0, foDur: 2, fiDelay: 0, fiDur: 2, mix: 'crossfade' };
let xfade    = null; // active mixer state (armed at a crossfaded transition)
let holdChunks = []; // outgoing-tail holdback: decoded PCM not yet sent to the worklet
let holdLen  = 0;    // frames currently held
let postedFrames = 0; // frames posted to the worklet since its last flush
let trackBase = 0;   // posted-frame epoch where the current track audibly starts

// Finite-track decoded PCM, kept alive so we can seek within it.
let rawPtr = 0, rawLen = 0, rawRate = 0, finitePos = 0;

// Gapless prefetch: the predicted next track's raw file bytes, fetched into
// memory during the current track's playback so an auto-advance decodes
// straight from RAM with no network wait (the "silence before the next track"
// gap). Whole-file (drain) formats only — MP3/AAC already start fast via
// progressive streaming.
let prefetch = null;      // { url, bytes: Uint8Array }
let prefetchUrl = null;   // url currently being fetched (single-flight guard)
let prefetchAbort = null; // AbortController for the in-flight prefetch

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
    case 'insert':     return insertTracks(msg.urls, msg.position, msg.index | 0);
    case 'removeAt':   return removeAt(msg.index | 0);
    case 'clearQueue': return clearQueue();
    case 'play':       return play();
    case 'pause':      return pause();
    case 'toggle':     return userPaused || !playing ? play() : pause();
    case 'stop':       return stop();
    case 'next':       return skip(+1);
    case 'prev':       return skip(-1);
    case 'skipTo':     return startTrack(msg.index | 0, 0, true);
    case 'seek':       return seek(msg.ms);
    case 'shuffle':    shuffle = !!msg.enabled; return emitStatus();
    case 'repeat':     repeat  = msg.mode | 0;  return emitStatus();
    case 'crossfade':  return setXfadeCfg(msg);
    case 'dsp':        return applyDsp(msg.name, msg.args);
  }
};

function setXfadeCfg(m) {
  const MODES = ['off', 'auto-skip', 'manual-skip', 'shuffle', 'shuffle-or-manual', 'always'];
  xfadeCfg = {
    mode: typeof m.mode === 'number' ? (MODES[m.mode] || 'off') : (m.mode || 'off'),
    foDelay: Math.max(0, +m.fadeOutDelay    || 0),
    foDur:   Math.max(0, +m.fadeOutDuration || 0),
    fiDelay: Math.max(0, +m.fadeInDelay     || 0),
    fiDur:   Math.max(0, +m.fadeInDuration  || 0),
    mix: (m.mixMode === 1 || m.mixMode === 'mix') ? 'mix' : 'crossfade',
  };
}

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
    if (m.type === 'level') {
      // Derive the queue from monotonic counters instead of trusting the
      // worklet's snapshot: `m.queued` describes a PAST state and counts
      // in-flight (posted but not yet delivered) chunks as zero. Trusting it
      // collapsed backpressure, "finished" the track in seconds, and the
      // end-of-track flush then wiped the audio right as it arrived.
      wlConsumed = m.consumed;
      wlQueued = Math.max(0, postedFrames - wlConsumed);
    }
  };
}

// ── Live heap views ─────────────────────────────────────────────────────────
// ALLOW_MEMORY_GROWTH replaces the wasm memory buffer when the heap grows,
// which detaches any previously created view — including the Module.HEAPxx
// snapshots. A detached Int16Array has length 0, so a stale `HEAP16.slice`
// silently returns EMPTY chunks (audio dies while everything "works").
// Always build a fresh view over the LIVE buffer for every access.
function liveBuf() {
  const m = Module.wasmMemory;
  return m ? m.buffer : Module.HEAPU8.buffer;
}
function copyI16(bytePtr, samples) {
  return new Int16Array(liveBuf(), bytePtr, samples).slice(); // detached-proof copy
}
function readU32(bytePtr) {
  return new Uint32Array(liveBuf(), bytePtr, 1)[0];
}
function writeU8(bytes, bytePtr) {
  new Uint8Array(liveBuf(), bytePtr, bytes.length).set(bytes);
}

// ── Worklet transport ─────────────────────────────────────────────────────
function flushWorklet() {
  pcmPort && pcmPort.postMessage({ type: 'flush' });
  wlConsumed = 0; wlQueued = 0; postedFrames = 0; trackBase = 0;
}
function setWorkletPaused(v) { pcmPort && pcmPort.postMessage({ type: 'paused', value: v }); }

/** Copy `procLen` i16 samples at HEAP16[procPtr…] out of the heap, free the
 *  heap buffer, and run the copy through the output pipeline
 *  (crossfade mixer → tail holdback → worklet). */
function postPcm(procPtr, procLen) {
  const copy = copyI16(procPtr, procLen);
  Module._rb_buffer_free(procPtr, procLen);
  pushPcm(copy);
}

/** Final hop: transfer an Int16Array to the worklet. */
function emitPcm(arr) {
  // Read the length BEFORE posting: the transfer detaches arr.buffer, after
  // which arr.length is 0 — counting after the post makes every chunk count
  // as zero frames, backpressure never engages, the track "finishes"
  // instantly and the end-of-track flush wipes the audio (the "plays one
  // second then stops" bug).
  const frames = arr.length >> 1;
  pcmPort.postMessage({ pcm: arr.buffer }, [arr.buffer]);
  postedFrames += frames;
  wlQueued = Math.max(0, postedFrames - wlConsumed);
}

/** Output pipeline: crossfade-mix if a fade is armed, then hold back the
 *  newest `holdFrames()` so a transition has an outgoing tail to fade. */
function pushPcm(arr) {
  if (xfade) {
    for (const part of xfadeMix(arr)) holdPush(part);
    return;
  }
  holdPush(arr);
}

function holdPush(arr) {
  const H = holdFrames();
  if (H <= 0) { emitPcm(arr); return; }
  holdChunks.push(arr);
  holdLen += arr.length >> 1;
  while (holdChunks.length && holdLen - (holdChunks[0].length >> 1) >= H) {
    const c = holdChunks.shift();
    holdLen -= c.length >> 1;
    emitPcm(c);
  }
}

/** Take the held tail as one flat buffer (for a crossfaded transition). */
function takeTail() {
  const tail = new Int16Array(holdLen * 2);
  let off = 0;
  for (const c of holdChunks) { tail.set(c, off); off += c.length; }
  holdChunks = []; holdLen = 0;
  return tail;
}

/** Send everything held to the worklet (plain, non-crossfaded track end). */
function flushHold() {
  for (const c of holdChunks) emitPcm(c);
  holdChunks = []; holdLen = 0;
}

function dropHold() { holdChunks = []; holdLen = 0; }

// ── Rockbox pcmbuf crossfade, ported from apps/pcmbuf.c via
//    crates/rockbox-playback/src/crossfade.rs (Q16 gains, Bresenham ramps,
//    saturating mix). ──────────────────────────────────────────────────────
const XF_UNITY = 1 << 16;

/** Linear fade stepper — pcmbuf.c mixfader_init / mixfader_step. */
class MixFader {
  constructor(start, end, nframes) {
    const nsamp2 = nframes * 2;
    this.endfac = end;
    this.nsamp2 = nsamp2;
    if (nsamp2 === 0) { this.factor = end; this.ferr = 0; this.dfquo = 0; this.dfrem = 0; this.dfinc = 0; return; }
    const dfact2 = 2 * Math.abs(end - start);
    this.factor = start;
    this.ferr = dfact2 >> 1;
    this.dfinc = end < start ? -1 : 1;
    this.dfquo = Math.trunc(dfact2 / nsamp2) * this.dfinc;
    this.dfrem = dfact2 - Math.trunc(dfact2 / nsamp2) * nsamp2;
  }
  step() {
    if (this.factor === this.endfac) return;
    this.factor += this.dfquo;
    this.ferr += this.dfrem;
    if (this.ferr >= this.nsamp2) { this.factor += this.dfinc; this.ferr -= this.nsamp2; }
  }
}

/** pcmbuf.c mixfade_sample(): apply a Q16 gain with rounding. */
const mixfadeSample = (factor, s) => (factor * s + (XF_UNITY >> 1)) >> 16;
const clip16 = (s) => (s > 32767 ? 32767 : s < -32768 ? -32768 : s);

/** Does a transition crossfade? `auto` = the track ended on its own. */
function xfadeApplies(auto) {
  switch (xfadeCfg.mode) {
    case 'auto-skip':         return auto;
    case 'manual-skip':       return !auto;
    case 'shuffle':           return shuffle;
    case 'shuffle-or-manual': return shuffle || !auto;
    case 'always':            return true;
    default:                  return false;
  }
}

/** Outgoing tail to keep for the fade-out (frames). */
function holdFrames() {
  if (live || xfadeCfg.mode === 'off') return 0; // radio doesn't crossfade
  return Math.round((xfadeCfg.foDelay + xfadeCfg.foDur) * sampleRate);
}

/** Arm the mixer with the outgoing track's tail at a transition. */
function armCrossfade(tail) {
  const fr = (s) => Math.round(s * sampleRate);
  const foDelay = fr(xfadeCfg.foDelay), foDur = fr(xfadeCfg.foDur);
  const fiDelay = fr(xfadeCfg.fiDelay), fiDur = fr(xfadeCfg.fiDur);
  const region = Math.max(foDelay + foDur, fiDelay + fiDur, tail.length >> 1);
  xfade = {
    tail, pos: 0, region, foDelay, foDur, fiDelay, fiDur,
    mix: xfadeCfg.mix === 'mix',
    fo: new MixFader(XF_UNITY, 0, foDur),
    fi: new MixFader(0, XF_UNITY, fiDur),
  };
}

/** Gains for the current region frame (stepping the faders). */
function xfadeGains(x) {
  let og;
  if (x.mix) og = XF_UNITY;
  else if (x.pos < x.foDelay) og = XF_UNITY;
  else if (x.pos < x.foDelay + x.foDur) { og = x.fo.factor; x.fo.step(); }
  else og = 0;
  let ig;
  if (x.pos < x.fiDelay) ig = 0;
  else if (x.pos < x.fiDelay + x.fiDur) { ig = x.fi.factor; x.fi.step(); }
  else ig = XF_UNITY;
  return [og, ig];
}

/** Mix an incoming chunk against the outgoing tail; returns arrays to emit.
 *  Frames past the crossfade region pass through untouched. */
function xfadeMix(chunk) {
  const x = xfade;
  const chunkFrames = chunk.length >> 1;
  const n = Math.min(x.region - x.pos, chunkFrames);
  const tailFrames = x.tail.length >> 1;
  const out = new Int16Array(n * 2);
  for (let f = 0; f < n; f++) {
    const [og, ig] = xfadeGains(x);
    const t = x.pos < tailFrames ? x.pos * 2 : -1;
    const tl = t >= 0 ? x.tail[t] : 0;
    const tr = t >= 0 ? x.tail[t + 1] : 0;
    out[f * 2]     = clip16(mixfadeSample(og, tl) + mixfadeSample(ig, chunk[f * 2]));
    out[f * 2 + 1] = clip16(mixfadeSample(og, tr) + mixfadeSample(ig, chunk[f * 2 + 1]));
    x.pos++;
  }
  const parts = [out];
  if (x.pos >= x.region) {
    xfade = null;
    if (n < chunkFrames) parts.push(chunk.slice(n * 2)); // remainder passes through
  }
  return parts;
}

/** Incoming track ended while the fade was still active — play the rest of
 *  the outgoing tail out through its fade so it doesn't cut. */
function finalizeCrossfade() {
  const x = xfade;
  if (!x) return null;
  xfade = null;
  const tailFrames = x.tail.length >> 1;
  const n = Math.max(0, tailFrames - x.pos);
  if (n <= 0) return null;
  const out = new Int16Array(n * 2);
  for (let f = 0; f < n; f++) {
    const [og] = xfadeGains(x);
    out[f * 2]     = clip16(mixfadeSample(og, x.tail[x.pos * 2]));
    out[f * 2 + 1] = clip16(mixfadeSample(og, x.tail[x.pos * 2 + 1]));
    x.pos++;
  }
  return out;
}

// Keep the worklet queue short when crossfade can trigger on a manual skip,
// so the fade starts near "now" (queued audio can't be pulled back).
const highWater = () => {
  if (live) return 5 * sampleRate; // radio: ride out network jitter
  const manualCapable = xfadeCfg.mode !== 'off' && xfadeCfg.mode !== 'auto-skip';
  return manualCapable ? Math.round(0.6 * sampleRate) : 3 * sampleRate;
};
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
    const procLen = readU32(procLenPtr);
    setPos(pos + chunkLen);
    if (procPtr && procLen >= 2) postPcm(procPtr, procLen);
    else if (procPtr) Module._rb_buffer_free(procPtr, procLen);
    if (onProgress) onProgress();
  }
}

/**
 * Wait for the worklet's queue to finish PLAYING before auto-advancing —
 * advancing early would flush audio that hasn't been heard yet (the "track
 * stops after a few seconds" bug: everything decoded fast, then the advance
 * flushed the queue). Returns 'done' | 'aborted' | 'again' (the optional
 * `again()` predicate signals a seek rewound the cursor, so stream more).
 * A stall watchdog bails out if the queue stops shrinking while unpaused
 * (broken level reports must not wedge the queue advance forever).
 */
async function waitForDrain(token, again) {
  let last = wlQueued;
  let stall = 0;
  for (;;) {
    if (token !== loadToken) return 'aborted';
    if (again && again()) return 'again';
    if (wlQueued <= 0) return 'done';
    await sleep(50);
    if (userPaused) { stall = 0; continue; }
    if (wlQueued < last) { last = wlQueued; stall = 0; }
    else if (++stall > 40) return 'done'; // ~2 s with no progress
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

/** Detect the audio format from a buffer's magic bytes; returns a Rockbox file
 *  extension or null. Handles the common cases so extension-less URLs decode. */
function sniffExt(b) {
  if (b.length < 12) return null;
  const tag = (i, s) => { for (let j = 0; j < s.length; j++) if (b[i + j] !== s.charCodeAt(j)) return false; return true; };
  if (tag(0, 'fLaC')) return 'flac';
  if (tag(0, 'OggS')) return 'ogg';                       // vorbis / opus / speex
  if (tag(0, 'RIFF') && tag(8, 'WAVE')) return 'wav';
  if (tag(0, 'FORM') && tag(8, 'AIFF')) return 'aiff';
  if (tag(4, 'ftyp')) return 'm4a';                       // MP4 / M4A (AAC, ALAC)
  if (tag(0, 'wvpk')) return 'wv';                        // WavPack
  if (tag(0, 'MAC ')) return 'ape';                       // Monkey's Audio
  if (tag(0, 'TTA1')) return 'tta';                       // True Audio
  if (tag(0, 'MPCK') || tag(0, 'MP+')) return 'mpc';      // Musepack
  if (tag(0, '.snd')) return 'au';
  if (tag(0, 'ID3')) return 'mp3';                        // ID3-tagged MP3
  if (b[0] === 0xff && (b[1] & 0xe6) === 0xe2) return 'mp3'; // MPEG audio frame sync
  if (b[0] === 0xff && (b[1] & 0xf6) === 0xf0) return 'aac'; // ADTS AAC
  return null;
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

async function startTrack(i, seekMs, autoplay, xfadeTail = null) {
  if (i < 0 || i >= queue.length) { stop(); return; }
  const token = ++loadToken;
  const url = queue[i];

  freeRaw();
  live = false;
  streaming = false;
  index = i;
  lastInsertPos = -1; // new playing track resets the Insert chain anchor
  curInRate = 0;
  seekBaseMs = seekMs || 0;
  if (xfadeTail && xfadeTail.length) {
    // Crossfaded transition: keep the worklet playing (no flush, no pause);
    // the incoming track's PCM will be mixed against this outgoing tail.
    xfade = null; // a still-active fade's remainder was captured in the tail
    armCrossfade(xfadeTail);
    trackBase = postedFrames + Math.round(xfadeCfg.fiDelay * sampleRate);
  } else {
    xfade = null;
    dropHold(); // a skip discards un-played audio
    flushWorklet();
    // Hold the worklet while the new track loads/prebuffers; the buffered path
    // unpauses on play and the segment loop unpauses once prebuffered.
    setWorkletPaused(true);
  }
  Module._rb_dsp_flush(dsp);

  // Gapless fast-path: if we prefetched this track's bytes during the previous
  // one, decode straight from memory — no network round-trip, no re-drain.
  if (prefetch && prefetch.url === url) {
    const pf = prefetch;
    prefetch = null;
    // Faststart AAC was pre-demuxed to ADTS during the previous track — stream
    // it progressively (bounded memory). Everything else decodes whole-file.
    if (pf.adts) return playAdtsBuffer(pf.adts, url, i, token, pf.durationMs, pf.tags);
    const ext = sniffExt(pf.bytes.subarray(0, 16)) || formatExt(null, url);
    return playDecodedBytes(pf.bytes, ext, url, i, token, seekMs, autoplay);
  }
  prefetch = null; // any earlier prefetch predicted a different next track

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

/**
 * Finite file. Sniff the head, then either:
 *   - MP3/AAC — decode progressively in ~32 KB segments as it downloads, so
 *     playback starts within ~a second and memory stays bounded (a big file
 *     is never held whole). Not seekable in this mode.
 *   - anything else (FLAC/Ogg/ALAC/…) — a mid-file chunk has no header, so we
 *     drain the whole stream then whole-file decode (tags, duration, seeking).
 */
async function playFinite(resp, url, i, token, seekMs, autoplay) {
  const reader = resp.body.getReader();
  let head = new Uint8Array(0);
  let done = false;
  while (head.length < 16 && !done) {
    let r;
    try { r = await reader.read(); }
    catch (err) {
      postMessage({ type: 'error', message: `fetch failed: ${url} (${err})`, index: i });
      if (token === loadToken) skipAfterError(i);
      return;
    }
    if (token !== loadToken) { reader.cancel().catch(() => {}); return; }
    if (r.value && r.value.length) head = concatBytes(head, r.value);
    done = r.done;
  }
  // Magic bytes → Content-Type → URL extension.
  const ext = sniffExt(head) || formatExt(resp.headers.get('content-type'), url);

  if (ext === 'mp3' || ext === 'aac') {
    live = false; playing = true; userPaused = false;
    Module._rb_dsp_flush(dsp); curInRate = 0;
    curMeta = { codec: ext, duration_ms: 0 };
    postMessage({ type: 'track', index: i, url, live: false, metadata: curMeta });
    emitStatus();
    // Prefetch the next track even while this MP3/AAC streams progressively, so
    // a whole-file next track (FLAC/…) decodes from memory with no gap.
    void prefetchNext();
    return runSegmentLoop(reader, head, done, ext, token, url, i, null);
  }

  // M4A/MP4 (AAC): stream progressively by re-framing container samples as
  // ADTS — but only on a fresh play (seekMs 0); a seek needs the whole-file
  // path below, which is seekable. Non-faststart / ALAC / HE-AAC files fall
  // back to whole-file from inside playMp4.
  if ((ext === 'm4a' || ext === 'mp4' || ext === 'm4b') && !seekMs) {
    return playMp4(reader, head, done, url, i, token, autoplay);
  }

  // Buffer the rest, then whole-file decode (seekable).
  const bytes = await drainToBytes(reader, head, done, token);
  if (!bytes || token !== loadToken) return;
  return playDecodedBytes(bytes, ext, url, i, token, seekMs, autoplay);
}

/** Whole-file decode a fully-buffered track, then stream it. Shared by the
 *  drain path (playFinite) and the gapless prefetch fast-path (startTrack), so
 *  an auto-advance whose bytes were prefetched decodes straight from memory. */
async function playDecodedBytes(bytes, ext, url, i, token, seekMs, autoplay) {
  const path = `/track.${ext}`;
  Module.FS.writeFile(path, bytes);
  const p = allocPath(path);
  curMeta = readMetaJson(p);
  applyTrackReplaygain(curMeta);
  Module._rb_dsp_flush(dsp); curInRate = 0;
  rawPtr = Module._rb_decode_file(p, outLenPtr, outRatePtr);
  rawLen = readU32(outLenPtr);
  rawRate = readU32(outRatePtr) || (curMeta && curMeta.sample_rate) || sampleRate;
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
  // Now that this track is decoded and playing, prefetch the next one's bytes
  // in the background so its transition is gapless.
  void prefetchNext();
  // Stream the decoded buffer; when it's fully posted, either crossfade into
  // the next track (natural end) or let the worklet PLAY the queue out before
  // advancing. A seek can rewind the cursor while draining — stream again.
  for (;;) {
    const finished = await streamRaw(rawPtr, rawLen, rawRate, () => finitePos, (v) => (finitePos = v), token);
    if (!finished || token !== loadToken) return;
    const fin = finalizeCrossfade(); // this track started mid-fade and is short
    if (fin) holdPush(fin);
    const nxt = nextAfterEnd();
    if (nxt != null && xfadeApplies(true) && holdLen > 0) {
      const tail = takeTail();
      freeRaw();
      startTrack(nxt, 0, true, tail);
      return;
    }
    flushHold();
    const r = await waitForDrain(token, () => finitePos < rawLen);
    if (r === 'aborted') return;
    if (r === 'again') continue;
    break;
  }
  freeRaw();
  advanceAfterEnd();
}

/** Abort any in-flight prefetch (the predicted next track changed, or we're
 *  stopping). */
function cancelPrefetch() {
  if (prefetchAbort) { try { prefetchAbort.abort(); } catch (_) {} prefetchAbort = null; }
  prefetchUrl = null;
}

/** Fetch the predicted next track's bytes into memory while the current track
 *  plays, so its transition is gapless. Re-callable at any time (e.g. right
 *  after a "play next" insert): if the predicted next track changed, any
 *  in-flight prefetch for the old one is aborted and the new one starts
 *  immediately. Finite whole-file formats only — a live stream (no
 *  Content-Length) never finishes, and MP3/AAC start fast via progressive
 *  streaming (decoded by rb_decode_packet, not the whole-file decoder). */
async function prefetchNext() {
  const nxt = nextAfterEnd();
  const url = nxt != null ? queue[nxt] : null;
  if (!url) { cancelPrefetch(); prefetch = null; return; }
  if (prefetch && prefetch.url !== url) prefetch = null; // cached the wrong track
  if ((prefetch && prefetch.url === url) || prefetchUrl === url) return; // done / in flight
  cancelPrefetch(); // a different track was being prefetched — drop it
  prefetchUrl = url;
  const ac = new AbortController();
  prefetchAbort = ac;
  try {
    const r = await fetch(url, { signal: ac.signal });
    const finite =
      r.ok &&
      r.headers.get('content-length') != null &&
      r.headers.get('icy-metaint') == null &&
      r.headers.get('icy-name') == null;
    if (!finite) { try { await r.body?.cancel(); } catch (_) {} return; }
    const buf = new Uint8Array(await r.arrayBuffer());
    const ext = sniffExt(buf.subarray(0, 16)) || formatExt(r.headers.get('content-type'), url);
    if (ext === 'mp3' || ext === 'aac') return; // handled by the progressive path
    // Keep it only if it's still the predicted next track (the queue/index may
    // have moved while it downloaded).
    if (queue[nextAfterEnd()] !== url) return;
    if (ext === 'm4a' || ext === 'mp4' || ext === 'm4b') {
      // Pre-demux faststart AAC to ADTS now so the transition streams straight
      // from memory (bounded), same as a fresh progressive play. ALAC /
      // HE-AAC / moov-last fall back to a whole-file byte cache.
      const d = demuxMp4Buffer(buf);
      prefetch = d.mode === 'aac'
        ? { url, adts: d.adts, durationMs: d.durationMs, tags: d.tags }
        : { url, bytes: buf };
      return;
    }
    prefetch = { url, bytes: buf };
  } catch (_) {
    // aborted or network error — the normal fetch at advance time will handle it
  } finally {
    if (prefetchUrl === url) prefetchUrl = null;
    if (prefetchAbort === ac) prefetchAbort = null;
  }
}

/** Drain the rest of `reader` (after `head`) into one buffer. */
async function drainToBytes(reader, head, done, token) {
  const chunks = head.length ? [head] : [];
  let total = head.length;
  while (!done) {
    let r;
    try { r = await reader.read(); } catch (_) { return null; }
    if (token !== loadToken) { reader.cancel().catch(() => {}); return null; }
    if (r.value && r.value.length) { chunks.push(r.value); total += r.value.length; }
    done = r.done;
  }
  const out = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) { out.set(c, off); off += c.length; }
  return out;
}

/** Live stream: read the network, decode ~32 KB segments, stream the PCM. */
async function playLiveStream(resp, url, i, token, autoplay) {
  live = true;
  playing = true; userPaused = false;
  Module._rb_dsp_flush(dsp); curInRate = 0;
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
  return runSegmentLoop(resp.body.getReader(), new Uint8Array(0), false, ext, token, url, i, demux);
}

// ── Frame-aligned segmentation ────────────────────────────────────────────
// Slicing a stream at arbitrary byte offsets loses audio at EVERY boundary:
// the codec drops the partial frame at the slice end, re-syncs past the
// partial frame at the next slice's start, and (MP3) the bit reservoir's
// back-references break — an audible gap/click every ~2 s of radio. So for
// MP3 and ADTS AAC we parse frame headers in JS and cut only on frame
// boundaries, and prepend the last frames of the previous segment as a
// reservoir "primer" whose decoded samples are dropped from the output.

/** Parse an MPEG-audio frame header at b[i] → {len, spf} or null. */
function parseMpaFrame(b, i) {
  if (i + 4 > b.length) return null;
  if (b[i] !== 0xff || (b[i + 1] & 0xe0) !== 0xe0) return null;
  const ver = (b[i + 1] >> 3) & 3;   // 3=MPEG1, 2=MPEG2, 0=MPEG2.5, 1=reserved
  const layer = (b[i + 1] >> 1) & 3; // 1=III, 2=II, 3=I, 0=reserved
  if (ver === 1 || layer === 0) return null;
  const brIdx = b[i + 2] >> 4;
  const srIdx = (b[i + 2] >> 2) & 3;
  const pad = (b[i + 2] >> 1) & 1;
  if (brIdx === 0 || brIdx === 15 || srIdx === 3) return null; // free-format unsupported
  const v1 = ver === 3;
  const sr = (v1 ? [44100, 48000, 32000]
    : ver === 2 ? [22050, 24000, 16000] : [11025, 12000, 8000])[srIdx];
  let table;
  if (layer === 3)      table = v1 ? [0,32,64,96,128,160,192,224,256,288,320,352,384,416,448]
                                   : [0,32,48,56,64,80,96,112,128,144,160,176,192,224,256];
  else if (layer === 2) table = v1 ? [0,32,48,56,64,80,96,112,128,160,192,224,256,320,384]
                                   : [0,8,16,24,32,40,48,56,64,80,96,112,128,144,160];
  else                  table = v1 ? [0,32,40,48,56,64,80,96,112,128,160,192,224,256,320]
                                   : [0,8,16,24,32,40,48,56,64,80,96,112,128,144,160];
  const br = table[brIdx] * 1000;
  let len, spf;
  if (layer === 3)      { spf = 384;  len = (Math.floor(12 * br / sr) + pad) * 4; }
  else if (layer === 2) { spf = 1152; len = Math.floor(144 * br / sr) + pad; }
  else                  { spf = v1 ? 1152 : 576; len = Math.floor((v1 ? 144 : 72) * br / sr) + pad; }
  return len >= 4 ? { len, spf } : null;
}

/** Parse an ADTS (AAC) frame header at b[i] → {len, spf} or null. */
function parseAdtsFrame(b, i) {
  if (i + 7 > b.length) return null;
  if (b[i] !== 0xff || (b[i + 1] & 0xf6) !== 0xf0) return null;
  const len = ((b[i + 3] & 0x03) << 11) | (b[i + 4] << 3) | (b[i + 5] >> 5);
  return len >= 7 ? { len, spf: 1024 } : null;
}

/** Frame-aligned cutter over the shared `pending` buffer. Returns null for
 *  unsupported formats (caller falls back to blind slicing). */
function makeFramer(ext) {
  const parse = ext === 'mp3' ? parseMpaFrame : ext === 'aac' ? parseAdtsFrame : null;
  if (!parse) return null;
  return { parse, synced: false, primer: null, primerFrames: ext === 'mp3' ? 3 : 1 };
}

/**
 * Cut the next whole-frame segment (≥ LIVE_SEGMENT bytes unless `flush`) off
 * `pending`. Returns `{seg, frames, spf, primerBytes}` and the shortened
 * pending, or null when more data is needed.
 */
function takeAlignedSegment(fr, pending, flush) {
  const { parse } = fr;
  let start = 0;
  if (!fr.synced) {
    // Find a verified sync: a valid header whose length lands on another one.
    let i = 0;
    for (;;) {
      if (i + 8 >= pending.length) return { need: pending }; // wait for more
      const f = parse(pending, i);
      if (f) {
        if (i + f.len + 8 > pending.length) return { need: pending.slice(i) };
        if (parse(pending, i + f.len)) { start = i; fr.synced = true; break; }
      }
      i++;
    }
  }
  let off = start, frames = 0, spf = 0;
  const offs = [];
  while (off + 8 <= pending.length) {
    const f = parse(pending, off);
    if (!f) { fr.synced = false; break; } // glitch — cut what we have, resync after
    if (off + f.len > pending.length) break; // incomplete tail frame — wait
    offs.push(off);
    off += f.len; frames++; spf = f.spf;
    if (off - start >= LIVE_SEGMENT) break;
  }
  if (frames === 0) return { need: pending.slice(start) };
  if (!flush && fr.synced && off - start < LIVE_SEGMENT) return { need: pending.slice(start) };
  const seg = pending.slice(start, off);
  const pIdx = Math.max(0, frames - fr.primerFrames);
  return {
    seg, frames, spf,
    primerBytes: pending.slice(offs[pIdx], off),
    rest: pending.slice(off),
  };
}

/**
 * Progressive segment decoder shared by live streams and streamed-finite MP3/AAC.
 * Decodes `pending` (any pre-read head) then reads `reader` to the end, decoding
 * each frame-aligned ~LIVE_SEGMENT chunk with rb_decode_packet and streaming the
 * PCM (gapless via the reservoir primer). Starts playback once
 * ~LIVE_PREBUFFER_SEC is queued. `demux` (or null) strips ICY.
 */
async function runSegmentLoop(reader, pending, done, ext, token, url, i, demux) {
  streaming = true;
  // Prebuffer target must stay below the worklet high-water mark or the
  // backpressure gate would stop feeding before playback ever starts.
  const prebuf = Math.min(Math.round(LIVE_PREBUFFER_SEC * sampleRate),
                          Math.max(4096, highWater() - 8192));
  let gotMeta = false;
  let started = false;
  const framer = makeFramer(ext);
  const maybeStart = () => {
    if (!started && !userPaused && wlQueued >= prebuf) { started = true; setWorkletPaused(false); }
  };
  const drainPending = async (flush) => {
    for (;;) {
      if (token !== loadToken) return;
      if (framer) {
        const cut = takeAlignedSegment(framer, pending, flush);
        if (cut.need !== undefined) { pending = cut.need; return; }
        pending = cut.rest;
        // Prepend the previous segment's tail frames so the decoder has the
        // MP3 bit-reservoir data; their PCM is dropped via expectPcmFrames.
        const payload = framer.primer ? concatBytes(framer.primer, cut.seg) : cut.seg;
        const ok = await decodeSegment(payload, ext, token, !gotMeta, maybeStart,
                                       cut.frames * cut.spf);
        framer.primer = cut.primerBytes;
        if (ok) gotMeta = true;
      } else {
        // Unknown framing (e.g. Ogg): blind fixed-size slices.
        if (!flush && pending.length < LIVE_SEGMENT) return;
        if (pending.length === 0) return;
        const seg = pending.slice(0, LIVE_SEGMENT);
        pending = pending.slice(Math.min(LIVE_SEGMENT, pending.length));
        if (await decodeSegment(seg, ext, token, !gotMeta, maybeStart)) gotMeta = true;
      }
    }
  };
  try {
    await drainPending(false);
    while (token === loadToken && !done) {
      const r = await reader.read();
      if (token !== loadToken) { await reader.cancel().catch(() => {}); return; }
      if (r.value && r.value.length) {
        const audio = demux ? demux.push(r.value) : r.value;
        if (audio.length) pending = concatBytes(pending, audio);
      }
      done = r.done;
      await drainPending(done);
    }
    if (token === loadToken) {
      if (!started && !userPaused) setWorkletPaused(false);
      await finishTrack(token);
    }
  } catch (err) {
    if (token === loadToken) { postMessage({ type: 'error', message: `stream error: ${url} (${err})`, index: i }); flushHold(); advanceAfterEnd(); }
  }
}

/** What plays after the current track ends naturally (or null to stop). */
function nextAfterEnd() {
  if (repeat === 1) return index; // repeat one
  const next = index + 1;
  if (next < queue.length) return next;
  if (repeat === 2 && queue.length) return 0; // repeat all → wrap
  return null;
}

/** Natural end of a track: crossfade into the next when configured, else
 *  play the held tail + worklet queue out, then advance. */
async function finishTrack(token) {
  if (token !== loadToken) return;
  const fin = finalizeCrossfade(); // still mid-fade (very short track)
  if (fin) holdPush(fin);
  const nxt = nextAfterEnd();
  if (nxt != null && xfadeApplies(true) && holdLen > 0) {
    startTrack(nxt, 0, true, takeTail());
    return;
  }
  flushHold();
  if ((await waitForDrain(token)) === 'done') advanceAfterEnd();
}

/** Decode one self-contained encoded packet in memory and stream its PCM.
 *  Returns true if the segment produced audio. */
async function decodeSegment(bytes, ext, token, readMeta, maybeStart, expectPcmFrames = 0) {
  const dataPtr = copyPacket(bytes);
  const pcmPtr = Module._rb_decode_packet(dataPtr, bytes.length, allocPath(ext), outLenPtr, outRatePtr);
  const len = readU32(outLenPtr);
  const rate = readU32(outRatePtr);
  if (!pcmPtr || len < 2) { if (pcmPtr) Module._rb_buffer_free(pcmPtr, len); return false; }
  if (readMeta) updateLiveMeta({ codec: ext, sample_rate: rate });

  // Reservoir-primer trim: `expectPcmFrames` is what the segment's own frames
  // should produce; anything beyond it at the head is decoded primer audio
  // (already played as part of the previous segment) — drop it. Computing the
  // drop from the surplus (rather than a fixed primer size) stays correct
  // when the decoder itself skips unprimed frames.
  let pos = 0;
  if (expectPcmFrames > 0) {
    const pcmFrames = len >> 1;
    if (pcmFrames > expectPcmFrames) pos = (pcmFrames - expectPcmFrames) * 2;
  }
  await streamRaw(pcmPtr, len, rate, () => pos, (v) => (pos = v), token, maybeStart);
  Module._rb_buffer_free(pcmPtr, len);
  return true;
}

// ── MP4 / M4A progressive demux ───────────────────────────────────────────
// M4A is a container: the codec config (`esds`) and the sample-offset table
// (`stsz`/`stsc`/`stco`) live in `moov`, the audio in `mdat`. A mid-file byte
// range is undecodable on its own, so the whole-file path downloads all of it
// before the first sample plays. Instead — for *faststart* files (moov before
// mdat) carrying plain AAC — we parse `moov`, then re-frame each `mdat` sample
// as a self-contained ADTS packet and push it through the same progressive
// path MP3/ADTS-AAC use (rb_decode_packet). Playback starts within ~a second
// and memory stays bounded. Non-faststart (moov last), ALAC, or HE-AAC files
// fall back to whole-file decode (which is also seekable).

const MP4_EMPTY = new Uint8Array(0);
const be32 = (b, o) => b[o] * 0x1000000 + (b[o + 1] << 16) + (b[o + 2] << 8) + b[o + 3];
const be64 = (b, o) => be32(b, o) * 0x100000000 + be32(b, o + 4); // < 2^53 for our files
const box4 = (b, o) => String.fromCharCode(b[o], b[o + 1], b[o + 2], b[o + 3]);

/** Iterate boxes in b[start,end) → [{type, start, dataStart, dataEnd}]. */
function mp4Boxes(b, start, end) {
  const out = [];
  let o = start;
  while (o + 8 <= end) {
    let size = be32(b, o); const type = box4(b, o + 4); let hdr = 8;
    if (size === 1) { if (o + 16 > end) break; size = be64(b, o + 8); hdr = 16; }
    else if (size === 0) size = end - o;
    if (size < hdr || o + size > end) break;
    out.push({ type, start: o, dataStart: o + hdr, dataEnd: o + size });
    o += size;
  }
  return out;
}
const mp4Child = (b, box, type, base) =>
  mp4Boxes(b, base != null ? base : box.dataStart, box.dataEnd).find((x) => x.type === type);

/** 7-byte ADTS header (no CRC) wrapping an AAC frame of `frameLen` bytes. */
function mp4AdtsHeader(cfg, frameLen) {
  const len = frameLen + 7;
  return Uint8Array.of(
    0xff,
    0xf1, // MPEG-4, layer 0, protection absent
    (cfg.profile << 6) | (cfg.srIdx << 2) | (cfg.chan >> 2),
    ((cfg.chan & 3) << 6) | (len >> 11),
    (len >> 3) & 0xff,
    ((len & 7) << 5) | 0x1f,
    0xfc,
  );
}

/** esds AudioSpecificConfig → {profile, srIdx, chan} for ADTS, or null unless
 *  it's plain AAC (Main/LC/SSR/LTP) with a table sample rate and defined channels. */
function mp4ParseEsds(b, start, end) {
  let o = start + 4; // version + flags
  const vlen = () => { let l = 0, c; do { c = b[o++]; l = (l << 7) | (c & 0x7f); } while ((c & 0x80) && o < end); return l; };
  if (b[o++] !== 0x03) return null; vlen();     // ES_Descriptor
  o += 2; const flags = b[o++];                 // ES_ID + flags
  if (flags & 0x80) o += 2;
  if (flags & 0x40) o += 1 + b[o];
  if (flags & 0x20) o += 2;
  if (b[o++] !== 0x04) return null; vlen();      // DecoderConfigDescriptor
  if (b[o++] !== 0x40) return null;              // objectTypeIndication: AAC
  o += 1 + 3 + 4 + 4;                            // streamType + bufSize + max/avg bitrate
  if (b[o++] !== 0x05) return null; vlen();       // DecoderSpecificInfo → ASC
  const a0 = b[o], a1 = b[o + 1];
  const objType = (a0 >> 3) & 0x1f;
  const srIdx = ((a0 & 0x07) << 1) | (a1 >> 7);
  const chan = (a1 >> 3) & 0x0f;
  if (objType < 1 || objType > 4) return null;    // HE-AAC/other → whole-file
  if (srIdx > 12) return null;                    // explicit/reserved rate
  if (chan < 1 || chan > 7) return null;          // 0 = program config element
  return { profile: objType - 1, srIdx, chan };
}

/** Flat [{off, size}] sample list from stsz/stsc/stco(co64). */
function mp4BuildSamples(b, stbl) {
  const stsz = mp4Child(b, stbl, 'stsz');
  const stsc = mp4Child(b, stbl, 'stsc');
  const stco = mp4Child(b, stbl, 'stco') || mp4Child(b, stbl, 'co64');
  if (!stsz || !stsc || !stco) return null;
  let o = stsz.dataStart + 4;
  const uni = be32(b, o); o += 4;
  const count = be32(b, o); o += 4;
  const sizes = new Array(count);
  if (uni) sizes.fill(uni); else for (let k = 0; k < count; k++) { sizes[k] = be32(b, o); o += 4; }
  const c64 = stco.type === 'co64';
  let c = stco.dataStart + 4; const nchunks = be32(b, c); c += 4;
  const choff = new Array(nchunks);
  for (let k = 0; k < nchunks; k++) { choff[k] = c64 ? be64(b, c) : be32(b, c); c += c64 ? 8 : 4; }
  let s = stsc.dataStart + 4; const nrun = be32(b, s); s += 4;
  const runs = [];
  for (let k = 0; k < nrun; k++) { runs.push({ first: be32(b, s), spc: be32(b, s + 4) }); s += 12; }
  const samples = [];
  let si = 0;
  for (let ci = 0; ci < nchunks && si < count; ci++) {
    let spc = runs.length ? runs[0].spc : 0;
    for (const r of runs) { if (r.first <= ci + 1) spc = r.spc; else break; }
    let off = choff[ci];
    for (let k = 0; k < spc && si < count; k++) { samples.push({ off, size: sizes[si] }); off += sizes[si]; si++; }
  }
  return samples.length ? samples : null;
}

/** mdhd → duration in ms (0 if absent). */
function mp4DurationMs(b, mdia) {
  const mdhd = mp4Child(b, mdia, 'mdhd'); if (!mdhd) return 0;
  const v = b[mdhd.dataStart]; let o = mdhd.dataStart + 4;
  let ts, du;
  if (v === 1) { o += 16; ts = be32(b, o); du = be64(b, o + 4); }
  else { o += 8; ts = be32(b, o); du = be32(b, o + 4); }
  return ts ? Math.round(du / ts * 1000) : 0;
}

/** Best-effort udta>meta>ilst tags → {title, artist, album}. */
function mp4Tags(b, moov) {
  const udta = mp4Child(b, moov, 'udta'); if (!udta) return {};
  const meta = mp4Child(b, udta, 'meta'); if (!meta) return {};
  // ISO `meta` is a FullBox (children at +4); QuickTime's isn't. Detect by peek.
  let base = meta.dataStart + 4;
  if (!mp4Boxes(b, base, meta.dataEnd).some((x) => x.type === 'ilst' || x.type === 'hdlr')) base = meta.dataStart;
  const ilst = mp4Child(b, meta, 'ilst', base); if (!ilst) return {};
  const KEYS = { '\xa9nam': 'title', '\xa9ART': 'artist', '\xa9alb': 'album' };
  const tags = {};
  for (const it of mp4Boxes(b, ilst.dataStart, ilst.dataEnd)) {
    const key = KEYS[it.type]; if (!key) continue;
    const data = mp4Child(b, it, 'data'); if (!data || data.dataEnd <= data.dataStart + 8) continue;
    let s = ''; for (let o = data.dataStart + 8; o < data.dataEnd; o++) s += String.fromCharCode(b[o]);
    try { tags[key] = decodeURIComponent(escape(s)); } catch (_) { tags[key] = s; }
  }
  return tags;
}

/**
 * Incremental MP4→ADTS demuxer used as the `demux` for runSegmentLoop.
 * `push(chunk)` returns ADTS-wrapped AAC bytes as samples become available.
 * `mode` resolves during the probe phase: 'aac' (faststart AAC → stream) or
 * 'fallback' (moov-last / ALAC / HE-AAC → caller whole-file decodes).
 */
class Mp4AacDemux {
  constructor(keepRaw = true) {
    this.mode = 'probe';           // 'probe' | 'aac' | 'fallback'
    this.buf = MP4_EMPTY;          // unconsumed bytes
    this.filePos = 0;              // absolute file offset of buf[0]
    this.skip = 0;                 // bytes still to discard (box skip / mdat gap)
    this.keepRaw = keepRaw;        // accumulate raw bytes for a whole-file fallback
    this.raw = MP4_EMPTY;          // all bytes seen (for fallback); freed once streaming
    this.cfg = null;               // {profile, srIdx, chan}
    this.samples = null; this.si = 0;
    this.durationMs = 0; this.tags = {};
  }

  push(chunk) {
    if (chunk && chunk.length) {
      this.buf = concatBytes(this.buf, chunk);
      if (this.keepRaw && this.mode !== 'aac') this.raw = concatBytes(this.raw, chunk);
    }
    const out = [];
    this._pump(out);
    if (out.length === 0) return MP4_EMPTY;
    let total = 0; for (const p of out) total += p.length;
    const r = new Uint8Array(total); let o = 0;
    for (const p of out) { r.set(p, o); o += p.length; }
    return r;
  }

  _pump(out) {
    for (;;) {
      if (this.skip > 0) {
        const n = Math.min(this.skip, this.buf.length);
        this.buf = this.buf.subarray(n); this.filePos += n; this.skip -= n;
        if (this.skip > 0) return; // need more bytes to finish skipping
      }
      if (this.mode === 'fallback') return;
      if (this.mode === 'aac') { this._streamSamples(out); return; }
      // probe: walk top-level boxes until moov (→ stream) or mdat (→ fallback).
      const b = this.buf;
      if (b.length < 8) return;
      let size = be32(b, 0); const type = box4(b, 4);
      if (size === 1) { if (b.length < 16) return; size = be64(b, 8); }
      else if (size === 0) size = Infinity;
      if (type === 'moov') {
        if (size === Infinity) { this.mode = 'fallback'; return; }
        if (b.length < size) return;                 // wait for the whole moov
        if (!this._parseMoov(b.subarray(0, size))) { this.mode = 'fallback'; return; }
        this.filePos += size; this.buf = this.buf.subarray(size);
        this.mode = 'aac'; this.raw = MP4_EMPTY;      // progressive from here — free raw
        continue;                                     // stream any already-buffered mdat
      }
      if (type === 'mdat' || size === Infinity) { this.mode = 'fallback'; return; }
      this.skip = size;                               // ftyp/free/etc. — skip whole box
    }
  }

  _parseMoov(box) {
    const moov = mp4Boxes(box, 0, box.length).find((x) => x.type === 'moov');
    if (!moov) return false;
    for (const trak of mp4Boxes(box, moov.dataStart, moov.dataEnd).filter((x) => x.type === 'trak')) {
      const mdia = mp4Child(box, trak, 'mdia'); if (!mdia) continue;
      const hdlr = mp4Child(box, mdia, 'hdlr');
      if (hdlr && box4(box, hdlr.dataStart + 8) !== 'soun') continue; // skip non-audio tracks
      const minf = mp4Child(box, mdia, 'minf'); if (!minf) continue;
      const stbl = mp4Child(box, minf, 'stbl'); if (!stbl) continue;
      const stsd = mp4Child(box, stbl, 'stsd'); if (!stsd) continue;
      const mp4a = mp4Boxes(box, stsd.dataStart + 8, stsd.dataEnd).find((x) => x.type === 'mp4a');
      if (!mp4a) return false;                        // ALAC/etc. → whole-file
      const esds = mp4Boxes(box, mp4a.dataStart + 28, mp4a.dataEnd).find((x) => x.type === 'esds');
      if (!esds) return false;
      const cfg = mp4ParseEsds(box, esds.dataStart, esds.dataEnd);
      if (!cfg) return false;
      const samples = mp4BuildSamples(box, stbl);
      if (!samples) return false;
      this.cfg = cfg; this.samples = samples;
      this.durationMs = mp4DurationMs(box, mdia);
      this.tags = mp4Tags(box, moov);
      return true;
    }
    return false;
  }

  _streamSamples(out) {
    const s = this.samples;
    while (this.si < s.length) {
      const smp = s[this.si];
      const start = smp.off - this.filePos;
      if (start < 0) { this.si++; continue; }         // already passed (out-of-order guard)
      if (start + smp.size > this.buf.length) break;  // sample not fully buffered yet
      out.push(mp4AdtsHeader(this.cfg, smp.size));
      out.push(this.buf.subarray(start, start + smp.size).slice());
      this.si++;
    }
    // Drop consumed bytes (and any inter-sample gap) up to the next sample.
    const nextOff = this.si < s.length ? s[this.si].off : this.filePos + this.buf.length;
    const drop = nextOff - this.filePos;
    if (drop > 0) {
      const d = Math.min(drop, this.buf.length);
      this.buf = this.buf.subarray(d); this.filePos += d;
      if (drop > d) this.skip = drop - d;             // gap extends beyond what's buffered
    }
  }
}

/**
 * Play an M4A/MP4. Probe the container head: faststart AAC streams
 * progressively (re-framed to ADTS through runSegmentLoop); anything else
 * (moov-last, ALAC, HE-AAC) drains fully and whole-file decodes.
 */
async function playMp4(reader, head, done, url, i, token, autoplay) {
  const demux = new Mp4AacDemux();
  let adts = demux.push(head);
  while (demux.mode === 'probe' && !done) {
    let r;
    try { r = await reader.read(); }
    catch (err) {
      postMessage({ type: 'error', message: `fetch failed: ${url} (${err})`, index: i });
      if (token === loadToken) skipAfterError(i);
      return;
    }
    if (token !== loadToken) { reader.cancel().catch(() => {}); return; }
    if (r.value && r.value.length) adts = concatBytes(adts, demux.push(r.value));
    done = r.done;
  }
  if (token !== loadToken) { reader.cancel().catch(() => {}); return; }

  if (demux.mode !== 'aac') {
    // Non-faststart / ALAC / HE-AAC — buffer the rest, whole-file decode (seekable).
    const bytes = await drainToBytes(reader, demux.raw, done, token);
    if (!bytes || token !== loadToken) return;
    return playDecodedBytes(bytes, 'm4a', url, i, token, 0, autoplay);
  }

  // Faststart AAC — stream the container as an ADTS packet flow.
  live = false; playing = true; userPaused = false;
  Module._rb_dsp_flush(dsp); curInRate = 0;
  curMeta = { codec: 'aac', duration_ms: demux.durationMs || 0, ...demux.tags };
  postMessage({ type: 'track', index: i, url, live: false, metadata: curMeta });
  emitStatus();
  void prefetchNext();
  return runSegmentLoop(reader, adts, done, 'aac', token, url, i, demux);
}

/** A ReadableStream-reader-shaped adapter over an in-memory buffer so the
 *  progressive segment loop can consume prefetched bytes exactly like a
 *  network body. Chunked to keep `pending` (and its re-slicing) bounded. */
function memoryReader(bytes, chunk = 65536) {
  let o = 0;
  return {
    read: async () => {
      if (o >= bytes.length) return { value: undefined, done: true };
      const end = Math.min(o + chunk, bytes.length);
      const v = bytes.subarray(o, end);
      o = end;
      return { value: v, done: false };
    },
    cancel: async () => {},
  };
}

/** Convert a fully-downloaded M4A/MP4 to an ADTS stream, feeding the demux in
 *  chunks to bound peak memory. Returns {mode:'aac', adts, durationMs, tags}
 *  for faststart AAC, or {mode:'fallback'} for ALAC / HE-AAC / moov-last. */
function demuxMp4Buffer(buf) {
  const demux = new Mp4AacDemux(false);
  const parts = [];
  for (let o = 0; o < buf.length; o += 65536) {
    const a = demux.push(buf.subarray(o, Math.min(o + 65536, buf.length)));
    if (a.length) parts.push(a);
    if (demux.mode === 'fallback') break;
  }
  if (demux.mode !== 'aac') return { mode: demux.mode };
  let total = 0; for (const p of parts) total += p.length;
  const adts = new Uint8Array(total); let off = 0;
  for (const p of parts) { adts.set(p, off); off += p.length; }
  return { mode: 'aac', adts, durationMs: demux.durationMs, tags: demux.tags };
}

/** Stream an in-memory ADTS buffer through the progressive path — a gapless
 *  transition into a pre-demuxed faststart-AAC prefetch. */
async function playAdtsBuffer(adts, url, i, token, durationMs, tags) {
  live = false; playing = true; userPaused = false;
  Module._rb_dsp_flush(dsp); curInRate = 0;
  curMeta = { codec: 'aac', duration_ms: durationMs || 0, ...(tags || {}) };
  postMessage({ type: 'track', index: i, url, live: false, metadata: curMeta });
  emitStatus();
  void prefetchNext();
  return runSegmentLoop(memoryReader(adts), MP4_EMPTY, false, 'aac', token, url, i, null);
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
  if (!rawPtr && !live && !streaming) { startTrack(index >= 0 ? index : 0, 0, true); return; }
  playing = true; userPaused = false; setWorkletPaused(false);
  emitStatus();
}
function pause() { userPaused = true; setWorkletPaused(true); emitStatus(); }
function stop() {
  loadToken++;
  playing = false; userPaused = false; live = false; streaming = false;
  cancelPrefetch();
  freeRaw();
  xfade = null;
  dropHold();
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
  xfade = null;
  dropHold();
  flushWorklet();
  Module._rb_dsp_flush(dsp);
  curInRate = 0;
  finitePos = Math.min(rawLen, Math.floor(ms * rawRate / 1000) * 2);
}

function setQueue(urls, autoplay) {
  queue = Array.isArray(urls) ? urls.slice() : [];
  index = -1;
  cancelPrefetch(); prefetch = null; // queue replaced — any prefetch is stale
  emitQueue();
  if (queue.length && autoplay) startTrack(0, 0, true);
  else stop();
}
function enqueue(url) {
  queue.push(url);
  emitQueue();
  if (playing && !rawPtr && !live && !streaming) startTrack(index >= 0 ? index : 0, 0, true);
  else if (playing) void prefetchNext(); // may now be the next-up track
}
function clearQueue() { queue = []; index = -1; lastInsertPos = -1; cancelPrefetch(); prefetch = null; stop(); emitQueue(); }

// Chain anchor for the 'insert' mode: like Rockbox, consecutive Inserts land
// after the previously inserted batch, not each directly after the current
// track. Reset whenever the playing track changes.
let lastInsertPos = -1;

/** Insert `urls` with a Rockbox insertion mode (see apps/playlist.c):
 *  prepend | insert | insert-next (play next) | insert-last (play last) |
 *  insert-shuffled | insert-last-shuffled | replace | index. */
function insertTracks(urls, position, atIndex) {
  const list = (Array.isArray(urls) ? urls : [urls]).filter(Boolean).map(String);
  if (list.length === 0) return;

  if (position === 'replace') { setQueue(list, false); return; }

  const shuffleArr = (a) => {
    for (let i = a.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      [a[i], a[j]] = [a[j], a[i]];
    }
    return a;
  };

  let pos;
  switch (position) {
    case 'prepend':       pos = 0; break;
    case 'insert':        // chain after the previous Insert batch
      pos = lastInsertPos > index && lastInsertPos <= queue.length
        ? lastInsertPos : index + 1;
      break;
    case 'insert-next':   pos = index + 1; break;
    case 'insert-shuffled': {                // random slot after current
      const lo = index + 1;
      pos = lo + Math.floor(Math.random() * (queue.length - lo + 1));
      break;
    }
    case 'insert-last-shuffled': shuffleArr(list); pos = queue.length; break;
    case 'index':         pos = Math.max(0, Math.min(queue.length, atIndex)); break;
    case 'insert-last':
    default:              pos = queue.length; break;
  }

  queue.splice(pos, 0, ...list);
  if (index >= 0 && pos <= index) index += list.length; // renumber current
  lastInsertPos = pos + list.length;
  emitQueue();
  if (playing && !rawPtr && !live && !streaming) startTrack(index >= 0 ? index : 0, 0, true);
  // The next-up track may have changed (e.g. "play next") — pre-buffer it now
  // so its transition is gapless too, aborting any stale in-flight prefetch.
  else if (playing) void prefetchNext();
}

/** Remove queue entry `i` (Rockbox semantics): removing a track before the
 *  current one keeps it playing; removing the current track hard-cuts to the
 *  one that slides into its place; removing the last remaining track stops. */
function removeAt(i) {
  if (i < 0 || i >= queue.length) return;
  const wasCurrent = i === index;
  queue.splice(i, 1);
  if (queue.length === 0) { index = -1; stop(); emitQueue(); return; }
  if (i < index) {
    index--; // current track unaffected — just renumber
  } else if (wasCurrent) {
    if (i < queue.length) {
      // hard-cut to the track that slides into the removed slot
      emitQueue();
      startTrack(i, 0, playing && !userPaused);
      return;
    }
    // removed the tail while it was playing — nothing slides in: stop,
    // but keep the rest of the queue
    index = queue.length - 1;
    loadToken++;
    playing = false; userPaused = false; live = false; streaming = false;
    freeRaw();
    xfade = null;
    dropHold();
    flushWorklet();
    curMeta = null;
  }
  emitQueue();
}

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
  if (!playing && !live && !rawPtr && !streaming) return 'stopped';
  return userPaused ? 'paused' : 'playing';
}
function emitStatus() {
  postMessage({ type: 'status', state: stateName(), index, queue_len: queue.length, shuffle, repeat });
}
function emitQueue() {
  postMessage({ type: 'queue', urls: queue, index });
  // The queue length is part of the status snapshot too — keep it live so a
  // "+ Queue" click updates the "n / m" display immediately.
  emitStatus();
}
function emitProgress() {
  postMessage({
    type: 'progress',
    state: stateName(),
    index,
    live,
    elapsed_ms: seekBaseMs + Math.round(Math.max(0, wlConsumed - trackBase) * 1000 / sampleRate),
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
  writeU8(bytes, pktPtr);
  return pktPtr;
}
