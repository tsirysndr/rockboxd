/* rbdsp-sys configuration header — replaces the firmware target config.
 * Modeled on lib/rbcodec/rbcodecconfig-example.h (used by the warble
 * standalone test player). */
#ifndef RBCODECCONFIG_H_INCLUDED
#define RBCODECCONFIG_H_INCLUDED

#define HAVE_PITCHCONTROL
#define HAVE_SW_TONE_CONTROLS
#define NUM_CORES 1

#define DSP_OUT_MIN_HZ     8000
#define DSP_OUT_DEFAULT_HZ 44100
#define DSP_OUT_MAX_HZ     96000

#ifndef __ASSEMBLER__

/* {,u}int{8,16,32,64}_t, intptr_t, uintptr_t */
#include <inttypes.h>

/* bool, true, false */
#include <stdbool.h>

/* NULL, offsetof, size_t */
#include <stddef.h>

/* ssize_t, off_t */
#include <unistd.h>

/* limits */
#include <limits.h>

#define MAX_PATH 260

#endif /* __ASSEMBLER__ */

#endif /* RBCODECCONFIG_H_INCLUDED */
