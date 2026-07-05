/* Stub — shadows lib/rbcodec/metadata/replaygain.h (which drags in
 * metadata.h). The DSP only needs the dB→Q7.24 conversion helper;
 * its implementation lives in rbdsp_shim.c. */
#ifndef RBDSP_REPLAYGAIN_H
#define RBDSP_REPLAYGAIN_H

/* Get a sample scale factor in Q7.24 format from a gain value.
 * int_gain: gain in dB, multiplied by 100. */
long get_replaygain_int(long int_gain);

#endif
