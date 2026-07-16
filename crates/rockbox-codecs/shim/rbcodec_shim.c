/* Host-side codec_api implementation — modeled on Rockbox's own
 * standalone test player (lib/rbcodec/test/warble.c), minus SDL/DSP:
 * decoded PCM is converted to interleaved stereo s16 and handed to a
 * sink callback provided by the Rust wrapper.
 *
 * File model (same as warble): the codec streams from a live fd.
 *   read_filebuf     read() into the codec's buffer, advance curpos
 *   request_buffer   peek: read() then lseek() back
 *   advance_buffer   lseek() forward, update curpos AND id3->offset
 *   seek_buffer      absolute lseek(), update curpos
 *
 * Codec loading is static: build.rs compiles each codec with its entry
 * symbols renamed (-D__header=__header_<name>, …) and the table below
 * maps AFMT codec types to those headers — the same scheme as
 * firmware/target/hosted/android/cdylib/lc-android.c. */

#include "codecs.h"
#include "metadata.h"
#include "rbcodec.h"

#include <fcntl.h>
#include <stdarg.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* The global codec-side api pointer every codec's CODEC_HEADER captures
 * as &ci (declared extern in codeclib.h; defined by codec_crt0.c only in
 * the dlopen build). */
struct codec_api *ci;

/* ---- static codec table ---------------------------------------------- */

#define RBCODEC_HEADER(name) extern const struct codec_header __header_##name;

#ifdef RBCODEC_HAVE_WAV
RBCODEC_HEADER(wav)
#endif
#ifdef RBCODEC_HAVE_AIFF
RBCODEC_HEADER(aiff)
#endif
#ifdef RBCODEC_HAVE_AU
RBCODEC_HEADER(au)
#endif
#ifdef RBCODEC_HAVE_SMAF
RBCODEC_HEADER(smaf)
#endif
#ifdef RBCODEC_HAVE_VOX
RBCODEC_HEADER(vox)
#endif
#ifdef RBCODEC_HAVE_WAV64
RBCODEC_HEADER(wav64)
#endif
#ifdef RBCODEC_HAVE_FLAC
RBCODEC_HEADER(flac)
#endif
#ifdef RBCODEC_HAVE_SHORTEN
RBCODEC_HEADER(shorten)
#endif
#ifdef RBCODEC_HAVE_WAVPACK
RBCODEC_HEADER(wavpack)
#endif
#ifdef RBCODEC_HAVE_ALAC
RBCODEC_HEADER(alac)
#endif
#ifdef RBCODEC_HAVE_APE
RBCODEC_HEADER(ape)
#endif
#ifdef RBCODEC_HAVE_TTA
RBCODEC_HEADER(tta)
#endif
#ifdef RBCODEC_HAVE_MPA
RBCODEC_HEADER(mpa)
#endif
#ifdef RBCODEC_HAVE_VORBIS
RBCODEC_HEADER(vorbis)
#endif
#ifdef RBCODEC_HAVE_AAC
RBCODEC_HEADER(aac)
#endif
#ifdef RBCODEC_HAVE_AAC_BSF
RBCODEC_HEADER(aac_bsf)
#endif
#ifdef RBCODEC_HAVE_OPUS
RBCODEC_HEADER(opus)
#endif
#ifdef RBCODEC_HAVE_MPC
RBCODEC_HEADER(mpc)
#endif
#ifdef RBCODEC_HAVE_SPEEX
RBCODEC_HEADER(speex)
#endif
#ifdef RBCODEC_HAVE_WMA
RBCODEC_HEADER(wma)
#endif
#ifdef RBCODEC_HAVE_WMAPRO
RBCODEC_HEADER(wmapro)
#endif
#ifdef RBCODEC_HAVE_A52
RBCODEC_HEADER(a52)
#endif
#ifdef RBCODEC_HAVE_COOK
RBCODEC_HEADER(cook)
#endif
#ifdef RBCODEC_HAVE_RAAC
RBCODEC_HEADER(raac)
#endif
#ifdef RBCODEC_HAVE_A52_RM
RBCODEC_HEADER(a52_rm)
#endif
#ifdef RBCODEC_HAVE_ATRAC3_RM
RBCODEC_HEADER(atrac3_rm)
#endif
#ifdef RBCODEC_HAVE_ATRAC3_OMA
RBCODEC_HEADER(atrac3_oma)
#endif
#ifdef RBCODEC_HAVE_ADX
RBCODEC_HEADER(adx)
#endif
#ifdef RBCODEC_HAVE_MOD
RBCODEC_HEADER(mod)
#endif

