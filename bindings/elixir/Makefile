# Builds the shared rockbox_ffi_nif into priv/, linking the librockbox_ffi
# static archive. Driven by `elixir_make` (mix compile), but also runnable
# standalone: `make`.

REPO_ROOT := $(abspath $(CURDIR)/../..)
INCLUDE   := $(REPO_ROOT)/include
STATICLIB := $(REPO_ROOT)/target/release/librockbox_ffi.a

# Erlang erts include dir (holds erl_nif.h), discovered from the running erl.
ERTS_INCLUDE := $(shell erl -noshell -eval \
  'io:format("~ts", [filename:join([code:root_dir(), "erts-" ++ erlang:system_info(version), "include"])])' \
  -s init stop)

PRIV     := priv
NIF_SO   := $(PRIV)/rockbox_ffi_nif.so
C_SRC    := c_src/rockbox_ffi_nif.c

CFLAGS  := -O2 -fPIC -std=c11 -Wall -I$(ERTS_INCLUDE) -I$(INCLUDE)

UNAME := $(shell uname)
ifeq ($(UNAME),Darwin)
  # enif_* symbols are resolved by the BEAM at load time.
  LDFLAGS := -dynamiclib -Wl,-undefined,dynamic_lookup -lc++
  FRAMEWORKS := -framework CoreAudio -framework AudioToolbox \
                -framework AudioUnit -framework CoreFoundation \
                -framework Security -framework CoreServices
else
  LDFLAGS := -shared -lasound -lunwind -ldbus-1 -lstdc++ -lm -lpthread
  FRAMEWORKS :=
endif

all: $(NIF_SO)

# Ensure the Rust static library exists before linking the NIF.
$(STATICLIB):
	cd $(REPO_ROOT) && cargo build --release -p rockbox-ffi

$(NIF_SO): $(C_SRC) $(STATICLIB) | $(PRIV)
	cc $(CFLAGS) $(C_SRC) $(STATICLIB) $(LDFLAGS) $(FRAMEWORKS) -o $@

$(PRIV):
	mkdir -p $(PRIV)

clean:
	rm -f $(NIF_SO)

.PHONY: all clean
