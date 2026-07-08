/* rockbox-metadata platform header — portable variant of
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
#define htole16(x) swap16(x)
#define htole32(x) swap32(x)
#else
#define ROCKBOX_LITTLE_ENDIAN 1
#define letoh16(x) (x)
#define letoh32(x) (x)
#define betoh16(x) swap16(x)
#define betoh32(x) swap32(x)
#endif

#endif /* RBCODECPLATFORM_H_INCLUDED */
