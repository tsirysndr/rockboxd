/**
 * Rockbox AudioWorkletProcessor (single-threaded build).
 *
 * The decoder Worker posts interleaved-stereo S16 PCM chunks over a MessagePort
 * (no SharedArrayBuffer, so the page needs no COOP/COEP headers). This
 * processor queues the chunks and plays them, and periodically reports how many
 * frames it has consumed / has queued so the Worker can pace decoding and show
 * elapsed time.
 *
 * Messages in (via the node port):
 *   { type: 'pcmport', port }   — the MessagePort the Worker sends PCM on
 *   { type: 'paused', value }   — output silence without consuming
 *   { type: 'flush' }           — drop queued audio, reset counters (seek/track change)
 *
 * On the pcm port:
 *   in:  { pcm: ArrayBuffer }   — interleaved S16 stereo frames
 *   out: { type: 'level', consumed, queued }   — every ~40 ms
 */

class RockboxProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this._queue = [];     // Int16Array chunks (interleaved stereo), FIFO
    this._cur = null;     // chunk currently draining
    this._pos = 0;        // frame offset within _cur
    this._queued = 0;     // frames buffered (across _queue + _cur remainder)
    this._consumed = 0;   // frames output since the last flush
    this._paused = false;
    this._pcm = null;     // MessagePort to the Worker
    this._blocks = 0;

    // The node port is only used for the one-time PCM-port handshake. All PCM
    // *and* control messages (paused/flush) come from the Worker over that PCM
    // port, so they're handled together in _onPcm — routing control over a
    // different channel is what made pause/stop lag behind the buffered queue.
    this.port.onmessage = (e) => {
      if (e.data.type === 'pcmport') {
        this._pcm = e.data.port;
        this._pcm.onmessage = (ev) => this._onPcm(ev.data);
      }
    };
    this.port.postMessage({ type: 'ready' });
  }

  _onPcm(m) {
    if (m.type === 'paused') { this._paused = !!m.value; return; }
    if (m.type === 'flush') {
      this._queue = [];
      this._cur = null;
      this._pos = 0;
      this._queued = 0;
      this._consumed = 0;
      return;
    }
    const a = new Int16Array(m.pcm); // interleaved-stereo S16 frames
    this._queue.push(a);
    this._queued += a.length >> 1;
  }

  process(_inputs, outputs) {
    const left = outputs[0][0];
    const right = outputs[0][1] || left;
    const n = left.length;

    if (this._paused) {
      left.fill(0);
      right.fill(0);
      this._report();
      return true;
    }

    for (let i = 0; i < n; i++) {
      if (!this._cur || this._pos * 2 >= this._cur.length) {
        this._cur = this._queue.shift() || null;
        this._pos = 0;
      }
      if (!this._cur) {
        left[i] = 0;
        right[i] = 0; // underrun — play silence until more arrives
        continue;
      }
      const p = this._pos * 2;
      left[i] = this._cur[p] / 32768;
      right[i] = this._cur[p + 1] / 32768;
      this._pos++;
      this._queued--;
      this._consumed++;
    }
    this._report();
    return true;
  }

  _report() {
    // ~ every 40 ms (16 × 128 frames / 48 kHz).
    if (this._pcm && ++this._blocks % 16 === 0) {
      this._pcm.postMessage({
        type: 'level',
        consumed: this._consumed,
        queued: Math.max(0, this._queued),
      });
    }
  }
}

registerProcessor('rockbox-processor', RockboxProcessor);
