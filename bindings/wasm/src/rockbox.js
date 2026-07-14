/**
 * RockboxPlayer — browser music player on the Rockbox decode + DSP core (WASM).
 *
 *   const player = new RockboxPlayer();
 *   await player.init();                 // call from a user gesture (click)
 *   player.setQueue(['song.flac'], true);
 *
 * Architecture (single-threaded build — no SharedArrayBuffer, no COOP/COEP):
 *
 *   main thread (this file)   facade · AudioContext · GainNode(volume) · events
 *        │  commands ▼               ▲ events
 *   decoder Worker            rockbox-core.wasm (codecs + DSP + metadata);
 *        │  PCM ▼ (MessagePort)      decodes synchronously, owns queue/transport
 *   AudioWorklet              queues + plays the PCM → speakers
 */

const EQ_BAND_CUTOFFS = [60, 200, 500, 1000, 2000, 4000, 7000, 10000, 14000, 20000];

// ── Human-readable settings enums (translated to the DSP's ints internally) ──
export const RepeatMode = Object.freeze({ Off: 'off', One: 'one', All: 'all' });
export const ReplayGainMode = Object.freeze({
  Off: 'off', Track: 'track', Album: 'album', Shuffle: 'shuffle',
});
export const ChannelMode = Object.freeze({
  Stereo: 'stereo', Mono: 'mono', Custom: 'custom',
  MonoLeft: 'mono-left', MonoRight: 'mono-right', Karaoke: 'karaoke', Swap: 'swap',
});

const REPEAT_NUM = { off: 0, one: 1, all: 2 };
const REPEAT_STR = ['off', 'one', 'all'];
const RG_NUM = { track: 0, album: 1, shuffle: 2, off: 3 };
const CHAN_NUM = {
  stereo: 0, mono: 1, custom: 2, 'mono-left': 3, 'mono-right': 4, karaoke: 5, swap: 6,
};
/** Accept either an enum string or a raw number; fall back to `dflt`. */
const toNum = (v, map, dflt = 0) => (typeof v === 'number' ? v : (map[v] ?? dflt));

export class RockboxPlayer {
  constructor(opts = {}) {
    // Resolve the three sibling assets. `baseUrl` is the easy path — point it
    // at wherever you serve the package's dist/ files; each URL can also be
    // overridden individually, else it defaults to a sibling of this module.
    const rel = (name) => opts.baseUrl
      ? `${String(opts.baseUrl).replace(/\/$/, '')}/${name}`
      : new URL(`./${name}`, import.meta.url).href;
    this._coreUrl    = opts.coreUrl    ?? rel('rockbox-core.js');
    this._workletUrl = opts.workletUrl ?? rel('rockbox-audio-worklet.js');
    this._workerUrl  = opts.workerUrl  ?? rel('rockbox-decoder-worker.js');

    this._ctx = null;
    this._node = null;
    this._worker = null;
    this._ready = false;
    this._listeners = {};

    // Latest snapshot pushed by the Worker (for synchronous UI reads).
    this.state = { state: 'stopped', index: -1, queue_len: 0, shuffle: false, repeat: 'off' };
    this.progress = { elapsed_ms: 0, duration_ms: 0 };
    this.metadata = null;
    this.queue = [];
  }

