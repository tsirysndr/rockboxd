/* Host-side support code for the standalone rockbox-metadata build.
 *
 * Provides the firmware symbols the metadata parsers rely on (filesize,
 * BSD string helpers, the rbunicode conversion functions) plus the flat
 * rbmeta_read() bridge declared in rbmeta.h.
 *
 * The unicode routines are copied from firmware/common/unicode.c (GPL-2,
 * same project) minus the on-disk codepage table machinery: legacy
 * multi-byte codepages fall back to ISO-8859-1, which matches firmware
 * behavior before a codepage table has been loaded. */

#include "platform.h"
#include "rbunicode.h"
#include "metadata.h"
#include "rbmeta.h"

#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/* ---- firmware/common/filefuncs ------------------------------------- */

off_t filesize(int fd)
{
    struct stat st;
    if (fstat(fd, &st) < 0)
        return -1;
    return st.st_size;
}

/* ---- BSD string helpers (firmware/common/strlcpy.c & friends) ------ */

/* macOS _FORTIFY_SOURCE defines these as function-like macros; drop the
 * macros so the definitions below parse. */
#undef strlcpy
#undef strlcat

size_t strlcpy(char *dst, const char *src, size_t siz)
{
    const char *s = src;
    size_t n = siz;

    if (n != 0)
    {
        while (--n != 0)
        {
            if ((*dst++ = *s++) == '\0')
                break;
        }
    }

    if (n == 0)
    {
        if (siz != 0)
            *dst = '\0';
        while (*s++)
            ;
    }

    return s - src - 1;
}

size_t strlcat(char *dst, const char *src, size_t siz)
{
    char *d = dst;
    const char *s = src;
    size_t n = siz;
    size_t dlen;

    while (n-- != 0 && *d != '\0')
        d++;
    dlen = d - dst;
    n = siz - dlen;

    if (n == 0)
        return dlen + strlen(s);
    while (*s != '\0')
    {
        if (n != 1)
        {
            *d++ = *s;
            n--;
        }
        s++;
    }
    *d = '\0';

    return dlen + (s - src);
}

char *strmemccpy(char *dst, const char *src, size_t n)
{
    return strlcpy(dst, src, n) < n ? NULL : dst + n;
}

/* ---- rbunicode (from firmware/common/unicode.c) --------------------- */

static const unsigned char utf8comp[6] =
{
    0x00, 0xC0, 0xE0, 0xF0, 0xF8, 0xFC
};

static int utf8_ucs_get_extra_bytes_count(unsigned long ucs)
{
    int tail = 0;

    if (ucs > 0x7F)
        while (ucs >> (5*tail + 6))
            tail++;

    return tail;
}

static unsigned char *utf8encode_internal(unsigned long ucs,
                                          unsigned char *utf8, int tail)
{
    *utf8++ = (ucs >> (6*tail)) | utf8comp[tail];
    while (tail--)
        *utf8++ = ((ucs >> (6*tail)) & (MASK ^ 0xFF)) | COMP;
    return utf8;
}

unsigned char* utf8encode(unsigned long ucs, unsigned char *utf8)
{
    return utf8encode_internal(ucs, utf8, utf8_ucs_get_extra_bytes_count(ucs));
}

unsigned long utf8length(const unsigned char *utf8)
{
    unsigned long l = 0;

    while (*utf8 != 0)
        if ((*utf8++ & MASK) != COMP)
            l++;

    return l;
}

unsigned char* iso_decode_ex(const unsigned char *iso, unsigned char *utf8,
                             int cp, int count, int utf8_size)
{
    /* No codepage tables standalone: everything except explicit UTF-8
     * passthrough decodes as ISO-8859-1 (each byte is the UCS value) —
     * the same behavior the firmware has before a table is loaded. */
    while (count-- > 0 && utf8_size > 0)
    {
        if (*iso < 128 || cp == UTF_8)
        {
            *utf8++ = *iso++;
            --utf8_size;
        }
        else
        {
            unsigned long ucs = *iso++;
            int tail = utf8_ucs_get_extra_bytes_count(ucs);
            utf8_size -= tail + 1;
            if (utf8_size < 0)
                break;
            utf8 = utf8encode_internal(ucs, utf8, tail);
        }
    }
    return utf8;
}

unsigned char* iso_decode(const unsigned char *iso, unsigned char *utf8,
                          int cp, int count)
{
    return iso_decode_ex(iso, utf8, cp, count, INT_MAX);
}

bool utf16_has_bom(const unsigned char *utf16, bool *le)
{
    unsigned long ucs = utf16[0] << 8 | utf16[1];

    if (ucs == 0xFEFF) /* Check for BOM */
    {
        *le = false;
        return true;
    }

    if (ucs == 0xFFFE)
    {
        *le = true;
        return true;
    }

    /* If there is no BOM let's try to guess it. If one of the bytes is
       0x00, it is probably the most significant one. */
    *le = utf16[1] == 0;
    return false;
}

int get_codepage(void)
{
    return ISO_8859_1;
}

