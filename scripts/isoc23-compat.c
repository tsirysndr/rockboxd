/* isoc23-compat.c
 *
 * glibc 2.38 introduced __isoc23_strtol / __isoc23_fscanf / ... and its
 * headers redirect strtol/strtoul/strtoll/fscanf/sscanf/scanf to those
 * variants whenever _DEFAULT_SOURCE/_GNU_SOURCE is active (which it is for
 * the ARMHFHOST firmware build: tools/configure passes -D_GNU_SOURCE=1).
 *
 * The firmware is compiled inside the cross-rs Docker image (glibc >= 2.38),
 * so its objects reference __isoc23_* symbols. But the binary must run on real
 * ARMv6 devices (Raspberry Pi 1/Zero, Raspberry Pi OS bullseye 2.31 / bookworm
 * 2.36) whose glibc predates 2.38 and has no such symbols.
 *
 * This shim defines the __isoc23_* symbols as thin forwards to the classic
 * parsing functions. For Rockbox's usage (track numbers, tags, cpu frequency)
 * the C23 base-parsing changes are irrelevant, so forwarding is safe. The
 * final binary then only depends on the classic symbols from the runtime
 * glibc, keeping it portable to glibc >= 2.31.
 *
 * Archived into libfirmware.a by scripts/build-armhf.sh (ARM cross build only).
 */
#include <stdarg.h>
#include <stdio.h>

/* Reference the classic symbols directly via asm labels, bypassing the
 * glibc >= 2.38 header macros that would otherwise redirect these calls to
 * __isoc23_* as well (which would defeat the purpose). */
extern long               __cl_strtol   (const char *, char **, int)         __asm__("strtol");
extern unsigned long      __cl_strtoul  (const char *, char **, int)         __asm__("strtoul");
extern long long          __cl_strtoll  (const char *, char **, int)         __asm__("strtoll");
extern unsigned long long __cl_strtoull (const char *, char **, int)         __asm__("strtoull");
extern int                __cl_vfscanf  (FILE *, const char *, va_list)       __asm__("vfscanf");
extern int                __cl_vsscanf  (const char *, const char *, va_list) __asm__("vsscanf");
extern int                __cl_vscanf   (const char *, va_list)               __asm__("vscanf");

long __isoc23_strtol(const char *nptr, char **endptr, int base)
{ return __cl_strtol(nptr, endptr, base); }

unsigned long __isoc23_strtoul(const char *nptr, char **endptr, int base)
{ return __cl_strtoul(nptr, endptr, base); }

long long __isoc23_strtoll(const char *nptr, char **endptr, int base)
{ return __cl_strtoll(nptr, endptr, base); }

unsigned long long __isoc23_strtoull(const char *nptr, char **endptr, int base)
{ return __cl_strtoull(nptr, endptr, base); }

int __isoc23_fscanf(FILE *stream, const char *fmt, ...)
{ va_list ap; va_start(ap, fmt); int r = __cl_vfscanf(stream, fmt, ap); va_end(ap); return r; }

int __isoc23_sscanf(const char *str, const char *fmt, ...)
{ va_list ap; va_start(ap, fmt); int r = __cl_vsscanf(str, fmt, ap); va_end(ap); return r; }

int __isoc23_scanf(const char *fmt, ...)
{ va_list ap; va_start(ap, fmt); int r = __cl_vscanf(fmt, ap); va_end(ap); return r; }

int __isoc23_vfscanf(FILE *stream, const char *fmt, va_list ap)
{ return __cl_vfscanf(stream, fmt, ap); }

int __isoc23_vsscanf(const char *str, const char *fmt, va_list ap)
{ return __cl_vsscanf(str, fmt, ap); }
