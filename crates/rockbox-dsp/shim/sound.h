/* Stub — shadows firmware/export/sound.h. The DSP only needs the channel
 * configuration enum (values must match firmware/export/audiohw.h). */
#ifndef RBDSP_SOUND_H
#define RBDSP_SOUND_H

enum AUDIOHW_CHANNEL_CONFIG
{
    SOUND_CHAN_STEREO,
    SOUND_CHAN_MONO,
    SOUND_CHAN_CUSTOM,
    SOUND_CHAN_MONO_LEFT,
    SOUND_CHAN_MONO_RIGHT,
    SOUND_CHAN_KARAOKE,
    SOUND_CHAN_SWAP,
    SOUND_CHAN_NUM_MODES,
};

#endif
