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
         player_set_shuffle/2, player_is_shuffle_enabled/1, player_set_repeat/2,
         player_repeat/1,
         player_status_json/1,
         player_set_eq_enabled/2, player_is_eq_enabled/1, player_set_eq_band/5,
         player_set_eq_precut/2, player_set_eq_preset/2, player_set_tone/5,
         player_set_bass/2, player_set_treble/2, player_set_bass_cutoff/2,
         player_set_treble_cutoff/2, player_set_crossfeed/6,
         player_set_bass_enhancement/3, player_set_fatigue_reduction/2,
         player_set_surround/5,
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
    Base = filename:join(nif_dir(), "rockbox_ffi_nif"),
    SoPath =
        case target_triple() of
            undefined -> Base;
            Target -> resolve_nif(Base, Target)
        end,
    erlang:load_nif(SoPath, 0).

%% Resolve the extension-less path handed to load_nif (which appends the OS
%% suffix). Order of preference:
%%   1. priv/rockbox_ffi_nif-<triple>.so  — a local `make` / elixir_make build
%%   2. priv/rockbox_ffi_nif.so           — the unsuffixed name a plain `make`
%%                                          or elixir_make produces
%%   3. a checksum-verified copy in the user cache, downloaded on first use.
%%
%% Gleam has no build/install hook and the .so (8-16 MB each) is too large to
%% bundle in the 8 MB Hex tarball, so the Gleam package ships only checksums
%% (priv/rockbox_ffi_nif.manifest) and fetches the matching NIF from the GitHub
%% release the first time it loads. The Elixir binding always has the .so in
%% priv (elixir_make downloads it at compile time), so step 3 never runs there.
resolve_nif(Base, Target) ->
    Suffixed = Base ++ "-" ++ Target,
    case filelib:is_regular(Suffixed ++ ".so") of
        true -> Suffixed;
        false ->
            case filelib:is_regular(Base ++ ".so") of
                true -> Base;
                false ->
                    case ensure_cached(Target) of
                        {ok, Path} -> Path;
                        {error, _} -> Base   %% load_nif fails -> stubs raise
                    end
            end
    end.

%% Directory holding the NIF .so — the OTP app's priv, or (raw Gleam build)
%% ../priv relative to the .beam, or ./priv.
nif_dir() ->
    case code:priv_dir(?MODULE) of
        {error, _} ->
            case code:which(?MODULE) of
                Beam when is_list(Beam) ->
                    filename:join([filename:dirname(Beam), "..", "priv"]);
                _ ->
                    "priv"
            end;
        Dir ->
            Dir
    end.

%% Canonical target triple matching the priv/rockbox_ffi_nif-<triple>.so names
%% (same triples cc_precompiler uses for the Elixir binding). `undefined` on a
%% platform we don't ship a prebuilt for — the unsuffixed fallback is tried.
target_triple() ->
    Arch = normalize_arch(hd(string:lexemes(
        erlang:system_info(system_architecture), "-"))),
    case os:type() of
        {unix, darwin} -> Arch ++ "-apple-darwin";
        {unix, linux} -> Arch ++ "-linux-gnu";
        {unix, freebsd} -> Arch ++ "-unknown-freebsd";
        {unix, netbsd} -> Arch ++ "-unknown-netbsd";
        _ -> undefined
    end.

normalize_arch("amd64") -> "x86_64";
normalize_arch("arm64") -> "aarch64";
normalize_arch(Arch) -> Arch.

%% --- First-use NIF download (Gleam) -------------------------------------------
%% The Gleam package ships priv/rockbox_ffi_nif.manifest (repo, release tag and
%% one sha256 per target triple) instead of the multi-megabyte .so files. On
%% first load the matching NIF is fetched from the GitHub release into the user
%% cache dir, verified against the manifest checksum, and reused thereafter.

ensure_cached(Target) ->
    case read_manifest() of
        {error, R} -> {error, R};
        {ok, Terms} ->
            case {manifest_tag(Terms), manifest_checksum(Terms, Target)} of
                {undefined, _} -> {error, no_tag};
                {_, undefined} -> {error, {no_checksum, Target}};
                {Tag, Sha} ->
                    Repo = manifest_repo(Terms),
                    File = "rockbox_ffi_nif-" ++ Target ++ ".so",
                    Dir = filename:join(cache_root(), Tag),
                    Dest = filename:join(Dir, File),
                    ExtLess = filename:join(Dir, "rockbox_ffi_nif-" ++ Target),
                    case filelib:is_regular(Dest) of
                        true ->
                            {ok, ExtLess};
                        false ->
                            Url = "https://github.com/" ++ Repo ++
                                  "/releases/download/" ++ Tag ++ "/" ++ File,
                            case fetch_verify_write(Url, Dest, Dir, Sha) of
                                ok -> {ok, ExtLess};
                                {error, R} -> {error, R}
                            end
                    end
            end
    end.

