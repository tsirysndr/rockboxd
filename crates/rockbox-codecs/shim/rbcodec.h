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

/* Open an unbounded, forward-only stream (e.g. internet radio) instead of a
 * seekable file. `read_cb(user, buf, len)` pulls bytes (blocking; return 0 at
 * end, <0 on error). `ext` is a format hint ("mp3", "ogg", …) used to pick the
 * codec, since there is no file to sniff and get_metadata (which needs random
 * access) is skipped. Returns the same codes as rbcodec_open (0 ok, -3 no
 * codec for the format, -4 bad codec header, -5 CODEC_LOAD failed). */
typedef long (*rbcodec_stream_read_fn)(void *user, void *buf,
                                       unsigned long len);
int rbcodec_open_stream(rbcodec_stream_read_fn read_cb, void *user,
                        const char *ext);

/* Open a *seekable* callback source (e.g. an HTTP file buffered via range
 * requests) — like rbcodec_open_stream but with random access, so seeking
 * within the track works. `seek_cb(user, pos)` moves the source to absolute
 * byte `pos` (return 0 ok, non-zero fail); `size` is the total byte length
 * (or <= 0 if unknown) and `frequency` seeds id3->frequency for codecs that
 * need it (e.g. WAV) since get_metadata is skipped. Same return codes as
 * rbcodec_open_stream. */
typedef int (*rbcodec_stream_seek_fn)(void *user, int64_t pos);
int rbcodec_open_seekable(rbcodec_stream_read_fn read_cb,
                          rbcodec_stream_seek_fn seek_cb, void *user,
                          int64_t size, unsigned long frequency,
                          const char *ext);

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
