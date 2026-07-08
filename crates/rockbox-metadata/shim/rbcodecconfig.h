/* rockbox-metadata configuration header — replaces the firmware target
 * config. Modeled on lib/rbcodec/rbcodecconfig-example.h (used by the
 * warble standalone test player). */
#ifndef RBCODECCONFIG_H_INCLUDED
#define RBCODECCONFIG_H_INCLUDED

#define NUM_CORES 1

/* MEMORYSIZE > 8 selects the full-size tag buffers in metadata.h
 * (ID3V2_BUF_SIZE 1800 / ID3V2_MAX_ITEM_SIZE 500). */
#define MEMORYSIZE 64

/* Parse embedded album art positions (ID3 APIC, FLAC PICTURE, MP4 covr). */
#define HAVE_ALBUMART

/* vtx.c (ZX Spectrum Vortex Tracker) uses floating point. */
#define HAVE_FPU

#ifndef __ASSEMBLER__

/* {,u}int{8,16,32,64}_t, intptr_t, uintptr_t */
#include <inttypes.h>

/* bool, true, false */
#include <stdbool.h>

/* NULL, offsetof, size_t */
#include <stddef.h>

/* ssize_t, off_t, read, lseek */
#include <unistd.h>

/* limits */
#include <limits.h>

#define MAX_PATH 260

/* Firmware maps filesystem calls through FS_PREFIX ("sim_" / "app_"
 * wrappers on hosted targets). Standalone we use plain POSIX names. */
#ifndef FS_PREFIX
#define FS_PREFIX(_x_) _x_
#endif

/* firmware/common/filefuncs — implemented via fstat in rbmeta_shim.c */
off_t filesize(int fd);

/* Some parsers (au.c, …) use DEBUGF without including debug.h; on
 * firmware builds the target config.h provides it. logf is NOT defined
 * here — <math.h> declares float logf(float) and a force-included macro
 * would mangle it; the shim logf.h covers the files that use logging. */
#ifndef DEBUGF
#define DEBUGF(...) do { } while (0)
#endif
#ifndef panicf
#define panicf(...) do { } while (0)
#endif

#endif /* __ASSEMBLER__ */

#endif /* RBCODECCONFIG_H_INCLUDED */
