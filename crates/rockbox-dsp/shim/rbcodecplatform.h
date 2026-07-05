/* rbdsp-sys platform header — portable variant of
 * lib/rbcodec/rbcodecplatform-unix.h that also builds on macOS
 * (no <byteswap.h> / <endian.h>). */
#ifndef RBCODECPLATFORM_H_INCLUDED
#define RBCODECPLATFORM_H_INCLUDED

#include <assert.h>
#include <ctype.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>

#ifndef swap16
#define swap16(x) __builtin_bswap16(x)
#endif
#ifndef swap32
#define swap32(x) __builtin_bswap32(x)
#endif

#if defined(__BYTE_ORDER__) && __BYTE_ORDER__ == __ORDER_BIG_ENDIAN__
#define ROCKBOX_BIG_ENDIAN 1
#define letoh16(x) swap16(x)
#define letoh32(x) swap32(x)
#define betoh16(x) (x)
#define betoh32(x) (x)
#else
#define ROCKBOX_LITTLE_ENDIAN 1
#define letoh16(x) (x)
#define letoh32(x) (x)
#define betoh16(x) swap16(x)
#define betoh32(x) swap32(x)
#endif

/* tdspeed.c allocates its work buffers through these */
static inline bool tdspeed_alloc_buffers(int32_t **buffers,
                                         const int *buf_s, int nbuf)
{
    int i;
    for (i = 0; i < nbuf; i++)
    {
        buffers[i] = malloc(buf_s[i]);
        if (!buffers[i])
            return false;
    }
    return true;
}

static inline void tdspeed_free_buffers(int32_t **buffers, int nbuf)
{
    int i;
    for (i = 0; i < nbuf; i++)
    {
        free(buffers[i]);
    }
}

#endif /* RBCODECPLATFORM_H_INCLUDED */
