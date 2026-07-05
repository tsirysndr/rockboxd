/* Stub — shadows apps/settings.h. The DSP sources reference no settings
 * (no global_settings use in lib/rbcodec/dsp), but some (afr.c) rely on
 * this header's include chain for the dsp_misc.h declarations. */
#ifndef RBDSP_SETTINGS_H
#define RBDSP_SETTINGS_H
#include "platform.h"
#include "dsp_misc.h"
#endif
