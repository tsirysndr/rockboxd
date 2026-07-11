/* SPDX-License-Identifier: GPL-2.0-or-later
 *
 * PCM sink that routes audio directly to libsndio via sio_write. Used for the
 * OpenBSD headless target: OpenBSD has no ALSA and cpal has no sndio backend,
 * so this is the native audio path (works with the sndiod server for mixing).
 *
 * The Rust implementation lives in crates/sndio-sink/src/lib.rs.
 * This C file mirrors pcm-alsa.c exactly — only the function prefix changes.
 */

#include "autoconf.h"
#include "config.h"

#include <pthread.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "pcm.h"
#include "pcm-internal.h"
#include "pcm_mixer.h"
#include "pcm_sampr.h"
#include "pcm_sink.h"

#define LOGF_ENABLE
#include "logf.h"

/* ── Rust FFI — defined in crates/sndio-sink/src/lib.rs ───────────────────── */
extern void pcm_sndio_init(void);
extern void pcm_sndio_postinit(void);
extern void pcm_sndio_set_sample_rate(uint32_t rate_hz);
extern void pcm_sndio_start(void);
extern void pcm_sndio_push(const void *data, size_t size);
extern void pcm_sndio_stop(void);
extern void pcm_sndio_flush(void);
extern bool pcm_sndio_is_running(void);

/* ── Writer-thread state ────────────────────────────────────────────────── */

static const void     *pcm_data    = NULL;
static size_t          pcm_size    = 0;
static pthread_mutex_t sndio_mtx;
static pthread_t       sndio_tid;
static volatile bool   sndio_running  = false;
static volatile bool   sndio_stop     = false;
static volatile bool   sndio_draining = false;

static void *sndio_thread(void *arg)
{
    (void)arg;

    while (!sndio_stop) {
        pthread_mutex_lock(&sndio_mtx);
        const void *data = pcm_data;
        size_t      size = pcm_size;
        pcm_data = NULL;
        pcm_size = 0;
        pthread_mutex_unlock(&sndio_mtx);

        if (!data || !size) {
            sndio_stop = true;
            break;
        }

        pcm_sndio_push(data, size);

        if (sndio_stop || !pcm_sndio_is_running())
            break;

        sndio_draining = true;
        pthread_mutex_lock(&sndio_mtx);
        bool got_more = pcm_play_dma_complete_callback(PCM_DMAST_OK,
                                                        &pcm_data, &pcm_size);
        pthread_mutex_unlock(&sndio_mtx);
        sndio_draining = false;

        if (!got_more) {
            logf("pcm-sndio: no more PCM data, ring draining");
            break;
        }

        pcm_play_dma_status_callback(PCM_DMAST_STARTED);
    }

    sndio_running = false;
    return NULL;
}

/* ── Sink ops ───────────────────────────────────────────────────────────── */

static void sink_dma_init(void)
{
    pthread_mutexattr_t attr;
    pthread_mutexattr_init(&attr);
    pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE);
    pthread_mutex_init(&sndio_mtx, &attr);
    pthread_mutexattr_destroy(&attr);
    pcm_sndio_init();
}

static void sink_dma_postinit(void)
{
    pcm_sndio_postinit();
}

static void sink_set_freq(uint16_t freq_index)
{
    pcm_sndio_set_sample_rate((uint32_t)hw_freq_sampr[freq_index]);
}

static void sink_lock(void)   { pthread_mutex_lock(&sndio_mtx); }
static void sink_unlock(void) { pthread_mutex_unlock(&sndio_mtx); }

static void sink_dma_start(const void *addr, size_t size)
{
    logf("pcm-sndio: start (%p, %zu)", addr, size);

    pthread_mutex_lock(&sndio_mtx);
    sndio_stop    = false;
    sndio_running = true;
    pcm_data      = NULL;
    pcm_size      = 0;
    pthread_mutex_unlock(&sndio_mtx);

    pcm_sndio_start();
    pcm_sndio_push(addr, size);

    pthread_mutex_lock(&sndio_mtx);
    bool got_more = pcm_play_dma_complete_callback(PCM_DMAST_OK,
                                                    &pcm_data, &pcm_size);
    pthread_mutex_unlock(&sndio_mtx);

    if (!got_more) {
        logf("pcm-sndio: single-chunk track");
        sndio_running = false;
        return;
    }

    pcm_play_dma_status_callback(PCM_DMAST_STARTED);
    pthread_create(&sndio_tid, NULL, sndio_thread, NULL);
}

static void sink_dma_stop(void)
{
    logf("pcm-sndio: stop (draining=%d)", (int)sndio_draining);

    sndio_stop = true;
    pcm_sndio_stop();

    if (!sndio_draining)
        pcm_sndio_flush();

    if (sndio_running) {
        pthread_join(sndio_tid, NULL);
        sndio_running = false;
    }

    pthread_mutex_lock(&sndio_mtx);
    pcm_data       = NULL;
    pcm_size       = 0;
    sndio_draining = false;
    pthread_mutex_unlock(&sndio_mtx);
}

/* ── Sink structs ───────────────────────────────────────────────────────── */

struct pcm_sink builtin_pcm_sink = {
    .caps = {
        .samprs       = hw_freq_sampr,
        .num_samprs   = HW_NUM_FREQ,
        .default_freq = HW_FREQ_DEFAULT,
    },
    .ops = {
        .init     = sink_dma_init,
        .postinit = sink_dma_postinit,
        .set_freq = sink_set_freq,
        .lock     = sink_lock,
        .unlock   = sink_unlock,
        .play     = sink_dma_start,
        .stop     = sink_dma_stop,
    },
};

struct pcm_sink sndio_pcm_sink = {
    .caps = {
        .samprs       = hw_freq_sampr,
        .num_samprs   = HW_NUM_FREQ,
        .default_freq = HW_FREQ_DEFAULT,
    },
    .ops = {
        .init     = sink_dma_init,
        .postinit = sink_dma_postinit,
        .set_freq = sink_set_freq,
        .lock     = sink_lock,
        .unlock   = sink_unlock,
        .play     = sink_dma_start,
        .stop     = sink_dma_stop,
    },
};
