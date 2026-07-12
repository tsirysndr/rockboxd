%%% rockbox_ffi_nif — Erlang NIF loader for the librockbox_ffi C ABI.
%%%
%%% SHARED between the Elixir and Gleam bindings (kept identical). Both
%%% languages call these functions; the ergonomic wrappers live in their
%%% respective source trees. Every function here is a NIF — the bodies below
%%% only run if the .so failed to load.
-module(rockbox_ffi_nif).

-export([abi_version/0,
         meta_read_json/1, meta_probe/1,
         dsp_new/1, dsp_set_input_frequency/2, dsp_flush/1, dsp_eq_enable/2,
         dsp_set_tone/3, dsp_set_tone_cutoffs/3, dsp_set_surround/5,
         dsp_set_channel_config/2, dsp_set_stereo_width/2, dsp_set_compressor/7,
         dsp_set_replaygain/4, dsp_set_replaygain_gains/5,
         dsp_set_replaygain_gains_raw/5, dsp_set_eq_band/5, dsp_set_eq_precut/2,
         dsp_process/2,
         player_new/0, player_new_with_config/12, player_set_queue_json/2,
         player_enqueue/2, player_play/1, player_pause/1, player_toggle/1,
         player_stop/1, player_next/1, player_previous/1, player_skip_to/2,
         player_seek_ms/2, player_set_volume/2, player_volume/1,
         player_sample_rate/1, player_set_crossfade/7, player_set_replaygain/4,
         player_status_json/1,
         player_set_eq_enabled/2, player_is_eq_enabled/1, player_set_eq_band/5,
         player_set_eq_precut/2, player_set_eq_preset/2, player_set_tone/5,
         player_set_bass/2, player_set_treble/2, player_set_surround/5,
         player_set_channel_mode/2, player_set_stereo_width/2,
         player_set_compressor/7, player_set_dither/2, player_set_pitch/2,
         player_dsp_settings_json/1,
         player_new_with_config_ex/14, player_insert_json/4, player_queue_json/1,
         player_resume/1, player_save_resume/1, player_clear_resume/1,
         load_resume_json/1, player_import_m3u/4, player_load_m3u/2,
         player_export_m3u/2, m3u_read_json/1, m3u_write_json/2, is_url/1]).

-on_load(init/0).

-define(NOT_LOADED, erlang:nif_error({nif_not_loaded, ?MODULE})).

init() ->
    SoName =
        case code:priv_dir(?MODULE) of
            {error, _} ->
                %% Not part of an OTP app (e.g. a raw Gleam build): look
                %% beside the .beam file, or in ./priv.
                case code:which(?MODULE) of
                    Beam when is_list(Beam) ->
                        filename:join([filename:dirname(Beam), "..", "priv",
                                       "rockbox_ffi_nif"]);
                    _ ->
                        filename:join(["priv", "rockbox_ffi_nif"])
                end;
            Dir ->
                filename:join(Dir, "rockbox_ffi_nif")
        end,
    erlang:load_nif(SoName, 0).

abi_version() -> ?NOT_LOADED.

meta_read_json(_Path) -> ?NOT_LOADED.
meta_probe(_Name) -> ?NOT_LOADED.

dsp_new(_Rate) -> ?NOT_LOADED.
dsp_set_input_frequency(_D, _Hz) -> ?NOT_LOADED.
dsp_flush(_D) -> ?NOT_LOADED.
dsp_eq_enable(_D, _Enable) -> ?NOT_LOADED.
dsp_set_tone(_D, _Bass, _Treble) -> ?NOT_LOADED.
dsp_set_tone_cutoffs(_D, _Bass, _Treble) -> ?NOT_LOADED.
dsp_set_surround(_D, _Delay, _Balance, _Fx1, _Fx2) -> ?NOT_LOADED.
dsp_set_channel_config(_D, _Mode) -> ?NOT_LOADED.
dsp_set_stereo_width(_D, _Pct) -> ?NOT_LOADED.
dsp_set_compressor(_D, _Th, _Mk, _Ratio, _Knee, _Rel, _Atk) -> ?NOT_LOADED.
dsp_set_replaygain(_D, _Mode, _Noclip, _Preamp) -> ?NOT_LOADED.
dsp_set_replaygain_gains(_D, _Tg, _Ag, _Tp, _Ap) -> ?NOT_LOADED.
dsp_set_replaygain_gains_raw(_D, _Tg, _Ag, _Tp, _Ap) -> ?NOT_LOADED.
dsp_set_eq_band(_D, _Band, _Cutoff, _Q, _Gain) -> ?NOT_LOADED.
dsp_set_eq_precut(_D, _Db) -> ?NOT_LOADED.
dsp_process(_D, _Bin) -> ?NOT_LOADED.

