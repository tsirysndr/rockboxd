/* Stub — shadows firmware/include/core_alloc.h. malloc-backed handle table
 * instead of the firmware buflib (implementation in rbdsp_shim.c). Handles
 * are stable, so core_get_data never moves data — a strict superset of the
 * buflib guarantees the DSP code relies on. */
#ifndef RBDSP_CORE_ALLOC_H
#define RBDSP_CORE_ALLOC_H

#include <stddef.h>

int core_alloc(size_t size);
int core_free(int handle);
void *core_get_data(int handle);

#endif
