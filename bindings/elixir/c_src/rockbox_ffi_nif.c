/*
 * rockbox_ffi_nif.c — erl_nif shim over the librockbox_ffi C ABI.
 *
 * SHARED between the Elixir and Gleam bindings (kept byte-identical; see
 * bindings/gleam/c_src/rockbox_ffi_nif.c). It exposes a faithful 1:1 surface
 * of include/rockbox_ffi.h to the Erlang module `rockbox_ffi_nif`; all
 * ergonomics live in the language layers.
 *
 * Handles (RbDsp*, RbPlayer*) are wrapped in enif resources so the BEAM GC
 * frees them via the destructors. JSON reads return binaries; DSP processing
 * takes/returns int16 binaries. Heavy calls are scheduled on dirty threads.
 */
#include <math.h>
#include <string.h>

#include "erl_nif.h"
#include "rockbox_ffi.h"

static ErlNifResourceType *DSP_RES;
static ErlNifResourceType *PLAYER_RES;

static ERL_NIF_TERM am_nil;
static ERL_NIF_TERM am_true;
static ERL_NIF_TERM am_false;

/* ---- resource destructors ------------------------------------------- */
static void dsp_dtor(ErlNifEnv *env, void *obj) {
  (void)env;
  RbDsp *p = *(RbDsp **)obj;
  if (p) rb_dsp_free(p);
}
static void player_dtor(ErlNifEnv *env, void *obj) {
  (void)env;
  RbPlayer *p = *(RbPlayer **)obj;
  if (p) rb_player_free(p);
}

/* ---- small decode helpers ------------------------------------------- */
static int get_bool(ErlNifEnv *env, ERL_NIF_TERM t, bool *out) {
  if (enif_is_identical(t, am_true)) { *out = true; return 1; }
  if (enif_is_identical(t, am_false)) { *out = false; return 1; }
  int i;
  if (enif_get_int(env, t, &i)) { *out = i != 0; return 1; }
  return 0;
}

/* A double, or NaN for "tag absent". Any atom counts as absent — Elixir
 * passes `nil`, Gleam passes `Nil` (also the atom `nil`) via dynamic.from;
 * accepting any atom keeps both ergonomic. */
static int get_opt_double(ErlNifEnv *env, ERL_NIF_TERM t, double *out) {
  double d;
  if (enif_get_double(env, t, &d)) { *out = d; return 1; }
  long l;
  if (enif_get_long(env, t, &l)) { *out = (double)l; return 1; }
  if (enif_is_atom(env, t)) { *out = NAN; return 1; }
  return 0;
}

static int get_double_any(ErlNifEnv *env, ERL_NIF_TERM t, double *out) {
  if (enif_get_double(env, t, out)) return 1;
  long l;
  if (enif_get_long(env, t, &l)) { *out = (double)l; return 1; }
  return 0;
}

/* Fetch the RbDsp* from argv[0]; returns NULL term-check via badarg. */
static RbDsp *get_dsp(ErlNifEnv *env, ERL_NIF_TERM t) {
  void *obj;
  if (!enif_get_resource(env, t, DSP_RES, &obj)) return NULL;
  return *(RbDsp **)obj;
}
static RbPlayer *get_player(ErlNifEnv *env, ERL_NIF_TERM t) {
  void *obj;
  if (!enif_get_resource(env, t, PLAYER_RES, &obj)) return NULL;
  return *(RbPlayer **)obj;
}

/* Copy a NUL-terminated C string from an Erlang binary (caller frees). */
static char *binary_to_cstr(ErlNifEnv *env, ERL_NIF_TERM t) {
  ErlNifBinary bin;
  if (!enif_inspect_binary(env, t, &bin)) return NULL;
  char *s = enif_alloc(bin.size + 1);
  if (!s) return NULL;
  memcpy(s, bin.data, bin.size);
  s[bin.size] = 0;
  return s;
}

/* Build an Erlang binary from a heap C string, or `nil` if NULL. Frees it. */
static ERL_NIF_TERM take_cstr(ErlNifEnv *env, char *s) {
  if (!s) return am_nil;
  size_t len = strlen(s);
  ERL_NIF_TERM bin;
  unsigned char *dst = enif_make_new_binary(env, len, &bin);
  memcpy(dst, s, len);
  rb_string_free(s);
  return bin;
}

/* ===================================================================== */
/* misc                                                                  */
/* ===================================================================== */
static ERL_NIF_TERM nif_abi_version(ErlNifEnv *env, int argc,
                                    const ERL_NIF_TERM argv[]) {
  (void)argc; (void)argv;
  return enif_make_uint(env, rb_ffi_abi_version());
}

