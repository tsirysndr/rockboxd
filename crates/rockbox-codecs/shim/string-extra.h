/* Shadows firmware/include/string-extra.h — BSD string helpers the
 * firmware carries for targets that lack them. Implemented in
 * rbmeta_shim.c (a static-archive definition coexists with the libc one
 * on macOS / glibc ≥ 2.38).
 *
 * macOS _FORTIFY_SOURCE turns strlcpy/strlcat into function-like macros;
 * only declare them when they aren't already macros. */
#ifndef RBMETA_STRING_EXTRA_H
#define RBMETA_STRING_EXTRA_H

#include <string.h>
#include <strings.h>

#ifndef strlcpy
size_t strlcpy(char *dst, const char *src, size_t siz);
#endif
#ifndef strlcat
size_t strlcat(char *dst, const char *src, size_t siz);
#endif
#ifndef strmemccpy
char *strmemccpy(char *dst, const char *src, size_t n);
#endif

#endif
