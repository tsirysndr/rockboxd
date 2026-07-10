#             __________               __   ___.
#   Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
#   Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
#   Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
#   Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
#                     \/            \/     \/    \/            \/
#

CODECDIR = $(RBCODEC_BLD)/codecs
CODECS_SRC := $(call preprocess, $(RBCODECLIB_DIR)/codecs/SOURCES)
OTHER_SRC += $(CODECS_SRC)

CODECS := $(CODECS_SRC:.c=.codec)
CODECS := $(subst $(RBCODECLIB_DIR),$(RBCODEC_BLD),$(CODECS))

# Static-link mode: produce one .a per codec instead of one .codec dlopen lib.
# Triggered by the Android cdylib build (CODECS_STATIC=1 + -DCODECS_STATIC).
ifdef CODECS_STATIC
 CODECS := $(CODECS_SRC:.c=.a)
 CODECS := $(subst $(RBCODECLIB_DIR),$(RBCODEC_BLD),$(CODECS))
 CODECFLAGS += -DCODECS_STATIC
endif

# the codec helper library
include $(RBCODECLIB_DIR)/codecs/lib/libcodec.make
OTHER_INC += -I$(RBCODECLIB_DIR)/codecs/lib

# extra libraries
CODEC_LIBS := $(CODECLIB) $(FIXEDPOINTLIB)

# compile flags for codecs
CODECFLAGS := $(CFLAGS) $(RBCODEC_CFLAGS) -fstrict-aliasing \
			 -I$(RBCODECLIB_DIR)/codecs -I$(RBCODECLIB_DIR)/codecs/lib -DCODEC

ifdef APP_TYPE
 CODECLDFLAGS = $(SHARED_LDFLAGS)
 ifneq ($(UNAME), Darwin)
  CODECLDFLAGS += -Wl,--gc-sections
 endif
 CODECFLAGS += $(SHARED_CFLAGS) # <-- from Makefile
else
 CODECLDFLAGS = -T$(CODECLINK_LDS) -Wl,--gc-sections
endif
CODECLDFLAGS += -Wl,$(LDMAP_OPT),$(CODECDIR)/$*.map $(GLOBAL_LDOPTS)

ifdef USE_LTO
 CODECLDFLAGS += -fno-builtin -ffreestanding
 CODECFLAGS += -fno-builtin -ffreestanding
 CODECLDFLAGS += -e __header
endif


# the codec libraries
include $(RBCODECLIB_DIR)/codecs/demac/libdemac.make
include $(RBCODECLIB_DIR)/codecs/liba52/liba52.make
include $(RBCODECLIB_DIR)/codecs/libalac/libalac.make
include $(RBCODECLIB_DIR)/codecs/libasap/libasap.make
include $(RBCODECLIB_DIR)/codecs/libasf/libasf.make
include $(RBCODECLIB_DIR)/codecs/libayumi/libayumi.make
include $(RBCODECLIB_DIR)/codecs/libfaad/libfaad.make
include $(RBCODECLIB_DIR)/codecs/libffmpegFLAC/libffmpegFLAC.make
include $(RBCODECLIB_DIR)/codecs/libm4a/libm4a.make
include $(RBCODECLIB_DIR)/codecs/libmad/libmad.make
include $(RBCODECLIB_DIR)/codecs/libmusepack/libmusepack.make
include $(RBCODECLIB_DIR)/codecs/libspc/libspc.make
include $(RBCODECLIB_DIR)/codecs/libspeex/libspeex.make
include $(RBCODECLIB_DIR)/codecs/libtremor/libtremor.make
include $(RBCODECLIB_DIR)/codecs/libwavpack/libwavpack.make
include $(RBCODECLIB_DIR)/codecs/libwma/libwma.make
include $(RBCODECLIB_DIR)/codecs/libwmapro/libwmapro.make
include $(RBCODECLIB_DIR)/codecs/libcook/libcook.make
include $(RBCODECLIB_DIR)/codecs/librm/librm.make
include $(RBCODECLIB_DIR)/codecs/libatrac/libatrac.make
include $(RBCODECLIB_DIR)/codecs/libpcm/libpcm.make
include $(RBCODECLIB_DIR)/codecs/libtta/libtta.make
include $(RBCODECLIB_DIR)/codecs/libgme/libay.make
include $(RBCODECLIB_DIR)/codecs/libgme/libgbs.make
include $(RBCODECLIB_DIR)/codecs/libgme/libhes.make
include $(RBCODECLIB_DIR)/codecs/libgme/libnsf.make
include $(RBCODECLIB_DIR)/codecs/libgme/libsgc.make
include $(RBCODECLIB_DIR)/codecs/libgme/libvgm.make
include $(RBCODECLIB_DIR)/codecs/libgme/libkss.make
include $(RBCODECLIB_DIR)/codecs/libgme/libemu2413.make
include $(RBCODECLIB_DIR)/codecs/libopus/libopus.make
ifneq ($(MEMORYSIZE),2)
# cRSID (Commodore 64 SID format) uses GCC nested-function syntax that
# NDK clang rejects strictly. Skip on the Android cdylib build; the .sid
# format is niche and can be re-enabled later if demand surfaces.
ifndef CODECS_STATIC
include $(RBCODECLIB_DIR)/codecs/cRSID/cRSID.make
endif
endif