/* ===================================================================== */
/* metadata                                                              */
/* ===================================================================== */
static ERL_NIF_TERM nif_meta_read_json(ErlNifEnv *env, int argc,
                                       const ERL_NIF_TERM argv[]) {
  (void)argc;
  char *path = binary_to_cstr(env, argv[0]);
  if (!path) return enif_make_badarg(env);
  char *json = rb_meta_read_json(path);
  enif_free(path);
  return take_cstr(env, json);
}

static ERL_NIF_TERM nif_meta_probe(ErlNifEnv *env, int argc,
                                   const ERL_NIF_TERM argv[]) {
  (void)argc;
  char *name = binary_to_cstr(env, argv[0]);
  if (!name) return enif_make_badarg(env);
  char *label = rb_meta_probe(name);
  enif_free(name);
  return take_cstr(env, label);
}

/* ===================================================================== */
/* DSP                                                                   */
/* ===================================================================== */
static ERL_NIF_TERM nif_dsp_new(ErlNifEnv *env, int argc,
                                const ERL_NIF_TERM argv[]) {
  (void)argc;
  unsigned int rate;
  if (!enif_get_uint(env, argv[0], &rate)) return enif_make_badarg(env);
  RbDsp *p = rb_dsp_new(rate);
  if (!p) return am_nil;
  void *obj = enif_alloc_resource(DSP_RES, sizeof(RbDsp *));
  *(RbDsp **)obj = p;
  ERL_NIF_TERM term = enif_make_resource(env, obj);
  enif_release_resource(obj);
  return term;
}

#define DSP0(name, call)                                                    \
  static ERL_NIF_TERM name(ErlNifEnv *env, int argc,                        \
                           const ERL_NIF_TERM argv[]) {                      \
    (void)argc;                                                             \
    RbDsp *d = get_dsp(env, argv[0]);                                       \
    if (!d) return enif_make_badarg(env);                                   \
    call;                                                                   \
    return am_nil;                                                          \
  }

DSP0(nif_dsp_flush, rb_dsp_flush(d))