read_manifest() ->
    file:consult(filename:join(nif_dir(), "rockbox_ffi_nif.manifest")).

manifest_tag(Terms) ->
    case lists:keyfind(tag, 1, Terms) of {tag, V} -> V; false -> undefined end.

manifest_repo(Terms) ->
    case lists:keyfind(repo, 1, Terms) of
        {repo, V} -> V;
        false -> "tsirysndr/rockboxd"
    end.

manifest_checksum(Terms, Target) ->
    case [S || {checksum, T, S} <- Terms, T =:= Target] of
        [Sha | _] -> Sha;
        [] -> undefined
    end.

cache_root() ->
    filename:basedir(user_cache, "rockbox_ffi").

fetch_verify_write(Url, Dest, Dir, Sha) ->
    _ = application:ensure_all_started(crypto),
    _ = application:ensure_all_started(inets),
    _ = application:ensure_all_started(ssl),
    HttpOpts = [{timeout, 120000}, {connect_timeout, 15000},
                {autoredirect, true}, {ssl, tls_opts()}],
    Req = {Url, [{"user-agent", "rockbox_ffi_nif"}]},
    case httpc:request(get, Req, HttpOpts, [{body_format, binary}]) of
        {ok, {{_, 200, _}, _Hdrs, Body}} ->
            case sha256_hex(Body) of
                Sha ->
                    ok = filelib:ensure_dir(filename:join(Dir, "keep")),
                    Tmp = Dest ++ ".download",
                    case file:write_file(Tmp, Body) of
                        ok ->
                            _ = file:change_mode(Tmp, 8#755),
                            file:rename(Tmp, Dest);
                        {error, R} -> {error, R}
                    end;
                Got ->
                    {error, {checksum_mismatch, [{want, Sha}, {got, Got}]}}
            end;
        {ok, {{_, Code, _}, _, _}} -> {error, {http_status, Code}};
        {error, R} -> {error, R}
    end.

%% Verify the TLS chain when the platform exposes the OS trust store (OTP 25+),
%% otherwise skip peer verification — payload integrity is guaranteed by the
%% sha256 check regardless, since the checksum ships inside the signed Hex tarball.
tls_opts() ->
    try public_key:cacerts_get() of
        Certs ->
            [{verify, verify_peer}, {depth, 99}, {cacerts, Certs},
             {customize_hostname_check,
              [{match_fun, public_key:pkix_verify_hostname_match_fun(https)}]}]
    catch
        _:_ -> [{verify, verify_none}]
    end.

sha256_hex(Bin) ->
    lists:flatten([io_lib:format("~2.16.0b", [B])
                   || <<B>> <= crypto:hash(sha256, Bin)]).

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
player_set_shuffle(_P, _Enabled) -> ?NOT_LOADED.
player_is_shuffle_enabled(_P) -> ?NOT_LOADED.
player_set_repeat(_P, _Mode) -> ?NOT_LOADED.
player_repeat(_P) -> ?NOT_LOADED.
player_status_json(_P) -> ?NOT_LOADED.
player_set_eq_enabled(_P, _En) -> ?NOT_LOADED.
player_is_eq_enabled(_P) -> ?NOT_LOADED.
player_set_eq_band(_P, _Band, _Cutoff, _Q, _Gain) -> ?NOT_LOADED.
player_set_eq_precut(_P, _Db) -> ?NOT_LOADED.
player_set_eq_preset(_P, _Preset) -> ?NOT_LOADED.
player_set_tone(_P, _Bass, _Treble, _BassCut, _TrebleCut) -> ?NOT_LOADED.
player_set_bass(_P, _Bass) -> ?NOT_LOADED.
player_set_treble(_P, _Treble) -> ?NOT_LOADED.
player_set_bass_cutoff(_P, _Hz) -> ?NOT_LOADED.
player_set_treble_cutoff(_P, _Hz) -> ?NOT_LOADED.
player_set_crossfeed(_P, _Mode, _Direct, _Cross, _Hf, _HfCutoff) -> ?NOT_LOADED.
player_set_bass_enhancement(_P, _Strength, _Precut) -> ?NOT_LOADED.
player_set_fatigue_reduction(_P, _Strength) -> ?NOT_LOADED.
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
