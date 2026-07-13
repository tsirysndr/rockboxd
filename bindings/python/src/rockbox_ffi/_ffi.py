"""cffi (ABI mode) loader for librockbox_ffi.

Declares exactly the functions we call and ``dlopen``s the prebuilt shared
library — no C is compiled here. The library is located via the
``ROCKBOX_FFI_LIB`` env var, then by walking up to the repo's
``target/release`` directory.
"""

from __future__ import annotations

import os
from pathlib import Path

from cffi import FFI

ffi = FFI()

# Mirrors include/rockbox_ffi.h — keep in sync with the C ABI.
ffi.cdef(
    """
    typedef struct RbDsp RbDsp;
    typedef struct RbPlayer RbPlayer;
    typedef struct RbDecoder RbDecoder;

    uint32_t rb_ffi_abi_version(void);

    void rb_string_free(char *p);
    void rb_buffer_free(int16_t *p, size_t len);

    RbDecoder *rb_decoder_open(const char *path);
    void       rb_decoder_free(RbDecoder *d);
    char      *rb_decoder_metadata_json(RbDecoder *d);
    int16_t   *rb_decoder_next_chunk(RbDecoder *d, size_t *out_len, uint32_t *out_sample_rate);
    void       rb_decoder_seek_ms(RbDecoder *d, uint64_t ms);
    uint64_t   rb_decoder_elapsed_ms(RbDecoder *d);
    bool       rb_decoder_finished(RbDecoder *d, int32_t *out_code);

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
    int16_t *rb_dsp_process(RbDsp *p, const int16_t *input, size_t in_len,
                            size_t *out_len);

    char *rb_meta_read_json(const char *path);
    char *rb_meta_probe(const char *filename);

    RbPlayer *rb_player_new(void);
    RbPlayer *rb_player_new_with_config(uint32_t sample_rate, float buffer_seconds,
                                        float volume, int32_t rg_mode,
                                        float rg_preamp_db, bool rg_prevent_clipping,
                                        int32_t xfade_mode, uint32_t fo_delay_ms,
                                        uint32_t fo_dur_ms, uint32_t fi_delay_ms,
                                        uint32_t fi_dur_ms, int32_t mix_mode);
    RbPlayer *rb_player_new_with_config_ex(uint32_t sample_rate, float buffer_seconds,
                                           float volume, int32_t rg_mode,
                                           float rg_preamp_db, bool rg_prevent_clipping,
                                           int32_t xfade_mode, uint32_t fo_delay_ms,
                                           uint32_t fo_dur_ms, uint32_t fi_delay_ms,
                                           uint32_t fi_dur_ms, int32_t mix_mode,
                                           const char *resume_file,
                                           uint32_t resume_save_interval_ms);
    void  rb_player_free(RbPlayer *p);
    void  rb_player_set_queue_json(RbPlayer *p, const char *json);
    void  rb_player_enqueue(RbPlayer *p, const char *path);
    void  rb_player_insert_json(RbPlayer *p, const char *json, int32_t position,
                                size_t index);
    char *rb_player_queue_json(RbPlayer *p);
    void  rb_player_play(RbPlayer *p);
    void  rb_player_pause(RbPlayer *p);
    void  rb_player_toggle(RbPlayer *p);
    void  rb_player_stop(RbPlayer *p);
    void  rb_player_next(RbPlayer *p);
    void  rb_player_previous(RbPlayer *p);
    void  rb_player_skip_to(RbPlayer *p, size_t index);
    void  rb_player_seek_ms(RbPlayer *p, uint64_t ms);
    void  rb_player_set_volume(RbPlayer *p, float vol);
    void  rb_player_set_balance(RbPlayer *p, int32_t balance);
    void  rb_player_set_crossfade(RbPlayer *p, int32_t mode, uint32_t fo_delay_ms,
                                  uint32_t fo_dur_ms, uint32_t fi_delay_ms,
                                  uint32_t fi_dur_ms, int32_t mix_mode);
    void  rb_player_set_replaygain(RbPlayer *p, int32_t mode, float preamp_db,
                                   bool prevent_clipping);
    void    rb_player_set_shuffle(RbPlayer *p, bool enabled);
    bool    rb_player_is_shuffle_enabled(RbPlayer *p);
    void    rb_player_set_repeat(RbPlayer *p, int32_t mode);
    int32_t rb_player_repeat(RbPlayer *p);
    void  rb_player_set_eq_enabled(RbPlayer *p, bool enabled);
    bool  rb_player_is_eq_enabled(RbPlayer *p);
    void  rb_player_set_eq_band(RbPlayer *p, size_t band, int32_t cutoff_hz,
                                float q, float gain_db);
    void  rb_player_set_eq_precut(RbPlayer *p, float db);
    void  rb_player_set_eq_preset(RbPlayer *p, int32_t preset);
    void  rb_player_set_tone(RbPlayer *p, int32_t bass_db, int32_t treble_db,
                             int32_t bass_cutoff_hz, int32_t treble_cutoff_hz);
    void  rb_player_set_bass(RbPlayer *p, int32_t bass_db);
    void  rb_player_set_treble(RbPlayer *p, int32_t treble_db);
    void  rb_player_set_surround(RbPlayer *p, int32_t delay_ms, int32_t balance,
                                 int32_t cutoff_low_hz, int32_t cutoff_high_hz);
    void  rb_player_set_channel_mode(RbPlayer *p, int32_t mode);
    void  rb_player_set_stereo_width(RbPlayer *p, int32_t percent);
    void  rb_player_set_compressor(RbPlayer *p, int32_t threshold_db,
                                   int32_t makeup_gain, int32_t ratio,
                                   int32_t knee, int32_t attack_ms,
                                   int32_t release_ms);
    void  rb_player_set_bass_cutoff(RbPlayer *p, int32_t hz);
    void  rb_player_set_treble_cutoff(RbPlayer *p, int32_t hz);
    void  rb_player_set_crossfeed(RbPlayer *p, int32_t mode, int32_t direct_gain,
                                  int32_t cross_gain, int32_t hf_gain,
                                  int32_t hf_cutoff);
    void  rb_player_set_bass_enhancement(RbPlayer *p, int32_t strength,
                                         int32_t precut);
    void  rb_player_set_fatigue_reduction(RbPlayer *p, int32_t strength);
    void  rb_player_set_dither(RbPlayer *p, bool enabled);
    void  rb_player_set_pitch(RbPlayer *p, int32_t ratio);
    char *rb_player_dsp_settings_json(RbPlayer *p);
    float    rb_player_volume(RbPlayer *p);
    int32_t  rb_player_balance(RbPlayer *p);
    uint32_t rb_player_sample_rate(RbPlayer *p);
    char    *rb_player_status_json(RbPlayer *p);

    char *rb_player_resume(RbPlayer *p);
    void  rb_player_save_resume(RbPlayer *p);
    void  rb_player_clear_resume(RbPlayer *p);
    char *rb_load_resume_json(const char *path);

    char *rb_player_import_m3u(RbPlayer *p, const char *path, int32_t position,
                               size_t index);
    char *rb_player_load_m3u(RbPlayer *p, const char *path);
    int32_t rb_player_export_m3u(RbPlayer *p, const char *path);
    char *rb_m3u_read_json(const char *path);
    int32_t rb_m3u_write_json(const char *path, const char *json);
    bool rb_is_url(const char *s);
    """
)