  /** Boot the audio graph + decoder Worker. Resolves when playback-ready. */
  async init() {
    if (this._ready) return;

    this._ctx = new AudioContext();
    await this._ctx.resume();
    await this._ctx.audioWorklet.addModule(this._workletUrl);

    this._node = new AudioWorkletNode(this._ctx, 'rockbox-processor', {
      numberOfInputs: 0,
      numberOfOutputs: 1,
      outputChannelCount: [2],
    });
    // Volume isn't a rockbox-dsp stage, so it lives here as a Web Audio gain.
    this._gain = this._ctx.createGain();
    this._gain.gain.value = this._volume ?? 1;
    this._node.connect(this._gain).connect(this._ctx.destination);
    await this._once(this._node.port, 'ready');
    this._node.port.start?.();

    // Decoder Worker. Tell it where the (self-contained) core module lives.
    const workerUrl = new URL(this._workerUrl, import.meta.url);
    workerUrl.searchParams.set('core', new URL(this._coreUrl, import.meta.url).href);
    this._worker = new Worker(workerUrl.href);
    this._worker.onmessage = (e) => this._onWorker(e.data);
    await new Promise((res) => {
      const onReady = (e) => {
        if (e.data?.type === 'ready') { this._worker.removeEventListener('message', onReady); res(); }
      };
      this._worker.addEventListener('message', onReady);
    });

    // Direct PCM channel: Worker → AudioWorklet (no SharedArrayBuffer).
    const ch = new MessageChannel();
    this._node.port.postMessage({ type: 'pcmport', port: ch.port1 }, [ch.port1]);
    this._worker.postMessage({ cmd: 'pcmport', port: ch.port2 }, [ch.port2]);
    this._worker.postMessage({ cmd: 'init', sampleRate: this._ctx.sampleRate });

    this._ready = true;
    this._restoreSettings();
  }

  get audioContext() { return this._ctx; }
  get ready() { return this._ready; }

  // ── Transport ───────────────────────────────────────────────────────────
  setQueue(urls, autoplay = false) { this.queue = urls.slice(); this._post({ cmd: 'setQueue', urls, autoplay }); }
  enqueue(url)   { this.queue.push(url); this._post({ cmd: 'enqueue', url }); }
  clearQueue()   { this.queue = []; this._post({ cmd: 'clearQueue' }); }
  play()         { this._ctx?.resume(); this._post({ cmd: 'play' }); }
  pause()        { this._post({ cmd: 'pause' }); }
  toggle()       { this._ctx?.resume(); this._post({ cmd: 'toggle' }); }
  stop()         { this._post({ cmd: 'stop' }); }
  next()         { this._post({ cmd: 'next' }); }
  prev()         { this._post({ cmd: 'prev' }); }
  skipTo(index)  { this._ctx?.resume(); this._post({ cmd: 'skipTo', index }); }
  seek(ms)       { this._post({ cmd: 'seek', ms: ms | 0 }); }
  setShuffle(on) { this._post({ cmd: 'shuffle', enabled: !!on }); }
  /** RepeatMode.Off | .One | .All (or 0 | 1 | 2). */
  setRepeat(mode){ this._post({ cmd: 'repeat', mode: toNum(mode, REPEAT_NUM) }); }

  /** Output volume, 0.0..=1.0 (Web Audio GainNode; not a rockbox-dsp stage). */
  setVolume(v) {
    this._volume = Math.max(0, Math.min(1, +v));
    if (this._gain) this._gain.gain.value = this._volume;
  }
  get volume() { return this._volume ?? 1; }

  // ── DSP / EQ (forwarded to rockbox-dsp in the Worker) ─────────────────────
  setEqEnabled(on)                { this._save('eqEnabled', !!on); this._dsp('eq_enable', [on ? 1 : 0]); }
  setEqPrecut(db)                 { this._save('eqPrecut', db); this._dsp('set_eq_precut', [+db]); }
  setEqBand(band, cutoffHz, q, gainDb) {
    this._saveBand(band, cutoffHz, q, gainDb);
    this._dsp('set_eq_band', [band | 0, cutoffHz | 0, +q, +gainDb]);
  }
  setTone(bassDb, trebleDb)       { this._save('bass', bassDb); this._save('treble', trebleDb); this._dsp('set_tone', [bassDb | 0, trebleDb | 0]); }
  setToneCutoffs(bassHz, trebleHz){ this._dsp('set_tone_cutoffs', [bassHz | 0, trebleHz | 0]); }
  setSurround(delayMs, balance, fx1, fx2) { this._dsp('set_surround', [delayMs | 0, balance | 0, fx1 | 0, fx2 | 0]); }
  /** ChannelMode.Stereo | .Mono | … (or the raw 0–6 index). */
  setChannelMode(mode)            { const n = toNum(mode, CHAN_NUM); this._save('channelMode', n); this._dsp('set_channel_config', [n]); }
  setStereoWidth(pct)             { this._save('stereoWidth', pct | 0); this._dsp('set_stereo_width', [pct | 0]); }
  setCompressor(threshold, makeup, ratio, knee, release, attack) {
    this._dsp('set_compressor', [threshold | 0, makeup | 0, ratio | 0, knee | 0, release | 0, attack | 0]);
  }
  /** ReplayGainMode.Off | .Track | .Album | .Shuffle (or the raw int). */
  setReplaygain(mode, noclip, preampDb) {
    const n = toNum(mode, RG_NUM, RG_NUM.off);
    this._save('rgMode', n); this._save('rgNoclip', !!noclip); this._save('rgPreamp', +preampDb);
    this._dsp('set_replaygain', [n, noclip ? 1 : 0, +preampDb]);
  }