player_new() -> ?NOT_LOADED.
player_new_with_config(_Rate, _Buf, _Vol, _RgMode, _RgPre, _RgClip, _Xf, _FoDel,
                       _FoDur, _FiDel, _FiDur, _Mix) -> ?NOT_LOADED.
player_set_queue_json(_P, _Json) -> ?NOT_LOADED.
player_enqueue(_P, _Path) -> ?NOT_LOADED.
player_play(_P) -> ?NOT_LOADED.
player_pause(_P) -> ?NOT_LOADED.
player_toggle(_P) -> ?NOT_LOADED.
player_stop(_P) -> ?NOT_LOADED.
player_next(_P) -> ?NOT_LOADED.
player_previous(_P) -> ?NOT_LOADED.
player_skip_to(_P, _Idx) -> ?NOT_LOADED.
player_seek_ms(_P, _Ms) -> ?NOT_LOADED.
player_set_volume(_P, _Vol) -> ?NOT_LOADED.
player_volume(_P) -> ?NOT_LOADED.
player_sample_rate(_P) -> ?NOT_LOADED.
player_set_crossfade(_P, _Mode, _FoDel, _FoDur, _FiDel, _FiDur, _Mix) -> ?NOT_LOADED.
player_set_replaygain(_P, _Mode, _Preamp, _Clip) -> ?NOT_LOADED.
player_status_json(_P) -> ?NOT_LOADED.
player_set_eq_enabled(_P, _En) -> ?NOT_LOADED.
player_is_eq_enabled(_P) -> ?NOT_LOADED.
player_set_eq_band(_P, _Band, _Cutoff, _Q, _Gain) -> ?NOT_LOADED.
player_set_eq_precut(_P, _Db) -> ?NOT_LOADED.
player_set_eq_preset(_P, _Preset) -> ?NOT_LOADED.
player_set_tone(_P, _Bass, _Treble, _BassCut, _TrebleCut) -> ?NOT_LOADED.
player_set_bass(_P, _Bass) -> ?NOT_LOADED.
player_set_treble(_P, _Treble) -> ?NOT_LOADED.
player_set_surround(_P, _Delay, _Balance, _Low, _High) -> ?NOT_LOADED.
player_set_channel_mode(_P, _Mode) -> ?NOT_LOADED.
player_set_stereo_width(_P, _Pct) -> ?NOT_LOADED.
player_set_compressor(_P, _Th, _Mk, _Ratio, _Knee, _Atk, _Rel) -> ?NOT_LOADED.
player_set_dither(_P, _En) -> ?NOT_LOADED.
player_set_pitch(_P, _Ratio) -> ?NOT_LOADED.
player_dsp_settings_json(_P) -> ?NOT_LOADED.
player_new_with_config_ex(_Rate, _Buf, _Vol, _RgMode, _RgPre, _RgClip, _Xf,
                          _FoDel, _FoDur, _FiDel, _FiDur, _Mix, _ResumeFile,
                          _ResumeInt) -> ?NOT_LOADED.
player_insert_json(_P, _Json, _Position, _Index) -> ?NOT_LOADED.
player_queue_json(_P) -> ?NOT_LOADED.
player_resume(_P) -> ?NOT_LOADED.
player_save_resume(_P) -> ?NOT_LOADED.
player_clear_resume(_P) -> ?NOT_LOADED.
load_resume_json(_Path) -> ?NOT_LOADED.
player_import_m3u(_P, _Path, _Position, _Index) -> ?NOT_LOADED.
player_load_m3u(_P, _Path) -> ?NOT_LOADED.
player_export_m3u(_P, _Path) -> ?NOT_LOADED.
m3u_read_json(_Path) -> ?NOT_LOADED.
m3u_write_json(_Path, _Json) -> ?NOT_LOADED.
is_url(_S) -> ?NOT_LOADED.
