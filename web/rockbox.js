/**
 * RockboxPlayer — browser-side player built on the Rockbox decode + DSP core.
 *
 *   const player = new RockboxPlayer();
 *   await player.init();                 // call from a user gesture (click)
 *   player.setQueue(['song.flac'], true);
 *
 * Architecture (see web/README.md):
 *
 *   main thread (this file)   UI facade, AudioContext + AudioWorklet, events
 *        │  commands ▼               ▲ events
 *   decoder Worker            rockbox-core.wasm: rockbox-codecs + rockbox-dsp
 *        │  PCM ▼ (shared ring)      + rockbox-metadata; owns queue/transport
 *   AudioWorklet              plays the ring → speakers
 *
 * The WASM module (rockbox-codecs) decodes on a pthread and blocks on a
 * Condvar, which is illegal on the main thread — so all decode/DSP work lives
 * in the Worker and only PCM crosses into the audio thread via a lock-free
 * SharedArrayBuffer ring. That needs COOP/COEP headers (crossOriginIsolated).
 */

const EQ_BAND_CUTOFFS = [60, 200, 500, 1000, 2000, 4000, 7000, 10000, 14000, 20000];

export class RockboxPlayer {
  constructor(opts = {}) {
    this._coreUrl    = opts.coreUrl    ?? './rockbox-core.js';
    this._workletUrl = opts.workletUrl ?? new URL('./rockbox-audio-worklet.js', import.meta.url).href;
    this._workerUrl  = opts.workerUrl  ?? new URL('./rockbox-decoder-worker.js', import.meta.url).href;
    this._ringSeconds = opts.ringSeconds ?? 6;

    this._ctx = null;
    this._node = null;
    this._worker = null;
    this._ready = false;
    this._listeners = {};

    // Latest snapshot pushed by the Worker (for synchronous UI reads).
    this.state = { state: 'stopped', index: -1, queue_len: 0, shuffle: false, repeat: 0 };
    this.progress = { elapsed_ms: 0, duration_ms: 0 };
    this.metadata = null;
    this.queue = [];
  }

  /** Boot the audio graph + decoder Worker. Resolves when playback-ready. */
  async init() {
    if (this._ready) return;
    if (typeof SharedArrayBuffer === 'undefined' || !self.crossOriginIsolated) {
      throw new Error(
        'crossOriginIsolated is false — serve with COOP/COEP headers ' +
        '(Cross-Origin-Opener-Policy: same-origin, ' +
        'Cross-Origin-Embedder-Policy: require-corp). See scripts/wasm-dev-server.mjs.');
    }

    this._ctx = new AudioContext();
    await this._ctx.resume();
    await this._ctx.audioWorklet.addModule(this._workletUrl);

    const rate       = this._ctx.sampleRate;
    const ringFrames = Math.ceil(this._ringSeconds * rate);
    this._controlSab = new SharedArrayBuffer(8 * Int32Array.BYTES_PER_ELEMENT);
    this._audioSab   = new SharedArrayBuffer(ringFrames * 2 * Int16Array.BYTES_PER_ELEMENT);

    // AudioWorklet (consumer)
    this._node = new AudioWorkletNode(this._ctx, 'rockbox-processor', {
      numberOfInputs: 0,
      numberOfOutputs: 1,
      outputChannelCount: [2],
      processorOptions: { controlSab: this._controlSab, audioSab: this._audioSab, ringFrames },
    });
    // Volume isn't part of the rockbox-dsp pipeline, so it lives here as a
    // Web Audio GainNode between the worklet and the speakers.
    this._gain = this._ctx.createGain();
    this._gain.gain.value = this._volume ?? 1;
    this._node.connect(this._gain).connect(this._ctx.destination);
    await this._once(this._node.port, 'ready');
    this._node.port.start?.();

    // Decoder Worker (producer)
    this._worker = new Worker(this._workerUrl);
    this._worker.onmessage = (e) => this._onWorker(e.data);
    await new Promise((res) => {
      const onReady = (e) => {
        if (e.data?.type === 'ready') { this._worker.removeEventListener('message', onReady); res(); }
      };
      this._worker.addEventListener('message', onReady);
    });
    this._post({ cmd: 'init', controlSab: this._controlSab, audioSab: this._audioSab, ringFrames, sampleRate: rate });

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
  setRepeat(mode){ this._post({ cmd: 'repeat', mode: mode | 0 }); }

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
  setChannelMode(mode)            { this._save('channelMode', mode | 0); this._dsp('set_channel_config', [mode | 0]); }
  setStereoWidth(pct)             { this._save('stereoWidth', pct | 0); this._dsp('set_stereo_width', [pct | 0]); }
  setCompressor(threshold, makeup, ratio, knee, release, attack) {
    this._dsp('set_compressor', [threshold | 0, makeup | 0, ratio | 0, knee | 0, release | 0, attack | 0]);
  }
  /** mode: 0 track, 1 album, 2 shuffle, 3 off. */
  setReplaygain(mode, noclip, preampDb) {
    this._save('rgMode', mode | 0); this._save('rgNoclip', !!noclip); this._save('rgPreamp', +preampDb);
    this._dsp('set_replaygain', [mode | 0, noclip ? 1 : 0, +preampDb]);
  }

  static get EQ_BAND_CUTOFFS() { return EQ_BAND_CUTOFFS.slice(); }

  // ── Events ────────────────────────────────────────────────────────────────
  /** on('status'|'track'|'progress'|'queue'|'error', cb) */
  on(event, cb)  { (this._listeners[event] ??= new Set()).add(cb); return this; }
  off(event, cb) { this._listeners[event]?.delete(cb); return this; }
  _emit(event, data) { this._listeners[event]?.forEach((cb) => { try { cb(data); } catch (_) {} }); }

  _onWorker(msg) {
    switch (msg.type) {
      case 'status':   this.state = msg; this._emit('status', msg); break;
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