static ERL_NIF_TERM nif_dsp_set_input_frequency(ErlNifEnv *env, int argc,
                                                const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  unsigned int hz;
  if (!d || !enif_get_uint(env, argv[1], &hz)) return enif_make_badarg(env);
  rb_dsp_set_input_frequency(d, hz);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_eq_enable(ErlNifEnv *env, int argc,
                                      const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  bool en;
  if (!d || !get_bool(env, argv[1], &en)) return enif_make_badarg(env);
  rb_dsp_eq_enable(d, en);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_tone(ErlNifEnv *env, int argc,
                                     const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  int bass, treble;
  if (!d || !enif_get_int(env, argv[1], &bass) ||
      !enif_get_int(env, argv[2], &treble))
    return enif_make_badarg(env);
  rb_dsp_set_tone(d, bass, treble);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_tone_cutoffs(ErlNifEnv *env, int argc,
                                             const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  int b, t;
  if (!d || !enif_get_int(env, argv[1], &b) || !enif_get_int(env, argv[2], &t))
    return enif_make_badarg(env);
  rb_dsp_set_tone_cutoffs(d, b, t);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_surround(ErlNifEnv *env, int argc,
                                         const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  int delay, balance, fx1, fx2;
  if (!d || !enif_get_int(env, argv[1], &delay) ||
      !enif_get_int(env, argv[2], &balance) ||
      !enif_get_int(env, argv[3], &fx1) || !enif_get_int(env, argv[4], &fx2))
    return enif_make_badarg(env);
  rb_dsp_set_surround(d, delay, balance, fx1, fx2);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_channel_config(ErlNifEnv *env, int argc,
                                               const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  int mode;
  if (!d || !enif_get_int(env, argv[1], &mode)) return enif_make_badarg(env);
  rb_dsp_set_channel_config(d, mode);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_stereo_width(ErlNifEnv *env, int argc,
                                             const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  int pct;
  if (!d || !enif_get_int(env, argv[1], &pct)) return enif_make_badarg(env);
  rb_dsp_set_stereo_width(d, pct);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_compressor(ErlNifEnv *env, int argc,
                                           const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  int th, mk, ratio, knee, rel, atk;
  if (!d || !enif_get_int(env, argv[1], &th) ||
      !enif_get_int(env, argv[2], &mk) || !enif_get_int(env, argv[3], &ratio) ||
      !enif_get_int(env, argv[4], &knee) || !enif_get_int(env, argv[5], &rel) ||
      !enif_get_int(env, argv[6], &atk))
    return enif_make_badarg(env);
  rb_dsp_set_compressor(d, th, mk, ratio, knee, rel, atk);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_replaygain(ErlNifEnv *env, int argc,
                                           const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  int mode; bool noclip; double preamp;
  if (!d || !enif_get_int(env, argv[1], &mode) ||
      !get_bool(env, argv[2], &noclip) ||
      !get_double_any(env, argv[3], &preamp))
    return enif_make_badarg(env);
  rb_dsp_set_replaygain(d, mode, noclip, (float)preamp);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_replaygain_gains(ErlNifEnv *env, int argc,
                                                 const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  double tg, ag, tp, ap;
  if (!d || !get_opt_double(env, argv[1], &tg) ||
      !get_opt_double(env, argv[2], &ag) ||
      !get_opt_double(env, argv[3], &tp) ||
      !get_opt_double(env, argv[4], &ap))
    return enif_make_badarg(env);
  rb_dsp_set_replaygain_gains(d, (float)tg, (float)ag, (float)tp, (float)ap);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_replaygain_gains_raw(ErlNifEnv *env, int argc,
                                                     const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  ErlNifSInt64 tg, ag, tp, ap;
  if (!d || !enif_get_int64(env, argv[1], &tg) ||
      !enif_get_int64(env, argv[2], &ag) ||
      !enif_get_int64(env, argv[3], &tp) ||
      !enif_get_int64(env, argv[4], &ap))
    return enif_make_badarg(env);
  rb_dsp_set_replaygain_gains_raw(d, tg, ag, tp, ap);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_eq_band(ErlNifEnv *env, int argc,
                                        const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  unsigned int band; int cutoff; double q, gain;
  if (!d || !enif_get_uint(env, argv[1], &band) ||
      !enif_get_int(env, argv[2], &cutoff) ||
      !get_double_any(env, argv[3], &q) ||
      !get_double_any(env, argv[4], &gain))
    return enif_make_badarg(env);
  rb_dsp_set_eq_band(d, band, cutoff, (float)q, (float)gain);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_set_eq_precut(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  double db;
  if (!d || !get_double_any(env, argv[1], &db)) return enif_make_badarg(env);
  rb_dsp_set_eq_precut(d, (float)db);
  return am_nil;
}

static ERL_NIF_TERM nif_dsp_process(ErlNifEnv *env, int argc,
                                    const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbDsp *d = get_dsp(env, argv[0]);
  ErlNifBinary in;
  if (!d || !enif_inspect_binary(env, argv[1], &in))
    return enif_make_badarg(env);
  size_t n = in.size / 2; /* int16 samples */
  size_t out_len = 0;
  int16_t *out = rb_dsp_process(d, (const int16_t *)in.data, n, &out_len);
  if (!out || out_len == 0) {
    if (out) rb_buffer_free(out, out_len);
    ERL_NIF_TERM empty;
    enif_make_new_binary(env, 0, &empty);
    return empty;
  }
  ERL_NIF_TERM bin;
  unsigned char *dst = enif_make_new_binary(env, out_len * 2, &bin);
  memcpy(dst, out, out_len * 2);
  rb_buffer_free(out, out_len);
  return bin;
}

/* ===================================================================== */
/* Player                                                                */
/* ===================================================================== */
static ERL_NIF_TERM make_player(ErlNifEnv *env, RbPlayer *p) {
  if (!p) return am_nil;
  void *obj = enif_alloc_resource(PLAYER_RES, sizeof(RbPlayer *));
  *(RbPlayer **)obj = p;
  ERL_NIF_TERM term = enif_make_resource(env, obj);
  enif_release_resource(obj);
  return term;
}

static ERL_NIF_TERM nif_player_new(ErlNifEnv *env, int argc,
                                   const ERL_NIF_TERM argv[]) {
  (void)argc; (void)argv;
  return make_player(env, rb_player_new());
}

static ERL_NIF_TERM nif_player_new_with_config(ErlNifEnv *env, int argc,
                                               const ERL_NIF_TERM argv[]) {
  (void)argc;
  unsigned int rate, fo_del, fo_dur, fi_del, fi_dur;
  int rg_mode, xfade, mix;
  double buf_sec, vol, rg_preamp;
  bool rg_clip;
  if (!enif_get_uint(env, argv[0], &rate) ||
      !get_double_any(env, argv[1], &buf_sec) ||
      !get_double_any(env, argv[2], &vol) ||
      !enif_get_int(env, argv[3], &rg_mode) ||
      !get_double_any(env, argv[4], &rg_preamp) ||
      !get_bool(env, argv[5], &rg_clip) ||
      !enif_get_int(env, argv[6], &xfade) ||
      !enif_get_uint(env, argv[7], &fo_del) ||
      !enif_get_uint(env, argv[8], &fo_dur) ||
      !enif_get_uint(env, argv[9], &fi_del) ||
      !enif_get_uint(env, argv[10], &fi_dur) ||
      !enif_get_int(env, argv[11], &mix))
    return enif_make_badarg(env);
  RbPlayer *p = rb_player_new_with_config(
      rate, (float)buf_sec, (float)vol, rg_mode, (float)rg_preamp, rg_clip,
      xfade, fo_del, fo_dur, fi_del, fi_dur, mix);
  return make_player(env, p);
}

#define PLAYER0(name, call)                                                 \
  static ERL_NIF_TERM name(ErlNifEnv *env, int argc,                        \
                           const ERL_NIF_TERM argv[]) {                      \
    (void)argc;                                                             \
    RbPlayer *p = get_player(env, argv[0]);                                 \
    if (!p) return enif_make_badarg(env);                                   \
    call;                                                                   \
    return am_nil;                                                          \
  }

PLAYER0(nif_player_play, rb_player_play(p))
PLAYER0(nif_player_pause, rb_player_pause(p))
PLAYER0(nif_player_toggle, rb_player_toggle(p))
PLAYER0(nif_player_stop, rb_player_stop(p))
PLAYER0(nif_player_next, rb_player_next(p))
PLAYER0(nif_player_previous, rb_player_previous(p))

static ERL_NIF_TERM nif_player_set_queue_json(ErlNifEnv *env, int argc,
                                              const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  char *json = binary_to_cstr(env, argv[1]);
  if (!json) return enif_make_badarg(env);
  rb_player_set_queue_json(p, json);
  enif_free(json);
  return am_nil;
}

static ERL_NIF_TERM nif_player_enqueue(ErlNifEnv *env, int argc,
                                       const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  char *path = binary_to_cstr(env, argv[1]);
  if (!path) return enif_make_badarg(env);
  rb_player_enqueue(p, path);
  enif_free(path);
  return am_nil;
}

static ERL_NIF_TERM nif_player_skip_to(ErlNifEnv *env, int argc,
                                       const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  ErlNifUInt64 idx;
  if (!p || !enif_get_uint64(env, argv[1], &idx)) return enif_make_badarg(env);
  rb_player_skip_to(p, (size_t)idx);
  return am_nil;
}

static ERL_NIF_TERM nif_player_seek_ms(ErlNifEnv *env, int argc,
                                       const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  ErlNifUInt64 ms;
  if (!p || !enif_get_uint64(env, argv[1], &ms)) return enif_make_badarg(env);
  rb_player_seek_ms(p, ms);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_volume(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  double vol;
  if (!p || !get_double_any(env, argv[1], &vol)) return enif_make_badarg(env);
  rb_player_set_volume(p, (float)vol);
  return am_nil;
}

static ERL_NIF_TERM nif_player_volume(ErlNifEnv *env, int argc,
                                      const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return enif_make_double(env, rb_player_volume(p));
}

static ERL_NIF_TERM nif_player_sample_rate(ErlNifEnv *env, int argc,
                                           const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return enif_make_uint(env, rb_player_sample_rate(p));
}

static ERL_NIF_TERM nif_player_set_crossfade(ErlNifEnv *env, int argc,
                                             const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int mode, mix;
  unsigned int fo_del, fo_dur, fi_del, fi_dur;
  if (!p || !enif_get_int(env, argv[1], &mode) ||
      !enif_get_uint(env, argv[2], &fo_del) ||
      !enif_get_uint(env, argv[3], &fo_dur) ||
      !enif_get_uint(env, argv[4], &fi_del) ||
      !enif_get_uint(env, argv[5], &fi_dur) ||
      !enif_get_int(env, argv[6], &mix))
    return enif_make_badarg(env);
  rb_player_set_crossfade(p, mode, fo_del, fo_dur, fi_del, fi_dur, mix);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_replaygain(ErlNifEnv *env, int argc,
                                              const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int mode; double preamp; bool clip;
  if (!p || !enif_get_int(env, argv[1], &mode) ||
      !get_double_any(env, argv[2], &preamp) ||
      !get_bool(env, argv[3], &clip))
    return enif_make_badarg(env);
  rb_player_set_replaygain(p, mode, (float)preamp, clip);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_shuffle(ErlNifEnv *env, int argc,
                                           const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  bool en;
  if (!p || !get_bool(env, argv[1], &en)) return enif_make_badarg(env);
  rb_player_set_shuffle(p, en);
  return am_nil;
}

static ERL_NIF_TERM nif_player_is_shuffle_enabled(ErlNifEnv *env, int argc,
                                                  const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return rb_player_is_shuffle_enabled(p) ? am_true : am_false;
}

static ERL_NIF_TERM nif_player_set_repeat(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int mode;
  if (!p || !enif_get_int(env, argv[1], &mode)) return enif_make_badarg(env);
  rb_player_set_repeat(p, mode);
  return am_nil;
}

static ERL_NIF_TERM nif_player_repeat(ErlNifEnv *env, int argc,
                                      const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return enif_make_int(env, rb_player_repeat(p));
}

static ERL_NIF_TERM nif_player_status_json(ErlNifEnv *env, int argc,
                                           const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return take_cstr(env, rb_player_status_json(p));
}

/* ---- player DSP chain ----------------------------------------------- */
static ERL_NIF_TERM nif_player_set_eq_enabled(ErlNifEnv *env, int argc,
                                              const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  bool en;
  if (!p || !get_bool(env, argv[1], &en)) return enif_make_badarg(env);
  rb_player_set_eq_enabled(p, en);
  return am_nil;
}

static ERL_NIF_TERM nif_player_is_eq_enabled(ErlNifEnv *env, int argc,
                                             const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return rb_player_is_eq_enabled(p) ? am_true : am_false;
}

static ERL_NIF_TERM nif_player_set_eq_band(ErlNifEnv *env, int argc,
                                           const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  unsigned int band; int cutoff; double q, gain;
  if (!p || !enif_get_uint(env, argv[1], &band) ||
      !enif_get_int(env, argv[2], &cutoff) ||
      !get_double_any(env, argv[3], &q) ||
      !get_double_any(env, argv[4], &gain))
    return enif_make_badarg(env);
  rb_player_set_eq_band(p, band, cutoff, (float)q, (float)gain);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_eq_precut(ErlNifEnv *env, int argc,
                                             const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  double db;
  if (!p || !get_double_any(env, argv[1], &db)) return enif_make_badarg(env);
  rb_player_set_eq_precut(p, (float)db);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_eq_preset(ErlNifEnv *env, int argc,
                                             const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int preset;
  if (!p || !enif_get_int(env, argv[1], &preset)) return enif_make_badarg(env);
  rb_player_set_eq_preset(p, preset);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_tone(ErlNifEnv *env, int argc,
                                        const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int bass, treble, bass_cut, treble_cut;
  if (!p || !enif_get_int(env, argv[1], &bass) ||
      !enif_get_int(env, argv[2], &treble) ||
      !enif_get_int(env, argv[3], &bass_cut) ||
      !enif_get_int(env, argv[4], &treble_cut))
    return enif_make_badarg(env);
  rb_player_set_tone(p, bass, treble, bass_cut, treble_cut);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_bass(ErlNifEnv *env, int argc,
                                        const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int bass;
  if (!p || !enif_get_int(env, argv[1], &bass)) return enif_make_badarg(env);
  rb_player_set_bass(p, bass);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_treble(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int treble;
  if (!p || !enif_get_int(env, argv[1], &treble)) return enif_make_badarg(env);
  rb_player_set_treble(p, treble);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_bass_cutoff(ErlNifEnv *env, int argc,
                                               const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int hz;
  if (!p || !enif_get_int(env, argv[1], &hz)) return enif_make_badarg(env);
  rb_player_set_bass_cutoff(p, hz);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_treble_cutoff(ErlNifEnv *env, int argc,
                                                 const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int hz;
  if (!p || !enif_get_int(env, argv[1], &hz)) return enif_make_badarg(env);
  rb_player_set_treble_cutoff(p, hz);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_crossfeed(ErlNifEnv *env, int argc,
                                             const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int mode, direct_gain, cross_gain, hf_gain, hf_cutoff;
  if (!p || !enif_get_int(env, argv[1], &mode) ||
      !enif_get_int(env, argv[2], &direct_gain) ||
      !enif_get_int(env, argv[3], &cross_gain) ||
      !enif_get_int(env, argv[4], &hf_gain) ||
      !enif_get_int(env, argv[5], &hf_cutoff))
    return enif_make_badarg(env);
  rb_player_set_crossfeed(p, mode, direct_gain, cross_gain, hf_gain, hf_cutoff);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_bass_enhancement(ErlNifEnv *env, int argc,
                                                    const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int strength, precut;
  if (!p || !enif_get_int(env, argv[1], &strength) ||
      !enif_get_int(env, argv[2], &precut))
    return enif_make_badarg(env);
  rb_player_set_bass_enhancement(p, strength, precut);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_fatigue_reduction(ErlNifEnv *env, int argc,
                                                     const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int strength;
  if (!p || !enif_get_int(env, argv[1], &strength)) return enif_make_badarg(env);
  rb_player_set_fatigue_reduction(p, strength);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_surround(ErlNifEnv *env, int argc,
                                            const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int delay, balance, low, high;
  if (!p || !enif_get_int(env, argv[1], &delay) ||
      !enif_get_int(env, argv[2], &balance) ||
      !enif_get_int(env, argv[3], &low) || !enif_get_int(env, argv[4], &high))
    return enif_make_badarg(env);
  rb_player_set_surround(p, delay, balance, low, high);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_channel_mode(ErlNifEnv *env, int argc,
                                                const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int mode;
  if (!p || !enif_get_int(env, argv[1], &mode)) return enif_make_badarg(env);
  rb_player_set_channel_mode(p, mode);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_stereo_width(ErlNifEnv *env, int argc,
                                                const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int pct;
  if (!p || !enif_get_int(env, argv[1], &pct)) return enif_make_badarg(env);
  rb_player_set_stereo_width(p, pct);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_compressor(ErlNifEnv *env, int argc,
                                              const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int th, mk, ratio, knee, atk, rel;
  if (!p || !enif_get_int(env, argv[1], &th) ||
      !enif_get_int(env, argv[2], &mk) || !enif_get_int(env, argv[3], &ratio) ||
      !enif_get_int(env, argv[4], &knee) || !enif_get_int(env, argv[5], &atk) ||
      !enif_get_int(env, argv[6], &rel))
    return enif_make_badarg(env);
  rb_player_set_compressor(p, th, mk, ratio, knee, atk, rel);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_dither(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  bool en;
  if (!p || !get_bool(env, argv[1], &en)) return enif_make_badarg(env);
  rb_player_set_dither(p, en);
  return am_nil;
}

static ERL_NIF_TERM nif_player_set_pitch(ErlNifEnv *env, int argc,
                                         const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int ratio;
  if (!p || !enif_get_int(env, argv[1], &ratio)) return enif_make_badarg(env);
  rb_player_set_pitch(p, ratio);
  return am_nil;
}

static ERL_NIF_TERM nif_player_dsp_settings_json(ErlNifEnv *env, int argc,
                                                 const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return take_cstr(env, rb_player_dsp_settings_json(p));
}

static ERL_NIF_TERM nif_player_new_with_config_ex(ErlNifEnv *env, int argc,
                                                  const ERL_NIF_TERM argv[]) {
  (void)argc;
  unsigned int rate, fo_del, fo_dur, fi_del, fi_dur, resume_int;
  int rg_mode, xfade, mix;
  double buf_sec, vol, rg_preamp;
  bool rg_clip;
  if (!enif_get_uint(env, argv[0], &rate) ||
      !get_double_any(env, argv[1], &buf_sec) ||
      !get_double_any(env, argv[2], &vol) ||
      !enif_get_int(env, argv[3], &rg_mode) ||
      !get_double_any(env, argv[4], &rg_preamp) ||
      !get_bool(env, argv[5], &rg_clip) ||
      !enif_get_int(env, argv[6], &xfade) ||
      !enif_get_uint(env, argv[7], &fo_del) ||
      !enif_get_uint(env, argv[8], &fo_dur) ||
      !enif_get_uint(env, argv[9], &fi_del) ||
      !enif_get_uint(env, argv[10], &fi_dur) ||
      !enif_get_int(env, argv[11], &mix) ||
      !enif_get_uint(env, argv[13], &resume_int))
    return enif_make_badarg(env);
  /* argv[12] is the resume file: a binary path, or any atom (nil) for none. */
  char *resume_file = NULL;
  if (!enif_is_atom(env, argv[12])) {
    resume_file = binary_to_cstr(env, argv[12]);
    if (!resume_file) return enif_make_badarg(env);
  }
  RbPlayer *p = rb_player_new_with_config_ex(
      rate, (float)buf_sec, (float)vol, rg_mode, (float)rg_preamp, rg_clip,
      xfade, fo_del, fo_dur, fi_del, fi_dur, mix, resume_file, resume_int);
  if (resume_file) enif_free(resume_file);
  return make_player(env, p);
}

static ERL_NIF_TERM nif_player_insert_json(ErlNifEnv *env, int argc,
                                           const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int position;
  ErlNifUInt64 index;
  if (!p || !enif_get_int(env, argv[2], &position) ||
      !enif_get_uint64(env, argv[3], &index))
    return enif_make_badarg(env);
  char *json = binary_to_cstr(env, argv[1]);
  if (!json) return enif_make_badarg(env);
  rb_player_insert_json(p, json, position, (size_t)index);
  enif_free(json);
  return am_nil;
}

static ERL_NIF_TERM nif_player_queue_json(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return take_cstr(env, rb_player_queue_json(p));
}

static ERL_NIF_TERM nif_player_resume(ErlNifEnv *env, int argc,
                                      const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  return take_cstr(env, rb_player_resume(p));
}

PLAYER0(nif_player_save_resume, rb_player_save_resume(p))
PLAYER0(nif_player_clear_resume, rb_player_clear_resume(p))

static ERL_NIF_TERM nif_load_resume_json(ErlNifEnv *env, int argc,
                                         const ERL_NIF_TERM argv[]) {
  (void)argc;
  char *path = binary_to_cstr(env, argv[0]);
  if (!path) return enif_make_badarg(env);
  char *json = rb_load_resume_json(path);
  enif_free(path);
  return take_cstr(env, json);
}

static ERL_NIF_TERM nif_player_import_m3u(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  int position;
  ErlNifUInt64 index;
  if (!p || !enif_get_int(env, argv[2], &position) ||
      !enif_get_uint64(env, argv[3], &index))
    return enif_make_badarg(env);
  char *path = binary_to_cstr(env, argv[1]);
  if (!path) return enif_make_badarg(env);
  char *json = rb_player_import_m3u(p, path, position, (size_t)index);
  enif_free(path);
  return take_cstr(env, json);
}

static ERL_NIF_TERM nif_player_load_m3u(ErlNifEnv *env, int argc,
                                        const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  char *path = binary_to_cstr(env, argv[1]);
  if (!path) return enif_make_badarg(env);
  char *json = rb_player_load_m3u(p, path);
  enif_free(path);
  return take_cstr(env, json);
}

static ERL_NIF_TERM nif_player_export_m3u(ErlNifEnv *env, int argc,
                                          const ERL_NIF_TERM argv[]) {
  (void)argc;
  RbPlayer *p = get_player(env, argv[0]);
  if (!p) return enif_make_badarg(env);
  char *path = binary_to_cstr(env, argv[1]);
  if (!path) return enif_make_badarg(env);
  int32_t rc = rb_player_export_m3u(p, path);
  enif_free(path);
  return enif_make_int(env, rc);
}

static ERL_NIF_TERM nif_m3u_read_json(ErlNifEnv *env, int argc,
                                      const ERL_NIF_TERM argv[]) {
  (void)argc;
  char *path = binary_to_cstr(env, argv[0]);
  if (!path) return enif_make_badarg(env);
  char *json = rb_m3u_read_json(path);
  enif_free(path);
  return take_cstr(env, json);
}

static ERL_NIF_TERM nif_m3u_write_json(ErlNifEnv *env, int argc,
                                       const ERL_NIF_TERM argv[]) {
  (void)argc;
  char *path = binary_to_cstr(env, argv[0]);
  if (!path) return enif_make_badarg(env);
  char *json = binary_to_cstr(env, argv[1]);
  if (!json) { enif_free(path); return enif_make_badarg(env); }
  int32_t rc = rb_m3u_write_json(path, json);
  enif_free(path);
  enif_free(json);
  return enif_make_int(env, rc);
}

static ERL_NIF_TERM nif_is_url(ErlNifEnv *env, int argc,
                               const ERL_NIF_TERM argv[]) {
  (void)argc;
  char *s = binary_to_cstr(env, argv[0]);
  if (!s) return enif_make_badarg(env);
  bool r = rb_is_url(s);
  enif_free(s);
  return r ? am_true : am_false;
}

/* ===================================================================== */
/* registration                                                          */
/* ===================================================================== */
static int load(ErlNifEnv *env, void **priv, ERL_NIF_TERM info) {
  (void)priv; (void)info;
  am_nil = enif_make_atom(env, "nil");
  am_true = enif_make_atom(env, "true");
  am_false = enif_make_atom(env, "false");
  ErlNifResourceFlags fl = ERL_NIF_RT_CREATE | ERL_NIF_RT_TAKEOVER;
  DSP_RES = enif_open_resource_type(env, NULL, "rockbox_dsp", dsp_dtor, fl, NULL);
  PLAYER_RES =
      enif_open_resource_type(env, NULL, "rockbox_player", player_dtor, fl, NULL);
  if (!DSP_RES || !PLAYER_RES) return -1;
  return 0;
}

static ErlNifFunc funcs[] = {
    {"abi_version", 0, nif_abi_version, 0},
    {"meta_read_json", 1, nif_meta_read_json, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"meta_probe", 1, nif_meta_probe, 0},

    {"dsp_new", 1, nif_dsp_new, 0},
    {"dsp_set_input_frequency", 2, nif_dsp_set_input_frequency, 0},
    {"dsp_flush", 1, nif_dsp_flush, 0},
    {"dsp_eq_enable", 2, nif_dsp_eq_enable, 0},
    {"dsp_set_tone", 3, nif_dsp_set_tone, 0},
    {"dsp_set_tone_cutoffs", 3, nif_dsp_set_tone_cutoffs, 0},
    {"dsp_set_surround", 5, nif_dsp_set_surround, 0},
    {"dsp_set_channel_config", 2, nif_dsp_set_channel_config, 0},
    {"dsp_set_stereo_width", 2, nif_dsp_set_stereo_width, 0},
    {"dsp_set_compressor", 7, nif_dsp_set_compressor, 0},
    {"dsp_set_replaygain", 4, nif_dsp_set_replaygain, 0},
    {"dsp_set_replaygain_gains", 5, nif_dsp_set_replaygain_gains, 0},
    {"dsp_set_replaygain_gains_raw", 5, nif_dsp_set_replaygain_gains_raw, 0},
    {"dsp_set_eq_band", 5, nif_dsp_set_eq_band, 0},
    {"dsp_set_eq_precut", 2, nif_dsp_set_eq_precut, 0},
    {"dsp_process", 2, nif_dsp_process, ERL_NIF_DIRTY_JOB_CPU_BOUND},

    {"player_new", 0, nif_player_new, ERL_NIF_DIRTY_JOB_CPU_BOUND},
    {"player_new_with_config", 12, nif_player_new_with_config,
     ERL_NIF_DIRTY_JOB_CPU_BOUND},
    {"player_set_queue_json", 2, nif_player_set_queue_json, 0},
    {"player_enqueue", 2, nif_player_enqueue, 0},
    {"player_play", 1, nif_player_play, 0},
    {"player_pause", 1, nif_player_pause, 0},
    {"player_toggle", 1, nif_player_toggle, 0},
    {"player_stop", 1, nif_player_stop, 0},
    {"player_next", 1, nif_player_next, 0},
    {"player_previous", 1, nif_player_previous, 0},
    {"player_skip_to", 2, nif_player_skip_to, 0},
    {"player_seek_ms", 2, nif_player_seek_ms, 0},
    {"player_set_volume", 2, nif_player_set_volume, 0},
    {"player_volume", 1, nif_player_volume, 0},
    {"player_sample_rate", 1, nif_player_sample_rate, 0},
    {"player_set_crossfade", 7, nif_player_set_crossfade, 0},
    {"player_set_replaygain", 4, nif_player_set_replaygain, 0},
    {"player_set_shuffle", 2, nif_player_set_shuffle, 0},
    {"player_is_shuffle_enabled", 1, nif_player_is_shuffle_enabled, 0},
    {"player_set_repeat", 2, nif_player_set_repeat, 0},
    {"player_repeat", 1, nif_player_repeat, 0},
    {"player_status_json", 1, nif_player_status_json, 0},
    {"player_set_eq_enabled", 2, nif_player_set_eq_enabled, 0},
    {"player_is_eq_enabled", 1, nif_player_is_eq_enabled, 0},
    {"player_set_eq_band", 5, nif_player_set_eq_band, 0},
    {"player_set_eq_precut", 2, nif_player_set_eq_precut, 0},
    {"player_set_eq_preset", 2, nif_player_set_eq_preset, 0},
    {"player_set_tone", 5, nif_player_set_tone, 0},
    {"player_set_bass", 2, nif_player_set_bass, 0},
    {"player_set_treble", 2, nif_player_set_treble, 0},
    {"player_set_bass_cutoff", 2, nif_player_set_bass_cutoff, 0},
    {"player_set_treble_cutoff", 2, nif_player_set_treble_cutoff, 0},
    {"player_set_crossfeed", 6, nif_player_set_crossfeed, 0},
    {"player_set_bass_enhancement", 3, nif_player_set_bass_enhancement, 0},
    {"player_set_fatigue_reduction", 2, nif_player_set_fatigue_reduction, 0},
    {"player_set_surround", 5, nif_player_set_surround, 0},
    {"player_set_channel_mode", 2, nif_player_set_channel_mode, 0},
    {"player_set_stereo_width", 2, nif_player_set_stereo_width, 0},
    {"player_set_compressor", 7, nif_player_set_compressor, 0},
    {"player_set_dither", 2, nif_player_set_dither, 0},
    {"player_set_pitch", 2, nif_player_set_pitch, 0},
    {"player_dsp_settings_json", 1, nif_player_dsp_settings_json, 0},
    {"player_new_with_config_ex", 14, nif_player_new_with_config_ex,
     ERL_NIF_DIRTY_JOB_CPU_BOUND},
    {"player_insert_json", 4, nif_player_insert_json, 0},
    {"player_queue_json", 1, nif_player_queue_json, 0},
    {"player_resume", 1, nif_player_resume, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"player_save_resume", 1, nif_player_save_resume, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"player_clear_resume", 1, nif_player_clear_resume,
     ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"load_resume_json", 1, nif_load_resume_json, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"player_import_m3u", 4, nif_player_import_m3u, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"player_load_m3u", 2, nif_player_load_m3u, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"player_export_m3u", 2, nif_player_export_m3u, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"m3u_read_json", 1, nif_m3u_read_json, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"m3u_write_json", 2, nif_m3u_write_json, ERL_NIF_DIRTY_JOB_IO_BOUND},
    {"is_url", 1, nif_is_url, 0},
};

ERL_NIF_INIT(rockbox_ffi_nif, funcs, load, NULL, NULL, NULL)