ifeq ($(shell expr $(GCCNUM) \> 600),1)
$(MUSEPACKLIB): CODECFLAGS += -Wno-shift-negative-value
$(WMALIB): CODECFLAGS += -Wno-shift-negative-value
endif

ifndef DEBUG
# set CODECFLAGS per codec lib, since gcc takes the last -Ox and the last
# in a -ffoo -fno-foo pair, there is no need to filter them out
$(CODECLIB) : CODECFLAGS += -O1
$(A52LIB) : CODECFLAGS += -O1
$(ALACLIB) : CODECFLAGS += -O1
$(ASAPLIB) : CODECFLAGS += -O1
$(ASFLIB) : CODECFLAGS += -O2
$(ATRACLIB) : CODECFLAGS += -O1
$(AYLIB) : CODECFLAGS += -O2
$(AYUMILIB) : CODECFLAGS += -O3
$(COOKLIB): CODECFLAGS += -O1
$(DEMACLIB) : CODECFLAGS += -O3
$(FAADLIB) : CODECFLAGS += -O2
$(FFMPEGFLACLIB) : CODECFLAGS += -O2
$(GBSLIB) : CODECFLAGS +=  -O2
$(HESLIB) : CODECFLAGS +=  -O2
$(KSSLIB) : CODECFLAGS +=  -O2
$(M4ALIB) : CODECFLAGS += -O3
$(MADLIB) : CODECFLAGS += -O2
$(MUSEPACKLIB) : CODECFLAGS += -O1
$(NSFLIB) : CODECFLAGS +=  -O2
$(OPUSLIB) : CODECFLAGS +=  -O2
$(PCMSLIB) : CODECFLAGS += -O1
$(RMLIB) : CODECFLAGS += -O3
$(SGCLIB) : CODECFLAGS +=  -O2
$(SPCLIB) : CODECFLAGS +=  -O1
$(SPEEXLIB) : CODECFLAGS += -O2
$(TREMORLIB) : CODECFLAGS += -O2
$(TTALIB) : CODECFLAGS += -O2
$(VGMLIB) : CODECFLAGS +=  -O2
$(EMU2413LIB) : CODECFLAGS +=  -O3
$(WAVPACKLIB) : CODECFLAGS += -O1
$(WMALIB) : CODECFLAGS += -O2
$(WMAPROLIB) : CODECFLAGS += -O1
$(WMAVOICELIB) : CODECFLAGS += -O1
$(CRSID) : CODECFLAGS += -O3

# fine-tuning of CODECFLAGS per cpu arch
ifeq ($(ARCH),arch_arm)
  # redo per arm generation
  $(ALACLIB) : CODECFLAGS += -O2
  $(AYLIB) : CODECFLAGS +=  -O1
  $(EMU2413LIB) : CODECFLAGS +=  -O3
  $(GBSLIB) : CODECFLAGS +=  -O1
  $(HESLIB) : CODECFLAGS +=  -O1
  $(KSSLIB) : CODECFLAGS +=  -O1
  $(MADLIB) : CODECFLAGS += -O1
  $(NSFLIB) : CODECFLAGS +=  -O1
  $(SPEEXLIB) : CODECFLAGS += -O2
  $(SGCLIB) : CODECFLAGS +=  -O1
  $(VGMLIB) : CODECFLAGS +=  -O1
  $(WAVPACKLIB) : CODECFLAGS += -O3
  ifneq (,$(findstring ctru, $(MODELNAME)))
    # segfault with -O1
    $(SPCLIB) : CODECFLAGS += -O2
  endif
