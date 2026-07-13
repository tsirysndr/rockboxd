%%% rockbox_ffi_nif — Erlang NIF loader for the librockbox_ffi C ABI.
%%%
%%% SHARED between the Elixir and Gleam bindings (kept identical). Both
%%% languages call these functions; the ergonomic wrappers live in their
%%% respective source trees. Every function here is a NIF — the bodies below
%%% only run if the .so failed to load.
-module(rockbox_ffi_nif).

-moduledoc """
Raw Erlang NIF surface over the `librockbox_ffi` C ABI (Rockbox's metadata
parser, codec engine, DSP pipeline and queue-based player).

This is a faithful, low-level 1:1 binding — it takes and returns primitive
BEAM terms and does no argument massaging:

- **Strings/paths** are passed and returned as **UTF-8 binaries**, never
  charlists. `nil` is returned in place of a NULL C string.
- **JSON-returning** functions (`*_json`) hand back the raw JSON **binary**;
  the caller decodes it.
- **Handles** (`RbDsp`, `RbPlayer`, `RbDecoder`) are opaque NIF resources
  (`reference()`) freed automatically by the BEAM garbage collector — there is
  no explicit close/free. Dropping the last reference stops playback / frees
  the codec.
- **PCM** is an interleaved-stereo little-endian `int16` binary.
- Passing a wrong-typed argument (or a handle of the wrong kind) raises
  `badarg`. Every function raises `{nif_not_loaded, ?MODULE}` if the native
  library failed to load.

Ergonomic wrappers with atom keyword configs, enum atoms and pipe-friendly
returns live in the language layers (`Rockbox.*` for Elixir, the `rockbox`
package for Gleam) — prefer those in application code. The functions here are
shared verbatim between both bindings.

## Example — decode a file to PCM

```erlang
D = rockbox_ffi_nif:decoder_open(<<"/music/song.flac">>),
Meta = rockbox_ffi_nif:decoder_metadata_json(D),
Loop = fun Loop(Acc) ->
    case rockbox_ffi_nif:decoder_next_chunk(D) of
        nil -> lists:reverse(Acc);
        {Pcm, _Rate} -> Loop([Pcm | Acc])
    end
end,
Chunks = Loop([]).
```

## Example — play a queue

```erlang
P = rockbox_ffi_nif:player_new(),
rockbox_ffi_nif:player_set_queue_json(P, <<"[\\"/music/a.mp3\\",\\"/music/b.mp3\\"]">>),
rockbox_ffi_nif:player_play(P).
```
""".