static const struct codec_header *header_for_codectype(unsigned int afmt)
{
    switch (afmt)
    {
#ifdef RBCODEC_HAVE_WAV
    case AFMT_PCM_WAV:
        return &__header_wav;
#endif
#ifdef RBCODEC_HAVE_AIFF
    case AFMT_AIFF:
        return &__header_aiff;
#endif
#ifdef RBCODEC_HAVE_AU
    case AFMT_AU:
        return &__header_au;
#endif
#ifdef RBCODEC_HAVE_SMAF
    case AFMT_SMAF:
        return &__header_smaf;
#endif
#ifdef RBCODEC_HAVE_VOX
    case AFMT_VOX:
        return &__header_vox;
#endif
#ifdef RBCODEC_HAVE_WAV64
    case AFMT_WAVE64:
        return &__header_wav64;
#endif
#ifdef RBCODEC_HAVE_FLAC
    case AFMT_FLAC:
        return &__header_flac;
#endif
#ifdef RBCODEC_HAVE_SHORTEN
    case AFMT_SHN:
        return &__header_shorten;
#endif
#ifdef RBCODEC_HAVE_WAVPACK
    case AFMT_WAVPACK:
        return &__header_wavpack;
#endif
#ifdef RBCODEC_HAVE_ALAC
    case AFMT_MP4_ALAC:
        return &__header_alac;
#endif
#ifdef RBCODEC_HAVE_APE
    case AFMT_APE:
        return &__header_ape;
#endif
#ifdef RBCODEC_HAVE_TTA
    case AFMT_TTA:
        return &__header_tta;
#endif
#ifdef RBCODEC_HAVE_MPA
    case AFMT_MPA_L1:
    case AFMT_MPA_L2:
    case AFMT_MPA_L3:
        return &__header_mpa;
#endif
#ifdef RBCODEC_HAVE_VORBIS
    case AFMT_OGG_VORBIS:
        return &__header_vorbis;
#endif
#ifdef RBCODEC_HAVE_AAC
    case AFMT_MP4_AAC:
    case AFMT_MP4_AAC_HE:
        return &__header_aac;
#endif
#ifdef RBCODEC_HAVE_AAC_BSF
    case AFMT_AAC_BSF:
        return &__header_aac_bsf;
#endif
#ifdef RBCODEC_HAVE_OPUS
    case AFMT_OPUS:
        return &__header_opus;
#endif
#ifdef RBCODEC_HAVE_MPC
    case AFMT_MPC_SV7:
    case AFMT_MPC_SV8:
        return &__header_mpc;
#endif
#ifdef RBCODEC_HAVE_SPEEX
    case AFMT_SPEEX:
        return &__header_speex;
#endif
#ifdef RBCODEC_HAVE_WMA
    case AFMT_WMA:
        return &__header_wma;
#endif
#ifdef RBCODEC_HAVE_WMAPRO
    case AFMT_WMAPRO:
        return &__header_wmapro;
#endif
#ifdef RBCODEC_HAVE_A52
    case AFMT_A52:
        return &__header_a52;
#endif
#ifdef RBCODEC_HAVE_COOK
    case AFMT_RM_COOK:
        return &__header_cook;
#endif
#ifdef RBCODEC_HAVE_RAAC
    case AFMT_RM_AAC:
        return &__header_raac;
#endif
#ifdef RBCODEC_HAVE_A52_RM
    case AFMT_RM_AC3:
        return &__header_a52_rm;
#endif
#ifdef RBCODEC_HAVE_ATRAC3_RM
    case AFMT_RM_ATRAC3:
        return &__header_atrac3_rm;
#endif
#ifdef RBCODEC_HAVE_ATRAC3_OMA
    case AFMT_OMA_ATRAC3:
        return &__header_atrac3_oma;
#endif
#ifdef RBCODEC_HAVE_ADX
    case AFMT_ADX:
        return &__header_adx;
#endif
#ifdef RBCODEC_HAVE_MOD
    case AFMT_MOD:
        return &__header_mod;
#endif
    default:
        return NULL;
    }
}

/* ---- session state ----------------------------------------------------- */

