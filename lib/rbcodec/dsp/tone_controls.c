/***************************************************************************
 *             __________               __   ___.
 *   Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
 *   Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
 *   Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
 *   Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
 *                     \/            \/     \/    \/            \/
 *
 * Copyright (C) 2007 Thom Johansen
 * Copyright (C) 2012 Michael Sevakis
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
#include "rbcodecconfig.h"
#include "platform.h"
#include "fixedpoint.h"
#include "dsp_proc_entry.h"
#include "dsp_filter.h"
#include "tone_controls.h"
#include "dsp_misc.h"

/* These apply to all DSP streams to remain as consistant as possible with
 * the behavior of hardware tone controls */

/* Cutoffs in HZ - defaults match hardware tone controls */
#define TONE_BASS_CUTOFF_DEFAULT   200
#define TONE_TREBLE_CUTOFF_DEFAULT 3500
static unsigned int tone_bass_cutoff = TONE_BASS_CUTOFF_DEFAULT;
static unsigned int tone_treble_cutoff = TONE_TREBLE_CUTOFF_DEFAULT;

/* Current bass and treble gain values */
static int tone_bass = 0;
static int tone_treble = 0;

/* Current prescaler setting */
static int tone_prescale = 0;

/* Data for each DSP */
static struct dsp_filter tone_filters[DSP_COUNT] IBSS_ATTR;

static void update_filter(int id, unsigned int fout)
{
    filter_bishelf_coefs(fp_div(tone_bass_cutoff, fout, 32),
                         fp_div(tone_treble_cutoff, fout, 32),
                         tone_bass, tone_treble, -tone_prescale,
                         &tone_filters[id]);
}

/* Update the filters' coefficients based upon the bass/treble settings */
void tone_set_prescale(int prescale)
{
    int bass = tone_bass;
    int treble = tone_treble;

    tone_prescale = prescale;

    struct dsp_config *dsp;
    for (int i = 0; (dsp = dsp_get_config(i)); i++)
    {
        update_filter(i, dsp_get_output_frequency(dsp));
    
        bool enable = bass != 0 || treble != 0;
        dsp_proc_enable(dsp, DSP_PROC_TONE_CONTROLS, enable);

        if (enable && !dsp_proc_active(dsp, DSP_PROC_TONE_CONTROLS))
        {
            filter_flush(&tone_filters[i]); /* Going online */
            dsp_proc_activate(dsp, DSP_PROC_TONE_CONTROLS, true);
        }
    }
}

/* Prescaler is always set after setting bass/treble, so we wait with
 * calculating coefs until such time. */

/* Change the bass/treble cutoff frequencies in Hz (0 = default).
 * Takes effect on the next tone_set_prescale() call. */
void tone_set_bass_cutoff(int hz)
{
    tone_bass_cutoff = hz > 0 ? (unsigned int)hz : TONE_BASS_CUTOFF_DEFAULT;
}

void tone_set_treble_cutoff(int hz)
{
    tone_treble_cutoff = hz > 0 ? (unsigned int)hz : TONE_TREBLE_CUTOFF_DEFAULT;
}

/* Change the bass setting */
void tone_set_bass(int bass)
{
    tone_bass = bass;
}

/* Change the treble setting */
void tone_set_treble(int treble)
{
    tone_treble = treble;
}

/* Apply the tone control filter in-place */
static void tone_process(struct dsp_proc_entry *this,
                         struct dsp_buffer **buf_p)
{
    struct dsp_buffer *buf = *buf_p;
    filter_process((struct dsp_filter *)this->data, buf->p32, buf->remcount,
                   buf->format.num_channels);
}

/* DSP message hook */
static intptr_t tone_configure(struct dsp_proc_entry *this,
                               struct dsp_config *dsp,
                               unsigned int setting,
                               intptr_t value)
{
    switch (setting)
    {
    case DSP_PROC_INIT:
        if (value != 0)
            break; /* Already enabled */

        this->data = (intptr_t)&tone_filters[dsp_get_id(dsp)];
        this->process = tone_process;
        /* Fall-through */
    case DSP_FLUSH:
        filter_flush((struct dsp_filter *)this->data);
        break;

    case DSP_SET_OUT_FREQUENCY:
        update_filter(dsp_get_id(dsp), value);
        break;
    }

    return 0;
}

/* Database entry */
DSP_PROC_DB_ENTRY(TONE_CONTROLS,
                  tone_configure);
