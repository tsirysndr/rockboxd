/* Host-side support code for the extracted Rockbox DSP:
 *   - malloc-backed core_alloc (replaces the firmware buflib)
 *   - find_first_set_bit (declared in lib/rbcodec/platform.h)
 *   - get_replaygain_int (extracted from lib/rbcodec/metadata/replaygain.c
 *     to avoid pulling in the whole metadata layer)
 */
#include "platform.h"
#include "core_alloc.h"
#include "fixedpoint.h"
#include <stdlib.h>
#include <string.h>

/** core_alloc **/

#define RBDSP_MAX_HANDLES 64

static void *rbdsp_handles[RBDSP_MAX_HANDLES];

int core_alloc(size_t size)
{
    for (int i = 0; i < RBDSP_MAX_HANDLES; i++)
    {
        if (!rbdsp_handles[i])
        {
            rbdsp_handles[i] = calloc(1, size);
            return rbdsp_handles[i] ? i + 1 : -1;
        }
    }
    return -1;
}

int core_free(int handle)
{
    if (handle > 0 && handle <= RBDSP_MAX_HANDLES)
    {
        free(rbdsp_handles[handle - 1]);
        rbdsp_handles[handle - 1] = NULL;
    }
    return 0;
}

void *core_get_data(int handle)
{
    if (handle > 0 && handle <= RBDSP_MAX_HANDLES)
        return rbdsp_handles[handle - 1];
    return NULL;
}

/** platform.h **/

int find_first_set_bit(uint32_t value)
{
    return value ? __builtin_ctz(value) : 32;
}

/** replaygain **/

/* Constants match lib/rbcodec/metadata/replaygain.c */
#define FP_BITS (12)
#define FP_ONE  (1 << FP_BITS)
#define FP_MIN  (-48 * FP_ONE)
#define FP_MAX  ( 17 * FP_ONE)

static long convert_gain(long gain)
{
    /* Don't allow unreasonably low or high gain changes. */
    gain = MAX(gain, FP_MIN);
    gain = MIN(gain, FP_MAX);

    return fp_factor(gain, FP_BITS) << (24 - FP_BITS);
}

long get_replaygain_int(long int_gain)
{
    return convert_gain(int_gain * FP_ONE / 100);
}