#define CODEC_ARENA_SIZE (64 * 1024 * 1024)
#define REQUEST_BUFFER_MAX (32 * 1024)

static int input_fd = -1;
static struct mp3entry current_id3;
static const struct codec_header *current_hdr;
static struct codec_api shim_api;

static rbcodec_sink_t sink_cb;
static void *sink_user;

static _Atomic long pending_action = CODEC_ACTION_NULL;
static _Atomic long pending_param;
static _Atomic unsigned long elapsed_ms;

static unsigned char *codec_arena;   /* codec_get_buffer heap */
static unsigned char *peek_buffer;   /* request_buffer scratch */
static size_t peek_capacity;
static int16_t *out_buffer;          /* pcmbuf_insert conversion scratch */
static size_t out_capacity;          /* in frames */

static struct
{
    intptr_t freq;
    intptr_t depth;       /* fraction bits; > 16 means 32-bit samples */
    intptr_t stereo_mode; /* STEREO_INTERLEAVED / _NONINTERLEAVED / _MONO */
    int channels;
} fmt;

/* ---- codec_api callbacks ---------------------------------------------- */

static void *ci_codec_get_buffer(size_t *size)
{
    if (!codec_arena)
        codec_arena = malloc(CODEC_ARENA_SIZE);
    if (!codec_arena)
    {
        *size = 0;
        return NULL;
    }
    *size = CODEC_ARENA_SIZE;
    return codec_arena;
}

static inline int16_t clip16(int32_t v)
{
    if (v > INT16_MAX)
        return INT16_MAX;
    if (v < INT16_MIN)
        return INT16_MIN;
    return (int16_t)v;
}

/* Convert whatever the codec produced (16/32-bit, interleaved or not,
 * mono or stereo) into interleaved stereo s16 and hand it to the sink. */
static void ci_pcmbuf_insert(const void *ch1, const void *ch2, int count)
{
    if (count <= 0 || !sink_cb)
        return;

    if ((size_t)count > out_capacity)
    {
        out_capacity = (size_t)count;
        out_buffer = realloc(out_buffer, out_capacity * 2 * sizeof(int16_t));
        if (!out_buffer)
        {
            out_capacity = 0;
            return;
        }
    }

    int16_t *out = out_buffer;
    bool mono = (fmt.stereo_mode == STEREO_MONO) || fmt.channels == 1;

    if (fmt.depth > 16)
    {
        /* 32-bit samples with fmt.depth fraction bits → s15 */
        const int shift = (int)fmt.depth - 15;
        const int32_t *s1 = ch1;
        const int32_t *s2 = ch2;
        for (int i = 0; i < count; i++)
        {
            int16_t l, r;
            if (mono)
                l = r = clip16(s1[i] >> shift);
            else if (fmt.stereo_mode == STEREO_NONINTERLEAVED)
            {
                l = clip16(s1[i] >> shift);
                r = clip16(s2[i] >> shift);
            }
            else
            {
                l = clip16(s1[2 * i] >> shift);
                r = clip16(s1[2 * i + 1] >> shift);
            }
            out[2 * i] = l;
            out[2 * i + 1] = r;
        }
    }
    else
    {
        const int16_t *s1 = ch1;
        const int16_t *s2 = ch2;
        for (int i = 0; i < count; i++)
        {
            int16_t l, r;
            if (mono)
                l = r = s1[i];
            else if (fmt.stereo_mode == STEREO_NONINTERLEAVED)
            {
                l = s1[i];
                r = s2[i];
            }
            else
            {
                l = s1[2 * i];
                r = s1[2 * i + 1];
            }
            out[2 * i] = l;
            out[2 * i + 1] = r;
        }
    }

    sink_cb(sink_user, out, (size_t)count, (unsigned long)fmt.freq);
}

static void ci_set_elapsed(unsigned long value)
{
    atomic_store(&elapsed_ms, value);
}

/* ---- streaming (unbounded, forward-only) source ------------------------- *
 * For live/remote streams there is no seekable fd: bytes are pulled from a
 * Rust callback (blocking) into a lookahead buffer that supports the codec's
 * peek(request_buffer)/advance pattern and forward-only seeks. `get_metadata`
 * is skipped (it needs random access); self-describing codecs (MP3/Ogg/AAC)
 * derive their format from the bitstream. `rbcodec_stream_read_fn` is
 * declared in rbcodec.h. */

