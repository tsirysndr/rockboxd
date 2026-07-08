/* Shadows firmware/include/rbunicode.h.
 *
 * The firmware implementation (firmware/common/unicode.c) loads codepage
 * tables from disk through buflib; standalone we implement the handful of
 * functions the metadata parsers use in rbmeta_shim.c. Non-Latin legacy
 * codepages (SJIS, GB2312, …) fall back to ISO-8859-1 — the same behavior
 * the firmware has before a codepage table is loaded. */
#ifndef RBMETA_RBUNICODE_H
#define RBMETA_RBUNICODE_H

#include "config.h"
#include <stdbool.h>

#define MASK   0xC0 /* 11000000 */
#define COMP   0x80 /* 10x      */

enum codepages {
    ISO_8859_1 = 0, /* Latin1 */
    ISO_8859_7,     /* Greek */
    ISO_8859_8,     /* Hebrew */
    WIN_1251,       /* Cyrillic */
    ISO_8859_11,    /* Thai */
    WIN_1256,       /* Arabic */
    ISO_8859_9,     /* Turkish */
    ISO_8859_2,     /* Latin Extended */
    WIN_1250,       /* Central European */
    WIN_1252,       /* Western European */
    SJIS,           /* Japanese */
    GB_2312,        /* Simp. Chinese */
    KSX_1001,       /* Korean */
    BIG_5,          /* Trad. Chinese */
    UTF_8,          /* Unicode */
    NUM_CODEPAGES,
    INIT_CODEPAGE = ISO_8859_1,
};

/* Encode a UCS value as UTF-8 and return a pointer after this UTF-8 char. */
unsigned char* utf8encode(unsigned long ucs, unsigned char *utf8);
unsigned char* iso_decode(const unsigned char *iso, unsigned char *utf8,
                          int cp, int count);
unsigned char* iso_decode_ex(const unsigned char *iso, unsigned char *utf8,
                             int cp, int count, int utf8_size);
/* True if utf16 starts with a BOM; *le is set to the detected (or, without
 * a BOM, guessed) endianness. */
bool utf16_has_bom(const unsigned char *utf16, bool *le);
/* Current default codepage — always ISO_8859_1 standalone. */
int get_codepage(void);
unsigned char* utf16LEdecode(const unsigned char *utf16, unsigned char *utf8,
                             int count);
unsigned char* utf16BEdecode(const unsigned char *utf16, unsigned char *utf8,
                             int count);
/* Bounded variant: decodes at most `count` UTF-16 code units without
 * writing more than `utf8_size` bytes of UTF-8 output. */
unsigned char* utf16decode(const unsigned char *utf16, unsigned char *utf8,
                           int count, int utf8_size, bool le);
unsigned long utf8length(const unsigned char *utf8);

#endif /* RBMETA_RBUNICODE_H */
