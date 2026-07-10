#             __________               __   ___.
#   Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
#   Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
#   Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
#   Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
#                     \/            \/     \/    \/            \/
#

INCLUDES += -I$(APPSDIR) $(patsubst %,-I$(APPSDIR)/%,$(subst :, ,$(APPEXTRA)))
SRC += $(call preprocess, $(APPSDIR)/SOURCES)
LIB_SRC += $(call preprocess, $(APPSDIR)/SOURCES)
LIB_SRC += $(BUILDDIR)/lang/lang_core.c
LIB_SRC += $(BUILDDIR)/apps/bitmaps/mono/default_icons.c
LIB_SRC += $(BUILDDIR)/apps/bitmaps/native/rockboxlogo.320x98x16.c
LIB_SRC += $(BUILDDIR)/apps/bitmaps/native/usblogo.176x48x16.c

# apps/features.txt is a file that (is preprocessed and) lists named features
# based on defines in the config-*.h files. The named features will be passed
# to genlang and thus (translated) phrases can be used based on those names.
# button.h is included for the HAS_BUTTON_HOLD define.
#
# Kludge: depends on config.o which only depends on config-*.h to have config.h
# changes trigger a genlang re-run
#

ifneq (,$(USE_LTO))
$(BUILDDIR)/apps/features: PPCFLAGS += -DUSE_LTO
endif

$(BUILDDIR)/apps/features: $(APPSDIR)/features.txt  $(BUILDDIR)/firmware/common/config.o
	$(SILENT)mkdir -p $(BUILDDIR)/apps
	$(SILENT)mkdir -p $(BUILDDIR)/lang
	$(call PRINTS,PP $(<F))
	$(SILENT)$(CC) $(PPCFLAGS) \
                 -E -P -imacros "config.h" -imacros "button.h" -x c $< | \
		grep -v "^#" | grep -v "^ *$$" > $(BUILDDIR)/apps/features; \

$(BUILDDIR)/apps/genlang-features:  $(BUILDDIR)/apps/features
	$(call PRINTS,GEN $(subst $(BUILDDIR)/,,$@))tr \\n : < $< > $@

ASMDEFS_SRC += $(APPSDIR)/core_asmdefs.c


ROCKBOXLIB = $(BUILDDIR)/librockbox.a

ROCKBOXLIB_OBJ := $(call c2obj, $(LIB_SRC))

$(ROCKBOXLIB): $(ROCKBOXLIB_OBJ)
	$(SILENT)$(shell rm -f $@)
	$(call PRINTS,AR $(@F))$(AR) rcs $@ $^ >/dev/null
