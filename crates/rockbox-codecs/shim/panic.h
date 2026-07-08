/* Stub — shadows firmware/export/panic.h (tlsf.c includes it under
 * ROCKBOX). Implemented in rbcodec_shim.c as vfprintf + abort. */
#ifndef RBCODEC_PANIC_H
#define RBCODEC_PANIC_H

void panicf(const char *fmt, ...);

#endif
