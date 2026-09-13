/***************************************************************************
 *             __________               __   ___.
 *   Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
 *   Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
 *   Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
 *   Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
 *                     \/            \/     \/    \/            \/
 *
 * Output level tap for meters.
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
#ifndef PCM_METER_H
#define PCM_METER_H

#include <stddef.h>
#include <stdint.h>

/**
 * Measures what leaves the device so a client can draw a meter that follows
 * the music, rather than animating something plausible.
 *
 * Fed from the PCM path just before the buffer reaches the active sink, so it
 * reflects the audio actually played whichever sink is selected (cpal, SDL,
 * AirPlay, FIFO, ...). All values are 0..PCM_METER_SCALE.
 *
 * Deliberately integer-only: firmware/ is shared with targets that have no
 * FPU, and a meter is not worth pulling libm into that build.
 */

/** Full scale for every value reported here — the peak of a 16-bit sample. */
#define PCM_METER_SCALE 32767u

/**
 * Measure one output buffer of interleaved 16-bit stereo frames.
 *
 * Called from the PCM writer thread with `size` in bytes. Cheap enough for the
 * audio path: a couple of multiplies per frame on every second frame.
 */
void pcm_meter_feed(const void *addr, size_t size);

/**
 * Publish silence.
 *
 * Called when playback stops. Holding the last reading instead would leave a
 * stopped meter looking stuck rather than stopped.
 */
void pcm_meter_reset(void);

/**
 * Read the levels last published.
 *
 * `left`/`right` are the RMS over one output buffer. `low_left`/`low_right`
 * are the same signal through a one-pole low-pass at roughly 200 Hz — a meter
 * driven by the low band moves with the bass, which is what reads as following
 * the music, where full-band RMS is dominated by whatever is loudest and tends
 * to sit near the top.
 *
 * Safe to call from any thread: each value is a single word written by the
 * audio path and read here, which must not be made to wait on a lock just to
 * feed a meter. A reader can therefore catch two of the four from either side
 * of one buffer — invisible at meter refresh rates.
 */
void pcm_meter_read(uint32_t *left, uint32_t *right,
                    uint32_t *low_left, uint32_t *low_right);

/** How many spectrum bands pcm_meter_read_bands() fills. */
#define PCM_METER_BANDS 16

/**
 * Read the per-band levels last published: RMS per band, low to high, at the
 * same scale as pcm_meter_read(). The bands come from a ladder of one-pole
 * low-passes at log-spaced cutoffs over the mono mix — the difference of two
 * neighbouring stages is a band-pass — which is what a spectrum visualiser
 * draws. Same lock-free single-word contract as pcm_meter_read().
 */
void pcm_meter_read_bands(uint32_t *bands);

#endif /* PCM_METER_H */
