/***************************************************************************
 *             __________               __   ___.
 *   Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
 *   Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
 *   Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
 *                     \/            \/     \/    \/            \/
 *
 * Output level tap for meters. See pcm_meter.h.
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License
 * as published by the Free Software Foundation; either version 2
 * of the License, or (at your option) any later version.
 *
 * This software is distributed on an "AS IS" basis, WITHOUT WARRANTY OF ANY
 * KIND, either express or implied.
 *
 ****************************************************************************/
#include "config.h"

#include <stddef.h>
#include <stdint.h>

#include "pcm.h"
/* for pcm_get_frequency() — not exported through pcm.h */
#include "pcm-internal.h"
#include "pcm_meter.h"

/** Corner frequency of the bass band, in Hz. */
#define METER_LOW_HZ 200

/**
 * Measure one frame in this many.
 *
 * RMS over a few hundred frames is indistinguishable from RMS over every one
 * at meter resolution, and this runs on the audio path. The low-pass is
 * decimated with it, so its corner is computed against the decimated rate.
 */
#define METER_DECIMATE 2

/* Published levels. Written only by the audio path, read by anyone. */
static volatile uint32_t meter_left;
static volatile uint32_t meter_right;
static volatile uint32_t meter_low_left;
static volatile uint32_t meter_low_right;

/* Low-pass state, in Q16. Private to the audio path. */
static int32_t lp_left;
static int32_t lp_right;

/** Integer square root, so the RMS costs no libm on FPU-less targets. */
static uint32_t meter_isqrt(uint64_t value)
{
    uint64_t root = 0;
    uint64_t bit = 1ULL << 62;

    while (bit > value)
        bit >>= 2;

    while (bit != 0)
    {
        if (value >= root + bit)
        {
            value -= root + bit;
            root = (root >> 1) + bit;
        }
        else
        {
            root >>= 1;
        }

        bit >>= 2;
    }

    return (uint32_t)root;
}

void pcm_meter_feed(const void *addr, size_t size)
{
    const int16_t *samples = addr;
    size_t frames = size / (2 * sizeof (int16_t));

    if (!samples || frames == 0)
        return;

    unsigned long sampr = pcm_get_frequency();
    if (sampr == 0)
        sampr = 44100;

    /* One-pole coefficient in Q16. The exact form is 1 - exp(-2*pi*fc/fs);
     * for fc well below fs the linear term is within a percent of it and
     * needs no expf(). Clamped in case a sink ever runs at a rate low enough
     * for that to stop holding. */
    uint32_t alpha = (uint32_t)((2 * 31416ULL * METER_LOW_HZ * 65536ULL) /
                                (10000ULL * sampr * METER_DECIMATE));
    if (alpha > 65536)
        alpha = 65536;

    uint64_t sum_left = 0, sum_right = 0;
    uint64_t low_sum_left = 0, low_sum_right = 0;
    size_t counted = 0;

    for (size_t i = 0; i < frames; i += METER_DECIMATE)
    {
        int64_t left = samples[2 * i];
        int64_t right = samples[2 * i + 1];

        sum_left += (uint64_t)(left * left);
        sum_right += (uint64_t)(right * right);

        /* Q16 throughout: a 16-bit sample scaled by 65536 is the widest
         * value here and still fits an int32, but the difference against
         * the running state does not, so it is taken in 64 bits. */
        lp_left += (int32_t)(((left * 65536 - lp_left) * alpha) / 65536);
        lp_right += (int32_t)(((right * 65536 - lp_right) * alpha) / 65536);

        int64_t low_left = lp_left / 65536;
        int64_t low_right = lp_right / 65536;
        low_sum_left += (uint64_t)(low_left * low_left);
        low_sum_right += (uint64_t)(low_right * low_right);

        counted++;
    }

    if (counted == 0)
        return;

    meter_left = meter_isqrt(sum_left / counted);
    meter_right = meter_isqrt(sum_right / counted);
    meter_low_left = meter_isqrt(low_sum_left / counted);
    meter_low_right = meter_isqrt(low_sum_right / counted);
}

void pcm_meter_reset(void)
{
    meter_left = meter_right = 0;
    meter_low_left = meter_low_right = 0;
    lp_left = lp_right = 0;
}

void pcm_meter_read(uint32_t *left, uint32_t *right,
                    uint32_t *low_left, uint32_t *low_right)
{
    if (left)
        *left = meter_left;
    if (right)
        *right = meter_right;
    if (low_left)
        *low_left = meter_low_left;
    if (low_right)
        *low_right = meter_low_right;
}
