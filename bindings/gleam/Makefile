# Builds the shared rockbox_ffi_nif into priv/, linking the librockbox_ffi
# static archive. Gleam copies this project's priv/ into its build output, so
# the NIF loader (src/rockbox_ffi_nif.erl) finds the .so at runtime.
#
#   make          # build priv/rockbox_ffi_nif.so
#   gleam test    # (run make first)

REPO_ROOT := $(abspath $(CURDIR)/../..)
INCLUDE   := $(REPO_ROOT)/include
STATICLIB := $(REPO_ROOT)/target/release/librockbox_ffi.a

ERTS_INCLUDE := $(shell erl -noshell -eval \
  'io:format("~ts", [filename:join([code:root_dir(), "erts-" ++ erlang:system_info(version), "include"])])' \
  -s init stop)

PRIV   := priv
NIF_SO := $(PRIV)/rockbox_ffi_nif.so
C_SRC  := c_src/rockbox_ffi_nif.c

CFLAGS := -O2 -fPIC -std=c11 -Wall -I$(ERTS_INCLUDE) -I$(INCLUDE)

UNAME := $(shell uname)
ifeq ($(UNAME),Darwin)
  LDFLAGS := -dynamiclib -Wl,-undefined,dynamic_lookup -lc++
  FRAMEWORKS := -framework CoreAudio -framework AudioToolbox \
                -framework AudioUnit -framework CoreFoundation \
                -framework Security -framework CoreServices
else
  LDFLAGS := -shared -lasound -lunwind -ldbus-1 -lstdc++ -lm -lpthread
  FRAMEWORKS :=
endif

all: $(NIF_SO)

$(STATICLIB):
	cd $(REPO_ROOT) && cargo build --release -p rockbox-ffi

$(NIF_SO): $(C_SRC) $(STATICLIB) | $(PRIV)
	cc $(CFLAGS) $(C_SRC) $(STATICLIB) $(LDFLAGS) $(FRAMEWORKS) -o $@

$(PRIV):
	mkdir -p $(PRIV)

clean:
	rm -f $(NIF_SO)

.PHONY: all clean
