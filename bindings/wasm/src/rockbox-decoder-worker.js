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
    case 'skipTo':     return beginManual(msg.index);
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
  if (xfadeCfg.mode === 'off') return 0;
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
    return runSegmentLoop(reader, head, done, ext, token, url, i, null);
  }

  // Buffer the rest, then whole-file decode (seekable).
  const bytes = await drainToBytes(reader, head, done, token);
  if (!bytes || token !== loadToken) return;

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

/**
 * Progressive segment decoder shared by live streams and streamed-finite MP3/AAC.
 * Decodes `pending` (any pre-read head) then reads `reader` to the end, decoding
 * each ~32 KB segment with rb_decode_packet and streaming the PCM. Starts
 * playback once ~LIVE_PREBUFFER_SEC is queued. `demux` (or null) strips ICY.
 */
async function runSegmentLoop(reader, pending, done, ext, token, url, i, demux) {
  streaming = true;
  // Prebuffer target must stay below the worklet high-water mark or the
  // backpressure gate would stop feeding before playback ever starts.
  const prebuf = Math.min(Math.round(LIVE_PREBUFFER_SEC * sampleRate),
                          Math.max(4096, highWater() - 8192));
  let gotMeta = false;
  let started = false;
  const maybeStart = () => {
    if (!started && !userPaused && wlQueued >= prebuf) { started = true; setWorkletPaused(false); }
  };
  const drainPending = async () => {
    while (token === loadToken && pending.length >= LIVE_SEGMENT) {
      const seg = pending.slice(0, LIVE_SEGMENT);
      pending = pending.slice(LIVE_SEGMENT);
      // Only mark meta as read once a segment actually decodes (the first
      // segment of an MP3 is often just ID3/album-art bytes and fails).
      if (await decodeSegment(seg, ext, token, !gotMeta, maybeStart)) gotMeta = true;
    }
  };
  try {
    await drainPending();
    while (token === loadToken && !done) {
      const r = await reader.read();
      if (token !== loadToken) { await reader.cancel().catch(() => {}); return; }
      if (r.value && r.value.length) {
        const audio = demux ? demux.push(r.value) : r.value;
        if (audio.length) pending = concatBytes(pending, audio);
      }
      done = r.done;
      await drainPending();
    }
    if (token === loadToken && pending.length) await decodeSegment(pending, ext, token, !gotMeta, maybeStart);
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
async function decodeSegment(bytes, ext, token, readMeta, maybeStart) {
  const dataPtr = copyPacket(bytes);
  const pcmPtr = Module._rb_decode_packet(dataPtr, bytes.length, allocPath(ext), outLenPtr, outRatePtr);
  const len = readU32(outLenPtr);
  const rate = readU32(outRatePtr);
  if (!pcmPtr || len < 2) { if (pcmPtr) Module._rb_buffer_free(pcmPtr, len); return false; }
  if (readMeta) updateLiveMeta({ codec: ext, sample_rate: rate });

  let pos = 0;
  await streamRaw(pcmPtr, len, rate, () => pos, (v) => (pos = v), token, maybeStart);
  Module._rb_buffer_free(pcmPtr, len);
  return true;
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
  emitQueue();
  if (queue.length && autoplay) startTrack(0, 0, true);
  else stop();
}
function enqueue(url) {
  queue.push(url);
  emitQueue();
  if (playing && !rawPtr && !live && !streaming) startTrack(index >= 0 ? index : 0, 0, true);
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
  if (!playing && !live && !rawPtr && !streaming) return 'stopped';
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