else ifeq ($(ARCH),arch_m68k)
  $(CODECLIB) : CODECFLAGS += -O2
  $(A52LIB) : CODECFLAGS += -O2
  $(ASFLIB) : CODECFLAGS += -O3
  $(ATRACLIB) : CODECFLAGS += -O2
  $(COOKLIB): CODECFLAGS += -O2
  $(DEMACLIB) : CODECFLAGS += -O2
  $(FFMPEGFLACLIB) : CODECFLAGS += -Os
  $(SPCLIB) : CODECFLAGS +=  -O3
  $(WMAPROLIB) : CODECFLAGS += -O3
  $(WMAVOICELIB) : CODECFLAGS += -O2
else ifeq ($(ARCH),arch_mips)
  $(OPUSLIB) : CODECFLAGS += -fno-strict-aliasing
endif

ifeq ($(MEMORYSIZE),2)
  $(CODECLIB) : CODECFLAGS += -Os
  $(ASFLIB) : CODECFLAGS += -Os
  $(WMALIB) : CODECFLAGS += -Os
endif
endif #ifndef DEBUG

ifndef APP_TYPE
  CONFIGFILE := $(FIRMDIR)/export/config/$(MODELNAME).h
  CODEC_LDS := $(APPSDIR)/plugins/plugin.lds # codecs and plugins use same file
  CODECLINK_LDS := $(CODECDIR)/codec.link
endif

CODEC_CRT0 := $(CODECDIR)/codec_crt0.o

ifndef CODECS_STATIC
# Static-link mode skips the codec linker script entirely — there is no
# per-codec final link step, just objcopy + ar.
$(CODECS): $(CODEC_CRT0) $(CODECLINK_LDS)

$(CODECLINK_LDS): $(CODEC_LDS) $(CONFIGFILE)
	$(call PRINTS,PP $(@F))
	$(shell mkdir -p $(dir $@))
	$(call preprocess2file, $<, $@, -DCODEC)
endif