static unsigned char *utf8encode_ex(unsigned long ucs, unsigned char *utf8,
                                    int *utf8_size)
{
    const int tail = utf8_ucs_get_extra_bytes_count(ucs);
    *utf8_size -= tail + 1;
    return *utf8_size < 0 ? utf8 : utf8encode_internal(ucs, utf8, tail);
}

unsigned char* utf16decode(const unsigned char *utf16, unsigned char *utf8,
                           int count, int utf8_size, bool le)
{
    /* little-endian flag is used as significant byte index */
    int lei = le ? 1 : 0;

    unsigned long ucs;

    while (count > 0 && utf8_size > 0)
    {
        /* Check for a surrogate pair */
        if (*(utf16 + lei) >= 0xD8 && *(utf16 + lei) < 0xE0)
        {
            ucs = 0x10000 + ((utf16[1 - lei] << 10)
                  | ((utf16[lei] - 0xD8) << 18)
                  | utf16[2 + (1 - lei)] | ((utf16[2 + lei] - 0xDC) << 8));
            utf16 += 4;
            count -= 2;
        }
        else
        {
            ucs = utf16[lei] << 8 | utf16[1 - lei];
            utf16 += 2;
            count -= 1;
        }
        utf8 = utf8encode_ex(ucs, utf8, &utf8_size);
    }
    return utf8;
}

unsigned char* utf16LEdecode(const unsigned char *utf16, unsigned char *utf8,
                             int count)
{
    return utf16decode(utf16, utf8, count, INT_MAX, true);
}

unsigned char* utf16BEdecode(const unsigned char *utf16, unsigned char *utf8,
                             int count)
{
    return utf16decode(utf16, utf8, count, INT_MAX, false);
}

/* ---- flat bridge for the Rust wrapper -------------------------------- */

const char *rbmeta_codec_label(unsigned int codectype)
{
    if (codectype >= AFMT_NUM_CODECS)
        return NULL;
    return audio_formats[codectype].label;
}

static void copy_str(char *dst, size_t n, const char *src)
{
    if (src)
        strlcpy(dst, src, n);
    else
        dst[0] = '\0';
}

int rbmeta_read(const char *path, struct rbmeta_tags *out)
{
    memset(out, 0, sizeof(*out));

    int fd = open(path, O_RDONLY);
    if (fd < 0)
        return -1;

    /* mp3entry is ~3 KB (inline tag buffers) — keep it off the stack. */
    struct mp3entry *id3 = malloc(sizeof(*id3));
    if (!id3)
    {
        close(fd);
        return -3;
    }

    bool ok = get_metadata(id3, fd, path);
    close(fd);

    if (!ok)
    {
        free(id3);
        return -2;
    }

    copy_str(out->codec, sizeof(out->codec), rbmeta_codec_label(id3->codectype));
    copy_str(out->title, sizeof(out->title), id3->title);
    copy_str(out->artist, sizeof(out->artist), id3->artist);
    copy_str(out->album, sizeof(out->album), id3->album);
    copy_str(out->albumartist, sizeof(out->albumartist), id3->albumartist);
    copy_str(out->composer, sizeof(out->composer), id3->composer);
    copy_str(out->grouping, sizeof(out->grouping), id3->grouping);
    copy_str(out->comment, sizeof(out->comment), id3->comment);
    copy_str(out->genre, sizeof(out->genre), id3->genre_string);
    copy_str(out->year_string, sizeof(out->year_string), id3->year_string);
    copy_str(out->track_string, sizeof(out->track_string), id3->track_string);
    copy_str(out->disc_string, sizeof(out->disc_string), id3->disc_string);
    copy_str(out->mb_track_id, sizeof(out->mb_track_id), id3->mb_track_id);

    out->codectype = (int32_t)id3->codectype;
    out->tracknum = id3->tracknum;
    out->discnum = id3->discnum;
    out->year = id3->year;
    out->layer = id3->layer;
    out->id3version = id3->id3version;
    out->vbr = id3->vbr ? 1 : 0;
    out->bitrate = (int32_t)id3->bitrate;
    out->frequency = (uint32_t)id3->frequency;

    out->filesize = id3->filesize;
    out->length_ms = id3->length;
    out->samples = id3->samples;
    out->frame_count = id3->frame_count;
    out->first_frame_offset = id3->first_frame_offset;

    out->track_level = id3->track_level;
    out->album_level = id3->album_level;
    out->track_gain = id3->track_gain;
    out->album_gain = id3->album_gain;
    out->track_peak = id3->track_peak;
    out->album_peak = id3->album_peak;

    out->has_albumart = id3->has_embedded_albumart ? 1 : 0;
    if (id3->has_embedded_albumart)
    {
        out->albumart_type = (int32_t)id3->albumart.type;
        out->albumart_pos = id3->albumart.pos;
        out->albumart_size = id3->albumart.size;
    }

    out->has_cuesheet = id3->has_embedded_cuesheet ? 1 : 0;
    if (id3->has_embedded_cuesheet)
    {
        out->cuesheet_pos = id3->embedded_cuesheet.pos;
        out->cuesheet_size = id3->embedded_cuesheet.size;
        out->cuesheet_encoding = (int32_t)id3->embedded_cuesheet.encoding;
    }

    free(id3);
    return 0;
}