static rbcodec_stream_read_fn stream_read_cb;
static rbcodec_stream_seek_fn stream_seek_cb; /* NULL = forward-only stream */
static void *stream_user;
static int stream_mode; /* 1 = callback source (input_fd unused) */

static unsigned char *sbuf; /* lookahead buffer */
static size_t sbuf_cap, sbuf_len, sbuf_pos;
static int stream_eof;

static void stream_reset(void)
{
    stream_read_cb = NULL;
    stream_seek_cb = NULL;
    stream_user = NULL;
    stream_mode = 0;
    sbuf_len = sbuf_pos = 0;
    stream_eof = 0;
}

/* Ensure at least `need` unconsumed bytes are buffered, or EOF is reached. */
static void stream_fill(size_t need)
{
    if (sbuf_pos > 0)
    {
        memmove(sbuf, sbuf + sbuf_pos, sbuf_len - sbuf_pos);
        sbuf_len -= sbuf_pos;
        sbuf_pos = 0;
    }
    while (!stream_eof && sbuf_len < need)
    {
        if (sbuf_len + 65536 > sbuf_cap)
        {
            size_t ncap = sbuf_cap ? sbuf_cap : 262144;
            while (ncap < sbuf_len + 65536)
                ncap *= 2;
            unsigned char *nb = realloc(sbuf, ncap);
            if (!nb)
                break;
            sbuf = nb;
            sbuf_cap = ncap;
        }
        long n = stream_read_cb(stream_user, sbuf + sbuf_len, 65536);
        if (n <= 0)
        {
            stream_eof = 1;
            break;
        }
        sbuf_len += (size_t)n;
    }
}

static size_t ci_read_filebuf(void *ptr, size_t size)
{
    if (stream_mode)
    {
        size_t got = 0;
        while (got < size)
        {
            if (sbuf_pos >= sbuf_len)
            {
                stream_fill(size - got);
                if (sbuf_pos >= sbuf_len)
                    break; /* EOF */
            }
            size_t avail = sbuf_len - sbuf_pos;
            size_t take = (size - got < avail) ? size - got : avail;
            memcpy((unsigned char *)ptr + got, sbuf + sbuf_pos, take);
            sbuf_pos += take;
            got += take;
        }
        shim_api.curpos += got;
        return got;
    }
    ssize_t actual = read(input_fd, ptr, size);
    if (actual < 0)
        actual = 0;
    shim_api.curpos += actual;
    return (size_t)actual;
}

static void *ci_request_buffer(size_t *realsize, size_t reqsize)
{
    if (reqsize > REQUEST_BUFFER_MAX &&
        !rbcodec_format_is_atomic(current_id3.codectype))
        reqsize = REQUEST_BUFFER_MAX;

    if (stream_mode)
    {
        stream_fill(reqsize);
        size_t avail = sbuf_len - sbuf_pos;
        *realsize = (reqsize < avail) ? reqsize : avail;
        return sbuf + sbuf_pos;
    }

    if (reqsize > peek_capacity)
    {
        peek_capacity = reqsize;
        peek_buffer = realloc(peek_buffer, peek_capacity);
        if (!peek_buffer)
        {
            peek_capacity = 0;
            *realsize = 0;
            return NULL;
        }
    }

    ssize_t n = read(input_fd, peek_buffer, reqsize);
    if (n < 0)
        n = 0;
    lseek(input_fd, -n, SEEK_CUR); /* rewind the peek */
    *realsize = (size_t)n;
    return peek_buffer;
}

static void ci_advance_buffer(size_t amount)
{
    if (stream_mode)
    {
        while (amount > 0)
        {
            if (sbuf_pos >= sbuf_len)
            {
                stream_fill(amount);
                if (sbuf_pos >= sbuf_len)
                    break; /* EOF */
            }
            size_t avail = sbuf_len - sbuf_pos;
            size_t take = (amount < avail) ? amount : avail;
            sbuf_pos += take;
            amount -= take;
            shim_api.curpos += take;
        }
        current_id3.offset = shim_api.curpos;
        return;
    }
    lseek(input_fd, (off_t)amount, SEEK_CUR);
    shim_api.curpos += amount;
    current_id3.offset = shim_api.curpos;
}

