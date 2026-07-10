/***************************************************************************
 *             __________               __   ___.
 *   Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
 *   Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
 *   Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
 *   Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
 *                     \/            \/     \/    \/            \/
 *
 * Copyright (C) 2025 by Sho Tanimoto
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
#pragma once
#include <stddef.h>
#include <stdint.h>

#include "config.h"

struct pcm_sink_caps {
    const unsigned long* samprs;
    uint16_t             num_samprs;
    uint16_t             default_freq;
};

struct pcm_sink_ops {
    void (*init)(void);
    void (*postinit)(void);
    void (*set_freq)(uint16_t freq);
    void (*lock)(void);
    void (*unlock)(void);
    void (*play)(const void* addr, size_t size);
    void (*stop)(void);
};

struct pcm_sink {
    /* characteristics */
    const struct pcm_sink_caps caps;

    /* operations */
    const struct pcm_sink_ops ops;

    /* runtime states */
    unsigned long pending_freq;
    unsigned long configured_freq;
};

enum pcm_sink_ids {
    PCM_SINK_BUILTIN = 0,
#if (CONFIG_PLATFORM & PLATFORM_HOSTED)
    PCM_SINK_FIFO,
    PCM_SINK_AIRPLAY,
    PCM_SINK_SQUEEZELITE,
    PCM_SINK_UPNP,
    PCM_SINK_CHROMECAST,
    PCM_SINK_SNAPCAST_TCP,
#if defined(CODECS_STATIC) && !(CONFIG_PLATFORM & PLATFORM_WASM)
    PCM_SINK_CPAL,      /* cpal output — BUILTIN on Android cdylib and headless host */
#endif
#if (CONFIG_PLATFORM & PLATFORM_WASM)
    PCM_SINK_WEBAPI,    /* Web Audio API output for the WASM browser build */
#endif
#if !(CONFIG_PLATFORM & PLATFORM_WASM)
    /* Pinned to value 8 so it doesn't shift around when CPAL/WEBAPI are
     * conditionally compiled in.  Slot 7 stays NULL on regular host builds
     * (firmware/pcm.c skips NULL entries via `if (!sink) continue;`). */
    PCM_SINK_CMAF = 8,  /* CMAF (HLS + DASH) AAC-LC output */
#endif
/* Pinned to 9 — must come after CMAF = 8 so PCM_SINK_NUM resolves to 10
 * and sinks[9] is within the array bounds. */
#if defined(ARMHFHOST)
    PCM_SINK_ALSA = 9,  /* direct libasound sink for arm-linux-gnueabihf */
#endif
#endif
#ifdef USB_ENABLE_IAP
    PCM_SINK_IAP,
#endif
    PCM_SINK_NUM
};

/* defined in each platform pcm source */
extern struct pcm_sink builtin_pcm_sink;

#if (CONFIG_PLATFORM & PLATFORM_HOSTED)
/* FIFO/pipe sink — writes raw S16LE stereo PCM to a named FIFO or stdout */
extern struct pcm_sink fifo_pcm_sink;
void pcm_fifo_set_path(const char *path);

/* AirPlay (RAOP) sink — streams ALAC-encoded audio over RTP */
extern struct pcm_sink airplay_pcm_sink;

/* Squeezelite (Slim Protocol) sink — serves PCM via HTTP to squeezelite */
extern struct pcm_sink squeezelite_pcm_sink;

/* UPnP/DLNA sink — streams WAV over HTTP to UPnP renderers */
extern struct pcm_sink upnp_pcm_sink;
void pcm_upnp_set_http_port(uint16_t port);
void pcm_upnp_set_renderer_url(const char *url);

/* Chromecast sink — streams WAV over HTTP and loads via Cast protocol */
extern struct pcm_sink chromecast_pcm_sink;
void pcm_chromecast_set_http_port(uint16_t port);
void pcm_chromecast_set_device_host(const char *host);
void pcm_chromecast_set_device_port(uint16_t port);

/* Snapcast TCP sink — streams raw S16LE PCM to snapserver's tcp:// source */
extern struct pcm_sink tcp_pcm_sink;
void pcm_tcp_set_host(const char *host);
void pcm_tcp_set_port(uint16_t port);

#if !(CONFIG_PLATFORM & PLATFORM_WASM)
/* CMAF (HLS + DASH) sink — AAC-LC encodes PCM and serves fMP4 segments + manifests */
extern struct pcm_sink cmaf_pcm_sink;
void pcm_cmaf_set_http_port(uint16_t port);
void pcm_cmaf_set_bitrate(uint32_t bps);
#endif

#if defined(CODECS_STATIC) && !(CONFIG_PLATFORM & PLATFORM_WASM) && !defined(ARMHFHOST)
/* cpal sink — BUILTIN on the Android cdylib and headless macOS/Linux builds. */
extern struct pcm_sink cpal_pcm_sink;
#endif

#if defined(ARMHFHOST)
/* direct libasound sink for arm-linux-gnueabihf — bypasses cpal entirely. */
extern struct pcm_sink alsa_pcm_sink;
#endif

#if (CONFIG_PLATFORM & PLATFORM_WASM)
/* Web Audio API sink — BUILTIN on the WASM build. JS page receives PCM buffers
 * via Module.onPcmData(ptr, bytes, sampleRate) callback. */
extern struct pcm_sink webapi_pcm_sink;
#endif
#endif