-export([abi_version/0,
         meta_read_json/1, meta_probe/1,
         decoder_open/1, decoder_metadata_json/1, decoder_next_chunk/1,
         decoder_seek_ms/2, decoder_elapsed_ms/1, decoder_finished/1,
         dsp_new/1, dsp_set_input_frequency/2, dsp_flush/1, dsp_eq_enable/2,
         dsp_set_tone/3, dsp_set_tone_cutoffs/3, dsp_set_surround/5,
         dsp_set_channel_config/2, dsp_set_stereo_width/2, dsp_set_compressor/7,
         dsp_set_replaygain/4, dsp_set_replaygain_gains/5,
         dsp_set_replaygain_gains_raw/5, dsp_set_eq_band/5, dsp_set_eq_precut/2,
         dsp_process/2,
         player_new/0, player_new_with_config/12, player_set_queue_json/2,
         player_enqueue/2, player_remove/2, player_clear_queue/1,
         player_play/1, player_pause/1, player_toggle/1,
         player_stop/1, player_next/1, player_previous/1, player_skip_to/2,
         player_seek_ms/2, player_set_volume/2, player_set_balance/2,
         player_volume/1, player_balance/1,
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

-doc """
The ABI version of the loaded native library, as an integer.

Bump-on-break: a mismatch against the version a binding was built for means
the `.so` and the Erlang surface have drifted.

```erlang
1 = rockbox_ffi_nif:abi_version().
```
""".
abi_version() -> ?NOT_LOADED.

%% ---- metadata --------------------------------------------------------------

-doc """
Parse the tags and stream properties of the audio file at `Path` (a binary).

Returns a JSON binary — an object with keys such as `codec`, `title`,
`artist`, `duration_ms`, `sample_rate` and a nested `replaygain` object — or
`nil` if the file cannot be parsed. Runs on a dirty IO scheduler.

```erlang
Json = rockbox_ffi_nif:meta_read_json(<<"/music/song.flac">>),
#{<<"title">> := Title} = json:decode(Json).
```
""".
meta_read_json(_Path) -> ?NOT_LOADED.

-doc """
Guess the codec label (e.g. `<<"FLAC">>`) from a filename's extension without
opening the file. Returns `nil` for an unrecognised extension.

```erlang
<<"FLAC">> = rockbox_ffi_nif:meta_probe(<<"track.flac">>),
nil = rockbox_ffi_nif:meta_probe(<<"notes.txt">>).
```
""".
meta_probe(_Name) -> ?NOT_LOADED.

%% ---- decoder (codec engine) ------------------------------------------------

-doc """
Open a decoder for the local audio file at `Path` (a binary).

Returns an opaque decoder handle, or `nil` if the file cannot be opened or no
codec recognises it. The codec engine is process-wide: only one decoder can
decode at a time — opening a second blocks until the first handle is
garbage-collected. Runs on a dirty CPU scheduler.

```erlang
D = rockbox_ffi_nif:decoder_open(<<"/music/song.mp3">>).
```
""".
decoder_open(_Path) -> ?NOT_LOADED.

-doc """
Tags and stream properties for an open decoder, as a JSON binary (same shape
as `meta_read_json/1`). Runs on a dirty IO scheduler.

```erlang
Json = rockbox_ffi_nif:decoder_metadata_json(D).
```
""".
decoder_metadata_json(_D) -> ?NOT_LOADED.

-doc """
Decode the next buffer of PCM.

Returns `{Pcm, SampleRate}` where `Pcm` is an interleaved-stereo little-endian
`int16` binary and `SampleRate` is in Hz, or `nil` at end of track (or after a
codec error — check `decoder_finished/1`). Runs on a dirty CPU scheduler.

```erlang
case rockbox_ffi_nif:decoder_next_chunk(D) of
    {Pcm, Rate} -> handle(Pcm, Rate);
    nil         -> eof
end.
```
""".
decoder_next_chunk(_D) -> ?NOT_LOADED.

-doc """
Request a seek to `Ms` milliseconds into the track. Returns `nil`.

```erlang
rockbox_ffi_nif:decoder_seek_ms(D, 30000).
```
""".
decoder_seek_ms(_D, _Ms) -> ?NOT_LOADED.

-doc """
Playback position last reported by the codec, in milliseconds.

```erlang
Ms = rockbox_ffi_nif:decoder_elapsed_ms(D).
```
""".
decoder_elapsed_ms(_D) -> ?NOT_LOADED.

-doc """
End-of-track state as `{Done, Code}`.

`Done` is `true` once the track has finished. `Code` is the exit status
(`0` = clean end, negative = codec error) and is only meaningful when `Done`
is `true`.

```erlang
{true, 0} = rockbox_ffi_nif:decoder_finished(D).
```
""".
decoder_finished(_D) -> ?NOT_LOADED.

%% ---- DSP -------------------------------------------------------------------

-doc """
Create a DSP pipeline for interleaved S16LE stereo at `Rate` Hz.

Returns an opaque DSP handle, or `nil` on failure. The DSP wraps a
process-wide singleton, so only one handle should be alive at a time.

```erlang
Dsp = rockbox_ffi_nif:dsp_new(44100).
```
""".
dsp_new(_Rate) -> ?NOT_LOADED.

-doc """
Set the input sample rate (`Hz`) the pipeline resamples from. Returns `nil`.
""".
dsp_set_input_frequency(_D, _Hz) -> ?NOT_LOADED.

-doc """
Flush internal DSP buffers (e.g. after a seek). Returns `nil`.
""".
dsp_flush(_D) -> ?NOT_LOADED.

-doc """
Enable (`true`) or disable (`false`) the equalizer. Returns `nil`.
""".
dsp_eq_enable(_D, _Enable) -> ?NOT_LOADED.

-doc """
Set the bass and treble tone axes, both in dB. Returns `nil`.

```erlang
rockbox_ffi_nif:dsp_set_tone(Dsp, 3, -2).
```
""".
dsp_set_tone(_D, _Bass, _Treble) -> ?NOT_LOADED.

-doc """
Set the bass and treble tone shelf cutoff frequencies, in Hz (`0` = the native
defaults). Returns `nil`.
""".
dsp_set_tone_cutoffs(_D, _Bass, _Treble) -> ?NOT_LOADED.

-doc """
Haas surround effect: `Delay` in ms (`0` = off), `Balance` in percent, and the
two band-split cutoff/effect parameters `Fx1`/`Fx2`. Returns `nil`.
""".
dsp_set_surround(_D, _Delay, _Balance, _Fx1, _Fx2) -> ?NOT_LOADED.

-doc """
Set the channel-mixing mode (integer code: e.g. `0` stereo, mono, swap, …).
Returns `nil`.
""".
dsp_set_channel_config(_D, _Mode) -> ?NOT_LOADED.

-doc """
Set the custom stereo width, in percent. Returns `nil`.
""".
dsp_set_stereo_width(_D, _Pct) -> ?NOT_LOADED.

-doc """
Dynamic-range compressor. Arguments in order: threshold (`Th`, dB; `0`
disables), makeup gain (`Mk`), ratio, knee, release time (`Rel`) and attack
time (`Atk`). Returns `nil`.

Note the argument order (release before attack) differs from
`player_set_compressor/7`.
""".
dsp_set_compressor(_D, _Th, _Mk, _Ratio, _Knee, _Rel, _Atk) -> ?NOT_LOADED.

-doc """
Configure ReplayGain. `Mode` uses the DSP-native codes (`0` track, `1` album,
`2` shuffle, `3` off), `Noclip` is clipping prevention (boolean), `Preamp` is
the pre-amplification in dB. Returns `nil`.
""".
dsp_set_replaygain(_D, _Mode, _Noclip, _Preamp) -> ?NOT_LOADED.

-doc """
Supply the current track's ReplayGain values: track/album gains in plain dB
(`Tg`/`Ag`) and track/album peaks as linear amplitude (`Tp`/`Ap`). Pass the
atom `nil` for any absent tag. Returns `nil`.
""".
dsp_set_replaygain_gains(_D, _Tg, _Ag, _Tp, _Ap) -> ?NOT_LOADED.

-doc """
Like `dsp_set_replaygain_gains/5` but with native Q7.24 fixed-point integers
(the `raw_*` metadata fields); `0` means "not tagged". Returns `nil`.
""".
dsp_set_replaygain_gains_raw(_D, _Tg, _Ag, _Tp, _Ap) -> ?NOT_LOADED.

-doc """
Configure one EQ band. `Band` is the band index (0-based), `Cutoff` the centre
frequency in Hz, `Q` the Q factor, `Gain` the gain in dB. Returns `nil`.

```erlang
rockbox_ffi_nif:dsp_set_eq_band(Dsp, 4, 1000, 1.0, 6.0).
```
""".
dsp_set_eq_band(_D, _Band, _Cutoff, _Q, _Gain) -> ?NOT_LOADED.

-doc """
Set the global EQ pre-cut (attenuation applied before the bands), in dB.
Returns `nil`.
""".
dsp_set_eq_precut(_D, _Db) -> ?NOT_LOADED.

-doc """
Run interleaved-stereo S16LE PCM through the pipeline.

`Bin` is a little-endian `int16` binary; the result is the processed PCM as a
binary (possibly empty, and not necessarily the same length as the input).
Runs on a dirty CPU scheduler.

```erlang
Out = rockbox_ffi_nif:dsp_process(Dsp, In).
```
""".
dsp_process(_D, _Bin) -> ?NOT_LOADED.

%% ---- player: lifecycle & transport ----------------------------------------

-doc """
Create a player on the default output device with default settings.

Returns an opaque player handle, or `nil` if no output device is available. A
player owns a live audio device and a background engine thread; dropping the
last reference stops playback. Runs on a dirty CPU scheduler.

```erlang
P = rockbox_ffi_nif:player_new().
```
""".
player_new() -> ?NOT_LOADED.

-doc """
Create a player with explicit configuration.

Arguments: `Rate` (output sample rate in Hz; `0` = device default), `Buf`
(buffer length in seconds), `Vol` (initial volume, `1.0` = unity), `RgMode`
(player ReplayGain mode: `0` off, `1` track, `2` album), `RgPre` (ReplayGain
preamp in dB), `RgClip` (prevent clipping, boolean), `Xf` (crossfade mode:
`0` off … `5` always), fade-out delay/duration and fade-in delay/duration in
ms (`FoDel`/`FoDur`/`FiDel`/`FiDur`), and `Mix` (`0` crossfade, `1` mix).

Returns a player handle or `nil`. Runs on a dirty CPU scheduler.
""".
player_new_with_config(_Rate, _Buf, _Vol, _RgMode, _RgPre, _RgClip, _Xf, _FoDel,
                       _FoDur, _FiDel, _FiDur, _Mix) -> ?NOT_LOADED.

-doc """
Replace the queue from a JSON array of path/URL strings. Returns `nil`.

Each entry may be a local file path, an `http(s)://` URL to a finite remote
file, or a live streaming / radio URL.

```erlang
rockbox_ffi_nif:player_set_queue_json(P, <<"[\\"/music/a.mp3\\"]">>).
```
""".
player_set_queue_json(_P, _Json) -> ?NOT_LOADED.

-doc """
Append one track (`Path`, a binary path or URL) to the queue. Returns `nil`.
""".
player_enqueue(_P, _Path) -> ?NOT_LOADED.

-doc """
Remove the track at zero-based `Idx` from the queue. An out-of-range index is
ignored. Removing a track before the current one keeps the current track
playing; removing the currently-playing track hard-cuts to the track that
slides into its place; removing the last remaining track stops playback.
Returns `nil`.
""".
player_remove(_P, _Idx) -> ?NOT_LOADED.

-doc """
Empty the queue and stop playback (also clears any saved resume state).
Returns `nil`.
""".
player_clear_queue(_P) -> ?NOT_LOADED.

-doc "Start (or resume) playback of the current queue. Returns `nil`.".
player_play(_P) -> ?NOT_LOADED.

-doc "Pause playback. Returns `nil`.".
player_pause(_P) -> ?NOT_LOADED.

-doc "Toggle between play and pause. Returns `nil`.".
player_toggle(_P) -> ?NOT_LOADED.

-doc "Stop playback and reset the play position. Returns `nil`.".
player_stop(_P) -> ?NOT_LOADED.

-doc "Skip to the next track in the queue. Returns `nil`.".
player_next(_P) -> ?NOT_LOADED.

-doc "Skip to the previous track in the queue. Returns `nil`.".
player_previous(_P) -> ?NOT_LOADED.

-doc """
Jump to the queue entry at zero-based `Idx` and play it. Returns `nil`.
""".
player_skip_to(_P, _Idx) -> ?NOT_LOADED.

-doc "Seek to `Ms` milliseconds within the current track. Returns `nil`.".
player_seek_ms(_P, _Ms) -> ?NOT_LOADED.

-doc "Set the output volume (`Vol`, a float; `1.0` = unity gain). Returns `nil`.".
player_set_volume(_P, _Vol) -> ?NOT_LOADED.

-doc """
Set the stereo balance: `-100` (full left) to `+100` (full right), `0` centred.
Returns `nil`.
""".
player_set_balance(_P, _Balance) -> ?NOT_LOADED.

-doc "The current output volume as a float.".
player_volume(_P) -> ?NOT_LOADED.

-doc "The current stereo balance as an integer (`-100`..`+100`).".
player_balance(_P) -> ?NOT_LOADED.

-doc "The player's output sample rate, in Hz.".
player_sample_rate(_P) -> ?NOT_LOADED.

-doc """
Reconfigure crossfade. `Mode` `0` off … `5` always; `FoDel`/`FoDur` fade-out
delay/duration and `FiDel`/`FiDur` fade-in delay/duration in ms; `Mix` `0`
crossfade, `1` mix. Returns `nil`.
""".
player_set_crossfade(_P, _Mode, _FoDel, _FoDur, _FiDel, _FiDur, _Mix) -> ?NOT_LOADED.

-doc """
Reconfigure ReplayGain. `Mode` `0` off, `1` track, `2` album; `Preamp` in dB;
`Clip` prevents clipping (boolean). Returns `nil`.
""".
player_set_replaygain(_P, _Mode, _Preamp, _Clip) -> ?NOT_LOADED.

-doc "Enable (`true`) or disable (`false`) shuffle playback. Returns `nil`.".
player_set_shuffle(_P, _Enabled) -> ?NOT_LOADED.

-doc "Whether shuffle is currently enabled (`true`/`false`).".
player_is_shuffle_enabled(_P) -> ?NOT_LOADED.

-doc "Set the repeat mode (`0` off, `1` one, `2` all). Returns `nil`.".
player_set_repeat(_P, _Mode) -> ?NOT_LOADED.

-doc "The current repeat mode as an integer (`0` off, `1` one, `2` all).".
player_repeat(_P) -> ?NOT_LOADED.

-doc """
A snapshot of the player's status as a JSON binary (playback state, current
track, elapsed/total ms, volume, …).

```erlang
Json = rockbox_ffi_nif:player_status_json(P).
```
""".
player_status_json(_P) -> ?NOT_LOADED.

%% ---- player: DSP chain -----------------------------------------------------

-doc "Enable (`true`) or disable (`false`) the equalizer. Returns `nil`.".
player_set_eq_enabled(_P, _En) -> ?NOT_LOADED.

-doc "Whether the equalizer is currently enabled (`true`/`false`).".
player_is_eq_enabled(_P) -> ?NOT_LOADED.

-doc """
Configure one EQ band: `Band` index (0..10), `Cutoff` centre frequency in Hz,
`Q` factor, `Gain` in dB. Returns `nil`.

```erlang
rockbox_ffi_nif:player_set_eq_band(P, 5, 2000, 1.0, -3.0).
```
""".
player_set_eq_band(_P, _Band, _Cutoff, _Q, _Gain) -> ?NOT_LOADED.

-doc "Set the global EQ pre-cut, in dB. Returns `nil`.".
player_set_eq_precut(_P, _Db) -> ?NOT_LOADED.

-doc "Apply a built-in EQ preset by integer code (e.g. rock, jazz, …). Returns `nil`.".
player_set_eq_preset(_P, _Preset) -> ?NOT_LOADED.

-doc """
Bass/treble tone controls: `Bass`/`Treble` gains in dB, `BassCut`/`TrebleCut`
shelf cutoffs in Hz (`0` = native defaults). Returns `nil`.
""".
player_set_tone(_P, _Bass, _Treble, _BassCut, _TrebleCut) -> ?NOT_LOADED.

-doc "Set the bass tone axis in dB (treble and cutoffs unchanged). Returns `nil`.".
player_set_bass(_P, _Bass) -> ?NOT_LOADED.

-doc "Set the treble tone axis in dB (bass and cutoffs unchanged). Returns `nil`.".
player_set_treble(_P, _Treble) -> ?NOT_LOADED.

-doc "Override the bass shelf cutoff in Hz (`0` = native default). Returns `nil`.".
player_set_bass_cutoff(_P, _Hz) -> ?NOT_LOADED.

-doc "Override the treble shelf cutoff in Hz (`0` = native default). Returns `nil`.".
player_set_treble_cutoff(_P, _Hz) -> ?NOT_LOADED.

-doc """
Headphone crossfeed. `Mode` `0` off, `1` Meier, `2` custom. `Direct`, `Cross`
and `Hf` gains are in tenths of a dB (`<= 0`); `HfCutoff` in Hz. The cross /
high-frequency parameters only apply in custom mode. Returns `nil`.
""".
player_set_crossfeed(_P, _Mode, _Direct, _Cross, _Hf, _HfCutoff) -> ?NOT_LOADED.

-doc """
Perceptual bass enhancement: `Strength` as a percentage (`0` = off), `Precut`
in tenths of a dB (`<= 0`). Returns `nil`.
""".
player_set_bass_enhancement(_P, _Strength, _Precut) -> ?NOT_LOADED.

-doc "Auditory fatigue reduction: `0` off, `1` weak, `2` moderate, `3` strong. Returns `nil`.".
player_set_fatigue_reduction(_P, _Strength) -> ?NOT_LOADED.

-doc """
Haas surround effect: `Delay` in ms (`0` = off), `Balance` in percent, and the
`Low`/`High` band-split cutoffs in Hz. Returns `nil`.
""".
player_set_surround(_P, _Delay, _Balance, _Low, _High) -> ?NOT_LOADED.

-doc "Set the channel-mixing mode by integer code (`0` stereo … `6`). Returns `nil`.".
player_set_channel_mode(_P, _Mode) -> ?NOT_LOADED.

-doc "Custom stereo width in percent (audible with the custom channel mode). Returns `nil`.".
player_set_stereo_width(_P, _Pct) -> ?NOT_LOADED.

-doc """
Dynamic-range compressor. Arguments in order: threshold (`Th`, dB; `0`
disables), makeup gain (`Mk`), ratio, knee, attack time (`Atk`) and release
time (`Rel`). Returns `nil`.

Note the argument order (attack before release) differs from
`dsp_set_compressor/7`.
""".
player_set_compressor(_P, _Th, _Mk, _Ratio, _Knee, _Atk, _Rel) -> ?NOT_LOADED.

-doc "Enable (`true`) or disable (`false`) output dithering + noise shaping. Returns `nil`.".
player_set_dither(_P, _En) -> ?NOT_LOADED.

-doc """
Pitch/speed ratio (`10000` = normal; pitch and tempo shift together). Returns
`nil`.
""".
player_set_pitch(_P, _Ratio) -> ?NOT_LOADED.

-doc """
The full DSP-chain state as a JSON binary (EQ, tone, surround, compressor,
ReplayGain, …).
""".
player_dsp_settings_json(_P) -> ?NOT_LOADED.

%% ---- player: queue, resume, playlists --------------------------------------

-doc """
Like `player_new_with_config/12` plus resume support.

`ResumeFile` is a binary `.m3u8` path to auto-persist the queue + exact
position to (or the atom `nil` for none), and `ResumeInt` the save interval in
ms (`0` = the native default). Restore later with `player_resume/1`. Returns a
player handle or `nil`. Runs on a dirty CPU scheduler.
""".
player_new_with_config_ex(_Rate, _Buf, _Vol, _RgMode, _RgPre, _RgClip, _Xf,
                          _FoDel, _FoDur, _FiDel, _FiDur, _Mix, _ResumeFile,
                          _ResumeInt) -> ?NOT_LOADED.

-doc """
Insert tracks (a JSON array of path/URL strings) into the queue at `Position`
(insert-position code `0`..`7`); `Index` is only used when `Position` is
`7` (insert-at-index). Returns `nil`.
""".
player_insert_json(_P, _Json, _Position, _Index) -> ?NOT_LOADED.

-doc "The current queue as a JSON array of path/URL strings (binary), or `nil`.".
player_queue_json(_P) -> ?NOT_LOADED.

-doc """
Restore the queue + exact position from the player's resume file (does NOT
auto-play).

Returns a JSON binary (`{"tracks": [...], "index": i, "elapsed_ms": ms}`), or
`nil` when there is nothing to resume. Runs on a dirty IO scheduler.
""".
player_resume(_P) -> ?NOT_LOADED.

-doc "Persist the queue + exact position to the resume file now. Returns `nil`. Dirty IO.".
player_save_resume(_P) -> ?NOT_LOADED.

-doc "Delete the resume file (forget the saved state). Returns `nil`. Dirty IO.".
player_clear_resume(_P) -> ?NOT_LOADED.

-doc """
Read a resume-state file at `Path` (a binary) without a player, returning its
JSON binary (same shape as `player_resume/1`) or `nil`. Runs on a dirty IO
scheduler.
""".
load_resume_json(_Path) -> ?NOT_LOADED.

-doc """
Import an `.m3u`/`.m3u8` playlist at `Path` into the queue at `Position`
(insert-position code; `Index` used only for insert-at-index `7`).

Returns a JSON array of the imported path strings (binary), or `nil` on
failure. Runs on a dirty IO scheduler.
""".
player_import_m3u(_P, _Path, _Position, _Index) -> ?NOT_LOADED.

-doc """
Replace the queue with the `.m3u`/`.m3u8` playlist at `Path`.

Returns a JSON array of the loaded path strings (binary), or `nil` on failure.
Runs on a dirty IO scheduler.
""".
player_load_m3u(_P, _Path) -> ?NOT_LOADED.

-doc """
Export the current queue to an `.m3u8` file at `Path` (atomic write).

Returns an integer status code (`0` = success, non-zero = failure). Runs on a
dirty IO scheduler.
""".
player_export_m3u(_P, _Path) -> ?NOT_LOADED.

%% ---- standalone M3U + URL helpers ------------------------------------------

-doc """
Parse the `.m3u`/`.m3u8` file at `Path` (a binary) into a JSON array of
path/URL strings (binary), or `nil` on failure. Runs on a dirty IO scheduler.
""".
m3u_read_json(_Path) -> ?NOT_LOADED.

-doc """
Write a JSON array of path/URL strings (`Json`) to the `.m3u8` file at `Path`
(atomic). Returns an integer status code (`0` = success). Runs on a dirty IO
scheduler.
""".
m3u_write_json(_Path, _Json) -> ?NOT_LOADED.

-doc """
Whether `S` (a binary) looks like a playable URL rather than a local path.

```erlang
true  = rockbox_ffi_nif:is_url(<<"http://host/stream.mp3">>),
false = rockbox_ffi_nif:is_url(<<"/music/song.mp3">>).
```
""".
is_url(_S) -> ?NOT_LOADED.
