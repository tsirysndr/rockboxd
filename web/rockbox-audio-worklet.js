/**
 * Rockbox AudioWorkletProcessor.
 *
 * Reads interleaved-stereo S16LE PCM frames from a lock-free ring buffer that
 * the decoder Worker (rockbox-decoder-worker.js) fills. Both sides share two
 * SharedArrayBuffers:
 *
 *   controlSab — Int32Array, atomic indices + flags (see CTRL_* below)
 *   audioSab   — Int16Array, `ringFrames` × 2 samples (L,R interleaved)
 *
 * The processor only ever *reads* the audio ring and advances CTRL_READ; the
 * Worker only ever writes and advances CTRL_WRITE. Underruns emit silence.
 */

// Keep these indices in sync with rockbox-decoder-worker.js.
const CTRL_WRITE  = 0; // next frame the Worker will write   (Worker writes)
const CTRL_READ   = 1; // next frame the processor will read (processor writes)
const CTRL_PAUSED = 2; // 1 = output silence, don't consume  (Worker writes)
const CTRL_PLAYED = 3; // frames output for the current track (processor writes)
const CTRL_GEN    = 4; // bumped by the Worker on flush/seek  (Worker writes)

class RockboxProcessor extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const p = options.processorOptions;
    this._ctrl       = new Int32Array(p.controlSab);
    this._ring       = new Int16Array(p.audioSab);
    this._ringFrames = p.ringFrames;
    this._gen        = Atomics.load(this._ctrl, CTRL_GEN);
    this.port.postMessage({ type: 'ready' });
  }

  process(_inputs, outputs) {
    const left  = outputs[0][0];
    const right = outputs[0][1] || left;
    const n     = left.length; // 128 frames per Web Audio render quantum

    // A flush/seek in the Worker bumps the generation; it has already reset
    // the read/write indices, so we just re-sync our cached copy.
    this._gen = Atomics.load(this._ctrl, CTRL_GEN);

    if (Atomics.load(this._ctrl, CTRL_PAUSED)) {
      left.fill(0);
      right.fill(0);
      return true;
    }

    let played = 0;
    for (let i = 0; i < n; i++) {
      const ri = Atomics.load(this._ctrl, CTRL_READ);
      const wi = Atomics.load(this._ctrl, CTRL_WRITE);
      if (ri === wi) {
        left[i]  = 0; // underrun / end of buffered audio
        right[i] = 0;
      } else {
        const pos = ri * 2;
        left[i]  = this._ring[pos]     / 32768;
        right[i] = this._ring[pos + 1] / 32768;
        Atomics.store(this._ctrl, CTRL_READ, (ri + 1) % this._ringFrames);
        played++;
      }
    }
    if (played) Atomics.add(this._ctrl, CTRL_PLAYED, played);
    return true;
  }
}

registerProcessor('rockbox-processor', RockboxProcessor);
