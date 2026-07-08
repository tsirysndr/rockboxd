/* rockbox-codecs configuration header — replaces the firmware target
 * config.
 *
 * INVARIANT: the mp3entry-affecting defines (MEMORYSIZE, HAVE_ALBUMART,
 * HAVE_TAGCACHE absence, MAX_PATH) MUST match
 * crates/rockbox-metadata/shim/rbcodecconfig.h exactly — both crates'
 * archives are linked into the same binary and share `struct mp3entry`. */
#ifndef RBCODECCONFIG_H_INCLUDED
#define RBCODECCONFIG_H_INCLUDED

#define NUM_CORES 1
#define MEMORYSIZE 64
#define HAVE_ALBUMART
#define HAVE_FPU

/* Identifies the "target" in each codec's __header; the shim only checks
 * magic + api_version, but keep it consistent between shim and codecs
 * (both compiled with this header, so it always is). */
#define TARGET_ID 100

/* Nominal codec binary size — libm4a uses it to bound optional seek
 * tables. Same value as the hosted targets (sdlapp/androidcdylib). */
#define CODEC_SIZE 0x100000

/* dsp_core.h / dsp_misc.h are included by codecs.h for types and enum
 * values (the codecs only reach the DSP through ci->configure). */
#define DSP_OUT_MIN_HZ     8000
#define DSP_OUT_DEFAULT_HZ 44100
#define DSP_OUT_MAX_HZ     96000
#define HAVE_PITCHCONTROL
#define HAVE_SW_TONE_CONTROLS

#ifndef __ASSEMBLER__

#include <inttypes.h>
#include <stdbool.h>
#include <stddef.h>
#include <unistd.h>
#include <limits.h>

#define MAX_PATH 260

#ifndef FS_PREFIX
#define FS_PREFIX(_x_) _x_
#endif

/* implemented in rockbox-metadata's rbmeta_shim.c (linked alongside) */
off_t filesize(int fd);

#ifndef DEBUGF
#define DEBUGF(...) do { } while (0)
#endif
/* panicf is a REAL function (rbcodec_shim.c) — tlsf.c declares it via
 * panic.h, and a function-like macro would mangle that declaration. */
void panicf(const char *fmt, ...);

/* Alignment helpers from firmware/export/system.h (codeclib.c uses
 * MEM_ALIGN_UP for its malloc arena). */
#ifndef P2_M1
#define P2_M1(p2)  ((1 << (p2))-1)
#define ALIGN_DOWN_P2(n, p2) ((n) & ~P2_M1(p2))
#define ALIGN_UP_P2(n, p2)   ALIGN_DOWN_P2((n) + P2_M1(p2),p2)
#define ALIGN_DOWN(n, a)     ((typeof(n))((uintptr_t)(n)/(a)*(a)))
#define ALIGN_UP(n, a)       ALIGN_DOWN((n)+((a)-1),a)
#define MEM_ALIGN_ATTR __attribute__((aligned(sizeof(intptr_t))))
#define MEM_ALIGN_SIZE sizeof(intptr_t)
#define MEM_ALIGN_UP(x) \
    ((typeof (x))ALIGN_UP((uintptr_t)(x), MEM_ALIGN_SIZE))
#define MEM_ALIGN_DOWN(x) \
    ((typeof (x))ALIGN_DOWN((uintptr_t)(x), MEM_ALIGN_SIZE))
#endif

#endif /* __ASSEMBLER__ */

#endif /* RBCODECCONFIG_H_INCLUDED */