static bool ci_seek_buffer(size_t newpos)
{
    if (stream_mode)
    {
        if (stream_seek_cb)
        {
            /* Seekable callback (e.g. HTTP range-request cache): jump both
             * directions; the source fetches the target range on demand. */
            if (stream_seek_cb(stream_user, (int64_t)newpos) != 0)
                return false;
            sbuf_len = sbuf_pos = 0; /* discard lookahead from the old position */
            stream_eof = 0;
            shim_api.curpos = (off_t)newpos;
            current_id3.offset = shim_api.curpos;
            return true;
        }
        /* Forward-only live stream: rewind is impossible. */
        if ((off_t)newpos < shim_api.curpos)
            return false;
        ci_advance_buffer((size_t)((off_t)newpos - shim_api.curpos));
        return shim_api.curpos == (off_t)newpos;
    }
    off_t actual = lseek(input_fd, (off_t)newpos, SEEK_SET);
    if (actual >= 0)
        shim_api.curpos = actual;
    return actual != -1;
}

static void ci_seek_complete(void)
{
}

static void ci_set_offset(size_t value)
{
    current_id3.offset = value;
}

static void ci_configure(int setting, intptr_t value)
{
    switch (setting)
    {
    case DSP_SET_FREQUENCY:
        fmt.freq = value;
        break;
    case DSP_SET_SAMPLE_DEPTH:
        fmt.depth = value;
        break;
    case DSP_SET_STEREO_MODE:
        fmt.stereo_mode = value;
        fmt.channels = (value == STEREO_MONO) ? 1 : 2;
        break;
    default:
        break;
    }
}

static long ci_get_command(intptr_t *param)
{
    long action = atomic_exchange(&pending_action, CODEC_ACTION_NULL);
    *param = (intptr_t)atomic_load(&pending_param);
    return action;
}

static bool ci_loop_track(void)
{
    return false;
}

static void ci_strip_filesize(off_t value)
{
    shim_api.filesize -= value;
}

static unsigned ci_sleep(unsigned ticks)
{
    (void)ticks;
    return 0;
}

static void ci_yield(void)
{
}

static void ci_nop_void(void)
{
}

/* Codec-side DEBUGF/LOGF diagnostics, off unless RBCODEC_DEBUG is set in
 * the environment (they only fire in codecs compiled with DEBUG anyway). */
static void ci_debugf(const char *fmt_, ...)
{
    static int enabled = -1;
    if (enabled < 0)
        enabled = getenv("RBCODEC_DEBUG") != NULL;
    if (!enabled)
        return;
    va_list ap;
    va_start(ap, fmt_);
    vfprintf(stderr, fmt_, ap);
    va_end(ap);
    fputc('\n', stderr);
}

/* Fatal — used by tlsf on heap corruption. */
void panicf(const char *fmt_, ...)
{
    va_list ap;
    va_start(ap, fmt_);
    vfprintf(stderr, fmt_, ap);
    va_end(ap);
    fputc('\n', stderr);
    abort();
}

static void ci_panicf(const char *msg, ...)
{
    (void)msg;
}

static const struct codec_api shim_api_template = {
    .codec_get_buffer = ci_codec_get_buffer,
    .pcmbuf_insert = ci_pcmbuf_insert,
    .set_elapsed = ci_set_elapsed,
    .read_filebuf = ci_read_filebuf,
    .request_buffer = ci_request_buffer,
    .advance_buffer = ci_advance_buffer,
    .seek_buffer = ci_seek_buffer,
    .seek_complete = ci_seek_complete,
    .set_offset = ci_set_offset,
    .configure = ci_configure,
    .get_command = ci_get_command,
    .loop_track = ci_loop_track,
    .strip_filesize = ci_strip_filesize,
    .sleep = ci_sleep,
    .yield = ci_yield,
    .commit_dcache = ci_nop_void,
    .commit_discard_dcache = ci_nop_void,
    .commit_discard_idcache = ci_nop_void,
    .strcpy = strcpy,
    .strlen = strlen,
    .strcmp = strcmp,
    .strcat = strcat,
    .memset = memset,
    .memcpy = memcpy,
    .memmove = memmove,
    .memcmp = memcmp,
    .memchr = memchr,
    .debugf = ci_debugf,
    .qsort = qsort,
    .panicf = ci_panicf,
};

/* ---- public driver ------------------------------------------------------ */