  static get EQ_BAND_CUTOFFS() { return EQ_BAND_CUTOFFS.slice(); }

  // ── Events ────────────────────────────────────────────────────────────────
  /** on('status'|'track'|'progress'|'queue'|'error', cb) */
  on(event, cb)  { (this._listeners[event] ??= new Set()).add(cb); return this; }
  off(event, cb) { this._listeners[event]?.delete(cb); return this; }
  _emit(event, data) { this._listeners[event]?.forEach((cb) => { try { cb(data); } catch (_) {} }); }

  _onWorker(msg) {
    switch (msg.type) {
      case 'status': {
        // Present repeat as an enum string to the app.
        const s = { ...msg, repeat: REPEAT_STR[msg.repeat] ?? 'off' };
        this.state = s; this._emit('status', s); break;
      }
      case 'track':    this.metadata = msg.metadata; this._emit('track', msg); break;
      case 'progress': this.progress = msg; this.metadata = msg.metadata ?? this.metadata; this._emit('progress', msg); break;
      case 'queue':    this.queue = msg.urls; this._emit('queue', msg); break;
      case 'error':    console.warn('[Rockbox]', msg.message); this._emit('error', msg); break;
    }
  }

  // ── Settings persistence (localStorage) ─────────────────────────────────────
  _save(key, val) {
    this._settings ??= this._loadSettings();
    this._settings[key] = val;
    try { localStorage.setItem('rockbox:settings', JSON.stringify(this._settings)); } catch (_) {}
  }
  _saveBand(band, cutoff, q, gain) {
    this._settings ??= this._loadSettings();
    (this._settings.bands ??= [])[band] = { cutoff, q, gain };
    try { localStorage.setItem('rockbox:settings', JSON.stringify(this._settings)); } catch (_) {}
  }
  _loadSettings() {
    try { return JSON.parse(localStorage.getItem('rockbox:settings')) || {}; } catch (_) { return {}; }
  }
  getSettings() { return this._settings ??= this._loadSettings(); }
  _restoreSettings() {
    const s = this.getSettings();
    if (s.eqEnabled != null) this._dsp('eq_enable', [s.eqEnabled ? 1 : 0]);
    if (s.eqPrecut != null)  this._dsp('set_eq_precut', [+s.eqPrecut]);
    (s.bands || []).forEach((b, i) => { if (b) this._dsp('set_eq_band', [i, b.cutoff | 0, +b.q, +b.gain]); });
    if (s.bass != null || s.treble != null) this._dsp('set_tone', [s.bass | 0, s.treble | 0]);
    if (s.channelMode != null) this._dsp('set_channel_config', [s.channelMode | 0]);
    if (s.stereoWidth != null) this._dsp('set_stereo_width', [s.stereoWidth | 0]);
    if (s.rgMode != null) this._dsp('set_replaygain', [s.rgMode | 0, s.rgNoclip ? 1 : 0, +s.rgPreamp || 0]);
  }

  // ── Internal ────────────────────────────────────────────────────────────────
  _dsp(name, args) { this._post({ cmd: 'dsp', name, args }); }
  _post(msg) { this._worker?.postMessage(msg); }
  _once(target, type) {
    return new Promise((res) => {
      const h = (e) => { if (e.data?.type === type) { target.removeEventListener('message', h); res(e.data); } };
      target.addEventListener('message', h);
      target.start?.();
    });
  }
}

export default RockboxPlayer;