# codec/library dependencies
$(CODECDIR)/spc.codec : $(CODECDIR)/libspc.a
$(CODECDIR)/mpa.codec : $(CODECDIR)/libmad.a $(CODECDIR)/libasf.a
$(CODECDIR)/a52.codec : $(CODECDIR)/liba52.a
$(CODECDIR)/flac.codec : $(CODECDIR)/libffmpegFLAC.a
$(CODECDIR)/vorbis.codec : $(CODECDIR)/libtremor.a $(TLSFLIB) $(SETJMPLIB)
$(CODECDIR)/speex.codec : $(CODECDIR)/libspeex.a
$(CODECDIR)/mpc.codec : $(CODECDIR)/libmusepack.a
$(CODECDIR)/wavpack.codec : $(CODECDIR)/libwavpack.a
$(CODECDIR)/alac.codec : $(CODECDIR)/libalac.a $(CODECDIR)/libm4a.a
$(CODECDIR)/aac.codec : $(CODECDIR)/libfaad.a $(CODECDIR)/libm4a.a
$(CODECDIR)/shorten.codec : $(CODECDIR)/libffmpegFLAC.a
$(CODECDIR)/ape-pre.map : $(CODECDIR)/libdemac-pre.a
$(CODECDIR)/ape.codec : $(CODECDIR)/libdemac.a
$(CODECDIR)/wma.codec : $(CODECDIR)/libwma.a $(CODECDIR)/libasf.a
$(CODECDIR)/wmapro.codec : $(CODECDIR)/libwmapro.a $(CODECDIR)/libasf.a
$(CODECDIR)/wavpack_enc.codec: $(CODECDIR)/libwavpack.a
$(CODECDIR)/asap.codec : $(CODECDIR)/libasap.a
$(CODECDIR)/cook.codec : $(CODECDIR)/libcook.a $(CODECDIR)/librm.a
$(CODECDIR)/raac.codec : $(CODECDIR)/libfaad.a $(CODECDIR)/librm.a
$(CODECDIR)/a52_rm.codec : $(CODECDIR)/liba52.a $(CODECDIR)/librm.a
$(CODECDIR)/atrac3_rm.codec : $(CODECDIR)/libatrac.a $(CODECDIR)/librm.a
$(CODECDIR)/atrac3_oma.codec : $(CODECDIR)/libatrac.a
$(CODECDIR)/aiff.codec : $(CODECDIR)/libpcm.a
$(CODECDIR)/wav.codec : $(CODECDIR)/libpcm.a
$(CODECDIR)/smaf.codec : $(CODECDIR)/libpcm.a
$(CODECDIR)/au.codec : $(CODECDIR)/libpcm.a
$(CODECDIR)/vox.codec : $(CODECDIR)/libpcm.a
$(CODECDIR)/wav64.codec : $(CODECDIR)/libpcm.a
$(CODECDIR)/tta.codec : $(CODECDIR)/libtta.a
$(CODECDIR)/ay.codec : $(CODECDIR)/libay.a
$(CODECDIR)/vtx.codec : $(CODECDIR)/libayumi.a
$(CODECDIR)/gbs.codec : $(CODECDIR)/libgbs.a
$(CODECDIR)/hes.codec : $(CODECDIR)/libhes.a
$(CODECDIR)/nsf.codec : $(CODECDIR)/libnsf.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/sgc.codec : $(CODECDIR)/libsgc.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/vgm.codec : $(CODECDIR)/libvgm.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/kss.codec : $(CODECDIR)/libkss.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/opus.codec : $(CODECDIR)/libopus.a $(TLSFLIB)
$(CODECDIR)/sid.codec : $(CODECDIR)/cRSID.a
$(CODECDIR)/aac_bsf.codec : $(CODECDIR)/libfaad.a

ifndef CODECS_STATIC
$(CODECS): $(CODEC_LIBS) # this must be last in codec dependency list
else
# In CODECS_STATIC mode there is no per-codec link step, so CODEC_LIBS is not
# linked into each .a archive. But libcodec.a (CODECLIB) must still be built
# so cargo can link it into the final cdylib. Add it as an order-only dep of
# the per-codec archives so 'make lib' builds it as a side-effect.
$(CODECS): $(CODECLIB)
endif

# pattern rule for compiling codecs
$(CODECDIR)/%.o: $(RBCODECLIB_DIR)/codecs/%.c
	$(SILENT)mkdir -p $(dir $@)
	$(call PRINTS,CC $(subst $(ROOTDIR)/,,$<))$(CC) \
		-I$(dir $<) $(CODECFLAGS) -c $< -o $@

# pattern rule for compiling codecs
$(CODECDIR)/%.o: $(RBCODECLIB_DIR)/codecs/%.S
	$(SILENT)mkdir -p $(dir $@)
	$(call PRINTS,CC $(subst $(ROOTDIR)/,,$<))$(CC) \
		-I$(dir $<) $(CODECFLAGS) -c $< -o $@

$(CODECDIR)/%-pre.map: $(CODEC_CRT0) $(CODECLINK_LDS) $(CODECDIR)/%.o $(CODEC_LIBS)
	$(call PRINTS,LD $(@F))$(CC) $(CODECFLAGS) -o $(CODECDIR)/$*-pre.elf \
		$(filter %.o, $^) \
		$(filter-out $(CODECLIB),$(filter %.a, $+)) $(CODECLIB) \
		-lgcc $(subst .map,-pre.map,$(CODECLDFLAGS))

$(CODECDIR)/%.codec: $(CODECDIR)/%.o
	$(call PRINTS,LD $(@F))$(CC) $(CODECFLAGS) -o $(CODECDIR)/$*.elf \
		$(filter %.o, $^) \
		$(filter %.a, $+) \
		-lgcc $(CODECLDFLAGS)
	$(SILENT)$(call objcopy_plugin,$(CODECDIR)/$*.elf,$@)