def _candidate_paths() -> list[Path]:
    names = ["librockbox_ffi.dylib", "librockbox_ffi.so", "rockbox_ffi.dll"]
    paths: list[Path] = []

    # 1. Explicit override.
    env = os.environ.get("ROCKBOX_FFI_LIB")
    if env:
        paths.append(Path(env))

    # 2. Bundled binary (platform wheels ship one in the package's _lib/ dir).
    lib_dir = Path(__file__).resolve().parent / "_lib"
    for name in names:
        paths.append(lib_dir / name)

    # 3. Walk up from this file looking for target/release (repo checkout).
    here = Path(__file__).resolve()
    for parent in here.parents:
        rel = parent / "target" / "release"
        if rel.is_dir():
            for name in names:
                paths.append(rel / name)
    return paths


def _load():
    tried: list[str] = []
    for path in _candidate_paths():
        tried.append(str(path))
        if path.exists():
            return ffi.dlopen(str(path))
    raise OSError(
        "could not locate librockbox_ffi shared library. Set ROCKBOX_FFI_LIB "
        "or run `cargo build --release -p rockbox-ffi`. Tried:\n  "
        + "\n  ".join(tried)
    )


lib = _load()


def take_string(ptr) -> str | None:
    """Copy a heap C string returned by the ABI into a str, then free it.

    Returns ``None`` for a NULL pointer (the ABI's error/absent signal).
    """
    if ptr == ffi.NULL:
        return None
    try:
        return ffi.string(ptr).decode("utf-8", "replace")
    finally:
        lib.rb_string_free(ptr)