void rbcodec_set_sink(rbcodec_sink_t cb, void *user)
{
    sink_cb = cb;
    sink_user = user;
}

int rbcodec_open(const char *path)
{
    stream_mode = 0;
    input_fd = open(path, O_RDONLY);
    if (input_fd < 0)
        return -1;

    if (!get_metadata(&current_id3, input_fd, path))
    {
        close(input_fd);
        input_fd = -1;
        return -2;
    }

    current_hdr = header_for_codectype(current_id3.codectype);
    if (!current_hdr)
    {
        close(input_fd);
        input_fd = -1;
        return -3;
    }

    if (current_hdr->lc_hdr.magic != CODEC_MAGIC ||
        current_hdr->lc_hdr.api_version != CODEC_API_VERSION)
    {
        close(input_fd);
        input_fd = -1;
        return -4;
    }

    shim_api = shim_api_template;
    shim_api.filesize = filesize(input_fd);
    shim_api.curpos = 0;
    shim_api.id3 = &current_id3;
    shim_api.audio_hid = -1;
    shim_api.dsp = NULL;

    fmt.freq = current_id3.frequency ? (intptr_t)current_id3.frequency : 44100;
    fmt.depth = 16;
    fmt.stereo_mode = STEREO_INTERLEAVED;
    fmt.channels = 2;

    atomic_store(&pending_action, CODEC_ACTION_NULL);
    atomic_store(&elapsed_ms, current_id3.elapsed);

    /* get_metadata leaves the fd rewound to 0 (it lseeks back when it
     * doesn't own the fd); codecs start from id3->first_frame_offset or
     * seek on their own. */

    /* Wire the codec's `ci` and load it. */
    *current_hdr->api = &shim_api;
    if (current_hdr->entry_point(CODEC_LOAD) != CODEC_OK)
    {
        close(input_fd);
        input_fd = -1;
        return -5;
    }

    return 0;
}

/* Map a file extension (dot-optional) to an AFMT codec type, for streaming
 * where there is no file to sniff. Only self-describing streamable formats
 * need be listed. */
static unsigned int afmt_for_ext(const char *ext)
{
    if (!ext)
        return AFMT_UNKNOWN;
    while (*ext == '.')
        ext++;
    if (!strcasecmp(ext, "mp3") || !strcasecmp(ext, "mp2") ||
        !strcasecmp(ext, "mpa") || !strcasecmp(ext, "mp1"))
        return AFMT_MPA_L3;
    if (!strcasecmp(ext, "ogg") || !strcasecmp(ext, "oga"))
        return AFMT_OGG_VORBIS;
    if (!strcasecmp(ext, "opus"))
        return AFMT_OPUS;
    if (!strcasecmp(ext, "flac"))
        return AFMT_FLAC;
    /* A raw "aac" stream is an ADTS/ADIF bitstream (content-type audio/aac,
     * audio/aacp, audio/x-aac), NOT an MP4 container — so it must use the
     * bitstream decoder (aac_bsf), the same mapping Rockbox's own
     * get_afmt_from_content_type() and the .aac file extension use. Routing it
     * to AFMT_MP4_AAC runs the libm4a demuxer, which needs moov/mdat atoms and
     * fails outright on a raw ADTS frame stream (HE-AAC radio played silence).
     * The MP4 container path is only reachable via .m4a/.mp4, which stream
     * through the seekable/file route (moov isn't self-describing forward). */
    if (!strcasecmp(ext, "aac"))
        return AFMT_AAC_BSF;
    if (!strcasecmp(ext, "m4a") || !strcasecmp(ext, "mp4"))
        return AFMT_MP4_AAC;
    if (!strcasecmp(ext, "wv"))
        return AFMT_WAVPACK;
    if (!strcasecmp(ext, "mpc"))
        return AFMT_MPC_SV7;
    if (!strcasecmp(ext, "wma"))
        return AFMT_WMA;
    if (!strcasecmp(ext, "wav") || !strcasecmp(ext, "wave"))
        return AFMT_PCM_WAV;
    if (!strcasecmp(ext, "aif") || !strcasecmp(ext, "aiff"))
        return AFMT_AIFF;
    return AFMT_UNKNOWN;
}

