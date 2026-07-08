/* rbcodec.h — C driver API around Rockbox's codec_api, bridged to Rust.
 *
 * One decode session at a time: the codec_api contract is global (each
 * codec references a single `ci` pointer), so the Rust wrapper serializes
 * sessions behind a gate.
 *
 * Flow: rbcodec_open() → rbcodec_run() on a worker thread (blocks until
 * track end / halt, emitting PCM through the sink callback) →
 * rbcodec_close(). Seek/halt are requested asynchronously and picked up
 * by the codec's next ci->get_command() poll. */
#ifndef RBCODEC_SHIM_H
#define RBCODEC_SHIM_H

#include <stdint.h>
#include <stddef.h>

/* PCM sink: interleaved stereo s16 frames at `frequency` Hz. Called on
 * the decode thread. Must not call back into rbcodec_* except the
 * request functions. */
typedef void (*rbcodec_sink_t)(void *user, const int16_t *pcm,
                               size_t frames, unsigned long frequency);

void rbcodec_set_sink(rbcodec_sink_t cb, void *user);

/* Open `path`, parse metadata, pick a codec, run CODEC_LOAD.
 * Returns 0 ok, -1 open failed, -2 metadata parse failed,
 * -3 no codec compiled in for this format, -4 codec header invalid,
 * -5 CODEC_LOAD failed, -6 out of memory. */
int rbcodec_open(const char *path);

/* Blocking decode loop (call on a dedicated thread). Returns the codec
 * status: 0 = CODEC_OK, negative = codec error. */
int rbcodec_run(void);

/* Run CODEC_UNLOAD and release the file. */
void rbcodec_close(void);

/* Asynchronous requests, picked up at the codec's next command poll. */
void rbcodec_request_seek(long time_ms);
void rbcodec_request_halt(void);

/* Progress (ms), as last reported by the codec via ci->set_elapsed. */
unsigned long rbcodec_elapsed_ms(void);

/* Current output sample rate (set by the codec via DSP_SET_FREQUENCY). */
unsigned long rbcodec_frequency(void);

/* Format label ("FLAC", …) the open file resolved to, or NULL. */
const char *rbcodec_codec_name(void);

#endif /* RBCODEC_SHIM_H */