# CODECS_STATIC mode: produce a per-codec static archive with __header
# renamed to __header_<name> so all 44 codecs can co-link in one binary.
# objcopy modifies the .o in place; idempotent on re-run (the unrenamed
# __header isn't present anymore so the second pass is a no-op).
ifdef CODECS_STATIC
ifneq (,$(findstring wasm_app,$(APP_TYPE)))
# WASM: llvm-objcopy --redefine-sym is not supported for WASM object format.
# Rename the four colliding symbols at compile time via -DFOO=FOO_name instead.
# The generic compile rule (root.make line 511) uses $(CFLAGS); target-specific
# CFLAGS here propagate to the .o prerequisite build, so setting them on the
# .a target is sufficient.
CODEC_NAMES_WASM := $(notdir $(CODECS:.a=))

define WASM_CODEC_CFLAGS
$(CODECDIR)/$(1).a: CODECFLAGS += \
    -D__header=__header_$(1)       -D___header=___header_$(1) \
    -Dcodec_main=codec_main_$(1)   -D_codec_main=_codec_main_$(1) \
    -Dcodec_run=codec_run_$(1)     -D_codec_run=_codec_run_$(1) \
    -Dcodec_start=codec_start_$(1) -D_codec_start=_codec_start_$(1)
endef
$(foreach c,$(CODEC_NAMES_WASM),$(eval $(call WASM_CODEC_CFLAGS,$(c))))

# Per-codec crt0: codec_crt0.c compiled with per-codec renaming defines.
# codec_start calls codec_main; both must be renamed so they match the codec .o.
define WASM_CODEC_CRT0
$(CODECDIR)/$(1)-crt0.o: $(RBCODECLIB_DIR)/codecs/codec_crt0.c
	$$(call PRINTS,CC codec_crt0 [$(1)])
	$$(SILENT) $$(CC) $$(CFLAGS) \
	    -Dcodec_start=codec_start_$(1) -D_codec_start=_codec_start_$(1) \
	    -Dcodec_main=codec_main_$(1)   -D_codec_main=_codec_main_$(1) \
	    -c -o $$@ $$<
endef
$(foreach c,$(CODEC_NAMES_WASM),$(eval $(call WASM_CODEC_CRT0,$(c))))

# WASM archive rule: symbols renamed at compile time, just archive.
$(CODECDIR)/%.a: $(CODECDIR)/%.o $(CODECDIR)/%-crt0.o
	$(call PRINTS,STATIC $(@F))
	$(SILENT) $(AR) rcs $@ $< $(CODECDIR)/$*-crt0.o

else
# ELF / Mach-O: rename symbols post-compilation with objcopy --redefine-sym.
# objcopy --redefine-sym is given both the ELF (Linux/Android NDK) and the
# Mach-O (macOS smoke-test) symbol manglings — Mach-O prepends an extra
# leading underscore to C symbols. Four symbols per codec collide when
# all are linked into one binary:
#   __header      — defined by CODEC_HEADER macro
#   codec_main    — codec entry point (referenced from __header indirectly
#                   via codec_start)
#   codec_run     — codec decode loop  (referenced from __header directly)
#   codec_start   — wrapper from codec_crt0.c that calls codec_main
#                   (referenced from __header directly)
# All four get renamed per-codec so they coexist in the cdylib. codec_start
# lives in codec_crt0.o, so we also archive a per-codec renamed copy of
# codec_crt0 into each codec's .a — that gives us a codec_start_<name>
# that calls codec_main_<name> for that codec.
$(CODECDIR)/%.a: $(CODECDIR)/%.o $(CODEC_CRT0)
	$(call PRINTS,STATIC $(@F))
	$(SILENT) $(OC) --redefine-sym __header=__header_$* \
	                --redefine-sym ___header=___header_$* \
	                --redefine-sym codec_main=codec_main_$* \
	                --redefine-sym _codec_main=_codec_main_$* \
	                --redefine-sym codec_run=codec_run_$* \
	                --redefine-sym _codec_run=_codec_run_$* \
	                --redefine-sym codec_start=codec_start_$* \
	                --redefine-sym _codec_start=_codec_start_$* $<
	$(SILENT) cp $(CODEC_CRT0) $(CODECDIR)/$*-crt0.o
	$(SILENT) $(OC) --redefine-sym codec_start=codec_start_$* \
	                --redefine-sym _codec_start=_codec_start_$* \
	                --redefine-sym codec_main=codec_main_$* \
	                --redefine-sym _codec_main=_codec_main_$* \
	                $(CODECDIR)/$*-crt0.o
	$(SILENT) $(AR) rcs $@ $< $(CODECDIR)/$*-crt0.o