int rbcodec_open_stream(rbcodec_stream_read_fn read_cb, void *user,
                        const char *ext)
{
    unsigned int afmt = afmt_for_ext(ext);
    current_hdr = header_for_codectype(afmt);
    if (!current_hdr)
        return -3;
    if (current_hdr->lc_hdr.magic != CODEC_MAGIC ||
        current_hdr->lc_hdr.api_version != CODEC_API_VERSION)
        return -4;

    stream_reset();
    stream_read_cb = read_cb;
    stream_user = user;
    stream_mode = 1;
    input_fd = -1;

    memset(&current_id3, 0, sizeof(current_id3));
    current_id3.codectype = afmt;
    /* Self-describing codecs set the true rate via DSP_SET_FREQUENCY; this is
     * only a sane fallback. */
    current_id3.frequency = 44100;

    shim_api = shim_api_template;
    shim_api.filesize = INT64_MAX; /* unknown / unbounded */
    shim_api.curpos = 0;
    shim_api.id3 = &current_id3;
    shim_api.audio_hid = -1;
    shim_api.dsp = NULL;

    fmt.freq = 44100;
    fmt.depth = 16;
    fmt.stereo_mode = STEREO_INTERLEAVED;
    fmt.channels = 2;

    atomic_store(&pending_action, CODEC_ACTION_NULL);
    atomic_store(&elapsed_ms, 0);

    *current_hdr->api = &shim_api;
    if (current_hdr->entry_point(CODEC_LOAD) != CODEC_OK)
    {
        stream_reset();
        return -5;
    }
    return 0;
}

int rbcodec_open_seekable(rbcodec_stream_read_fn read_cb,
                          rbcodec_stream_seek_fn seek_cb, void *user,
                          int64_t size, unsigned long frequency,
                          const char *ext)
{
    unsigned int afmt = afmt_for_ext(ext);
    current_hdr = header_for_codectype(afmt);
    if (!current_hdr)
        return -3;
    if (current_hdr->lc_hdr.magic != CODEC_MAGIC ||
        current_hdr->lc_hdr.api_version != CODEC_API_VERSION)
        return -4;

    stream_reset();
    stream_read_cb = read_cb;
    stream_seek_cb = seek_cb; /* seekable: HTTP range-request cache */
    stream_user = user;
    stream_mode = 1;
    input_fd = -1;

    memset(&current_id3, 0, sizeof(current_id3));
    current_id3.codectype = afmt;
    current_id3.frequency = frequency ? frequency : 44100;

    shim_api = shim_api_template;
    shim_api.filesize = size > 0 ? size : INT64_MAX;
    shim_api.curpos = 0;
    shim_api.id3 = &current_id3;
    shim_api.audio_hid = -1;
    shim_api.dsp = NULL;

    fmt.freq = current_id3.frequency;
    fmt.depth = 16;
    fmt.stereo_mode = STEREO_INTERLEAVED;
    fmt.channels = 2;

    atomic_store(&pending_action, CODEC_ACTION_NULL);
    atomic_store(&elapsed_ms, 0);

    *current_hdr->api = &shim_api;
    if (current_hdr->entry_point(CODEC_LOAD) != CODEC_OK)
    {
        stream_reset();
        return -5;
    }
    return 0;
}

int rbcodec_run(void)
{
    if ((!stream_mode && input_fd < 0) || !current_hdr)
        return CODEC_ERROR;
    return current_hdr->run_proc();
}

void rbcodec_close(void)
{
    if (current_hdr)
    {
        current_hdr->entry_point(CODEC_UNLOAD);
        current_hdr = NULL;
    }
    if (input_fd >= 0)
    {
        close(input_fd);
        input_fd = -1;
    }
    stream_reset();
}

void rbcodec_request_seek(long time_ms)
{
    atomic_store(&pending_param, time_ms);
    atomic_store(&pending_action, CODEC_ACTION_SEEK_TIME);
}

void rbcodec_request_halt(void)
{
    atomic_store(&pending_param, 0);
    atomic_store(&pending_action, CODEC_ACTION_HALT);
}

unsigned long rbcodec_elapsed_ms(void)
{
    return atomic_load(&elapsed_ms);
}

unsigned long rbcodec_frequency(void)
{
    return (unsigned long)fmt.freq;
}

const char *rbcodec_codec_name(void)
{
    if (input_fd < 0)
        return NULL;
    if (current_id3.codectype >= AFMT_NUM_CODECS)
        return NULL;
    return audio_formats[current_id3.codectype].label;
}
