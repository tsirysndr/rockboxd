/* SPDX-License-Identifier: GPL-2.0-or-later
 *
 * No-op audio hardware driver for the headless host build, modelled on
 * firmware/drivers/audio/sdl.c.  Volume is handled by the OS audio mixer
 * via cpal; the firmware does not drive a codec chip directly on this target.
 *
 * Each function is compiled only when the corresponding AUDIOHW_HAVE_* cap
 * is defined, so the set built is always consistent with the config and lld
 * never sees a duplicate symbol.
 */

#include "config.h"

/* audiohw_set_volume is always required (called from sound.c:set_prescaled_volume).
 * Only the cpal sink exposes an OS-mixer volume hook (pcm_cpal_set_volume), so
 * the firmware level tracks Rockbox's volume slider there. The direct ALSA sink
 * (ARMHFHOST + FreeBSD/NetBSD) and the sndio sink (OpenBSD) have no per-sink
 * volume hook; on those targets Rockbox's software volume (DSP) already scales
 * the PCM before it reaches the sink, so this is a no-op.
 * Values are in tenth-decibel units (0 = 0 dB, -740 = -74 dB, INT_MIN = mute). */
#if !defined(ARMHFHOST) && !defined(__FreeBSD__) && \
    !defined(__NetBSD__) && !defined(__OpenBSD__)
#define HEADLESS_CPAL_VOLUME 1
extern void pcm_cpal_set_volume(int vol_l, int vol_r);
#endif

#if defined(AUDIOHW_HAVE_MONO_VOLUME)
void audiohw_set_volume(int volume)
{
#ifdef HEADLESS_CPAL_VOLUME
    pcm_cpal_set_volume(volume, volume);
#else
    (void)volume;
#endif
}
#else
void audiohw_set_volume(int vol_l, int vol_r)
{
#ifdef HEADLESS_CPAL_VOLUME
    pcm_cpal_set_volume(vol_l, vol_r);
#else
    (void)vol_l; (void)vol_r;
#endif
}
#endif

#if defined(AUDIOHW_HAVE_PRESCALER)
void audiohw_set_prescaler(int value)         { (void)value; }
#endif
#if defined(AUDIOHW_HAVE_BALANCE)
void audiohw_set_balance(int value)           { (void)value; }
#endif
/* bass/treble: skip when HAVE_SW_TONE_CONTROLS — audiohw-swcodec.c owns them */
#ifndef HAVE_SW_TONE_CONTROLS
#if defined(AUDIOHW_HAVE_BASS)
void audiohw_set_bass(int value)              { (void)value; }
#endif
#if defined(AUDIOHW_HAVE_TREBLE)
void audiohw_set_treble(int value)            { (void)value; }
#endif
#endif /* HAVE_SW_TONE_CONTROLS */
#if defined(AUDIOHW_HAVE_BASS_CUTOFF)
void audiohw_set_bass_cutoff(int value)       { (void)value; }
#endif
#if defined(AUDIOHW_HAVE_TREBLE_CUTOFF)
void audiohw_set_treble_cutoff(int value)     { (void)value; }
#endif
#if defined(AUDIOHW_HAVE_EQ)
void audiohw_set_eq_band_gain(unsigned band, int value)      { (void)band; (void)value; }
#endif
#if defined(AUDIOHW_HAVE_EQ_FREQUENCY)
void audiohw_set_eq_band_frequency(unsigned band, int value) { (void)band; (void)value; }
#endif
#if defined(AUDIOHW_HAVE_EQ_WIDTH)
void audiohw_set_eq_band_width(unsigned band, int value)     { (void)band; (void)value; }
#endif
#if defined(AUDIOHW_HAVE_DEPTH_3D)
void audiohw_set_depth_3d(int value)          { (void)value; }
#endif
#if defined(AUDIOHW_HAVE_LINEOUT)
void audiohw_set_lineout_volume(int vol_l, int vol_r) { (void)vol_l; (void)vol_r; }
#endif
#if defined(AUDIOHW_HAVE_FILTER_ROLL_OFF)
void audiohw_set_filter_roll_off(int value)   { (void)value; }
#endif

void audiohw_close(void) {}