endif

# Per-codec helper-lib dependencies (both WASM and ELF/Mach-O).
# When a codec is added/removed in SOURCES, mirror its existing .codec dep
# line below with .a on the LHS.
$(CODECDIR)/spc.a          : $(CODECDIR)/libspc.a
$(CODECDIR)/mpa.a          : $(CODECDIR)/libmad.a $(CODECDIR)/libasf.a
$(CODECDIR)/a52.a          : $(CODECDIR)/liba52.a
$(CODECDIR)/flac.a         : $(CODECDIR)/libffmpegFLAC.a
$(CODECDIR)/vorbis.a       : $(CODECDIR)/libtremor.a $(TLSFLIB) $(SETJMPLIB)
$(CODECDIR)/speex.a        : $(CODECDIR)/libspeex.a
$(CODECDIR)/mpc.a          : $(CODECDIR)/libmusepack.a
$(CODECDIR)/wavpack.a      : $(CODECDIR)/libwavpack.a
$(CODECDIR)/alac.a         : $(CODECDIR)/libalac.a $(CODECDIR)/libm4a.a
$(CODECDIR)/aac.a          : $(CODECDIR)/libfaad.a $(CODECDIR)/libm4a.a
$(CODECDIR)/shorten.a      : $(CODECDIR)/libffmpegFLAC.a
$(CODECDIR)/ape.a          : $(CODECDIR)/libdemac.a
$(CODECDIR)/wma.a          : $(CODECDIR)/libwma.a $(CODECDIR)/libasf.a
$(CODECDIR)/wmapro.a       : $(CODECDIR)/libwmapro.a $(CODECDIR)/libasf.a
$(CODECDIR)/wavpack_enc.a  : $(CODECDIR)/libwavpack.a
$(CODECDIR)/asap.a         : $(CODECDIR)/libasap.a
$(CODECDIR)/cook.a         : $(CODECDIR)/libcook.a $(CODECDIR)/librm.a
$(CODECDIR)/raac.a         : $(CODECDIR)/libfaad.a $(CODECDIR)/librm.a
$(CODECDIR)/a52_rm.a       : $(CODECDIR)/liba52.a $(CODECDIR)/librm.a
$(CODECDIR)/atrac3_rm.a    : $(CODECDIR)/libatrac.a $(CODECDIR)/librm.a
$(CODECDIR)/atrac3_oma.a   : $(CODECDIR)/libatrac.a
$(CODECDIR)/aiff.a         : $(CODECDIR)/libpcm.a
$(CODECDIR)/wav.a          : $(CODECDIR)/libpcm.a
$(CODECDIR)/smaf.a         : $(CODECDIR)/libpcm.a
$(CODECDIR)/au.a           : $(CODECDIR)/libpcm.a
$(CODECDIR)/vox.a          : $(CODECDIR)/libpcm.a
$(CODECDIR)/wav64.a        : $(CODECDIR)/libpcm.a
$(CODECDIR)/tta.a          : $(CODECDIR)/libtta.a
$(CODECDIR)/ay.a           : $(CODECDIR)/libay.a
$(CODECDIR)/vtx.a          : $(CODECDIR)/libayumi.a
$(CODECDIR)/gbs.a          : $(CODECDIR)/libgbs.a
$(CODECDIR)/hes.a          : $(CODECDIR)/libhes.a
$(CODECDIR)/nsf.a          : $(CODECDIR)/libnsf.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/sgc.a          : $(CODECDIR)/libsgc.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/vgm.a          : $(CODECDIR)/libvgm.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/kss.a          : $(CODECDIR)/libkss.a $(CODECDIR)/libemu2413.a
$(CODECDIR)/opus.a         : $(CODECDIR)/libopus.a $(TLSFLIB)
$(CODECDIR)/sid.a          : $(CODECDIR)/cRSID.a
$(CODECDIR)/aac_bsf.a      : $(CODECDIR)/libfaad.a
endif
