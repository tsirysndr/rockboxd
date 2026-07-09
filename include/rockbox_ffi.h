/*
 * rockbox_ffi.h — flat C ABI over rockbox-dsp + rockbox-metadata +
 * rockbox-playback. See crates/rockbox-ffi/.
 *
 * Conventions:
 *   - Handles (RbDsp*, RbPlayer*) are opaque; each *_new has a *_free.
 *     Passing NULL to a *_free or a setter is a safe no-op.
 *   - char* returned by *_json / *_probe is heap-allocated JSON (or a
 *     label) — free it with rb_string_free().
 *   - int16_t* returned by rb_dsp_process is heap-allocated — free it with
 *     rb_buffer_free(ptr, out_len).
 *   - Optional float inputs use NaN for "absent".
 */
#ifndef ROCKBOX_FFI_H
#define ROCKBOX_FFI_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct RbDsp RbDsp;
typedef struct RbPlayer RbPlayer;

/* ABI version; bump major on any breaking change. */
uint32_t rb_ffi_abi_version(void);

/* ---- deallocators --------------------------------------------------- */
void rb_string_free(char *p);
void rb_buffer_free(int16_t *p, size_t len);

/* ---- DSP ------------------------------------------------------------- */
/* ReplayGain mode: 0 track, 1 album, 2 shuffle, 3 off.
 * Channel config: 0 stereo, 1 mono, 2 custom, 3 mono-left, 4 mono-right,
 *                 5 karaoke, 6 swap. */
RbDsp *rb_dsp_new(uint32_t sample_rate);
void   rb_dsp_free(RbDsp *p);
void   rb_dsp_set_input_frequency(RbDsp *p, uint32_t hz);
void   rb_dsp_flush(RbDsp *p);
void   rb_dsp_eq_enable(RbDsp *p, bool enable);
void   rb_dsp_set_tone(RbDsp *p, int32_t bass_db, int32_t treble_db);
void   rb_dsp_set_tone_cutoffs(RbDsp *p, int32_t bass_hz, int32_t treble_hz);
void   rb_dsp_set_surround(RbDsp *p, int32_t delay_ms, int32_t balance,
                           int32_t fx1, int32_t fx2);
void   rb_dsp_set_channel_config(RbDsp *p, int32_t mode);
void   rb_dsp_set_stereo_width(RbDsp *p, int32_t percent);
void   rb_dsp_set_compressor(RbDsp *p, int32_t threshold, int32_t makeup_gain,
                             int32_t ratio, int32_t knee, int32_t release_time,
                             int32_t attack_time);
void   rb_dsp_set_replaygain(RbDsp *p, int32_t mode, bool noclip,
                             float preamp_db);
void   rb_dsp_set_replaygain_gains(RbDsp *p, float track_gain_db,
                                   float album_gain_db, float track_peak,
                                   float album_peak);
void   rb_dsp_set_replaygain_gains_raw(RbDsp *p, int64_t track_gain,
                                       int64_t album_gain, int64_t track_peak,
                                       int64_t album_peak);
void   rb_dsp_set_eq_band(RbDsp *p, size_t band, int32_t cutoff_hz, float q,
                          float gain_db);
void   rb_dsp_set_eq_precut(RbDsp *p, float db);
/* Returns a heap buffer of *out_len int16 samples; free with
 * rb_buffer_free(ptr, *out_len). NULL on error. */
int16_t *rb_dsp_process(RbDsp *p, const int16_t *input, size_t in_len,
                        size_t *out_len);

/* ---- metadata -------------------------------------------------------- */
char *rb_meta_read_json(const char *path);
char *rb_meta_probe(const char *filename);

/* ---- playback -------------------------------------------------------- */
/* Crossfade mode: 0 off, 1 auto-skip, 2 manual-skip, 3 shuffle,
 *                 4 shuffle-or-manual, 5 always.  Mix mode: 0 crossfade, 1 mix. */
RbPlayer *rb_player_new(void);
RbPlayer *rb_player_new_with_config(uint32_t sample_rate, float buffer_seconds,
                                    float volume, int32_t rg_mode,
                                    float rg_preamp_db, bool rg_prevent_clipping,
                                    int32_t xfade_mode, uint32_t fo_delay_ms,
                                    uint32_t fo_dur_ms, uint32_t fi_delay_ms,
                                    uint32_t fi_dur_ms, int32_t mix_mode);
void  rb_player_free(RbPlayer *p);
void  rb_player_set_queue_json(RbPlayer *p, const char *json);
void  rb_player_enqueue(RbPlayer *p, const char *path);
void  rb_player_play(RbPlayer *p);
void  rb_player_pause(RbPlayer *p);
void  rb_player_toggle(RbPlayer *p);
void  rb_player_stop(RbPlayer *p);
void  rb_player_next(RbPlayer *p);
void  rb_player_previous(RbPlayer *p);
void  rb_player_skip_to(RbPlayer *p, size_t index);
void  rb_player_seek_ms(RbPlayer *p, uint64_t ms);
void  rb_player_set_volume(RbPlayer *p, float vol);
void  rb_player_set_crossfade(RbPlayer *p, int32_t mode, uint32_t fo_delay_ms,
                              uint32_t fo_dur_ms, uint32_t fi_delay_ms,
                              uint32_t fi_dur_ms, int32_t mix_mode);
void  rb_player_set_replaygain(RbPlayer *p, int32_t mode, float preamp_db,
                               bool prevent_clipping);
float    rb_player_volume(RbPlayer *p);
uint32_t rb_player_sample_rate(RbPlayer *p);
char    *rb_player_status_json(RbPlayer *p);

#ifdef __cplusplus
}
#endif

#endif /* ROCKBOX_FFI_H */
