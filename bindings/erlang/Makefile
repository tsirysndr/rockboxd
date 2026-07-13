# Builds the shared rockbox_ffi_nif into priv/, linking the librockbox_ffi
# static archive.
#
# NOTE: On a consumer machine the Hex package downloads a PREBUILT
# priv/rockbox_ffi_nif.so via elixir_make + cc_precompiler and this Makefile is
# NEVER run. It only runs (a) inside the monorepo during CI precompilation, or
# (b) when a user opts into a from-source build with ROCKBOX_FFI_BUILD=1 — which
# requires a full monorepo checkout (Cargo workspace + include/ header). It is
# not runnable from a bare Hex package: the 30 MB static archive and the C
# header are not shipped.
#
# Driven by `elixir_make` (mix compile), but also runnable standalone: `make`.

REPO_ROOT := $(abspath $(CURDIR)/../..)

# Overridable so CI / cross-builds can point at prebuilt artifacts.
INCLUDE   ?= $(REPO_ROOT)/include
STATICLIB ?= $(REPO_ROOT)/target/release/librockbox_ffi.a

CC ?= cc

# Erlang erts include dir (holds erl_nif.h), discovered from the running erl.
ERTS_INCLUDE := $(shell erl -noshell -eval \
  'io:format("~ts", [filename:join([code:root_dir(), "erts-" ++ erlang:system_info(version), "include"])])' \
  -s init stop)

PRIV     := priv
NIF_SO   := $(PRIV)/rockbox_ffi_nif.so
C_SRC    := c_src/rockbox_ffi_nif.c

CFLAGS  := -O2 -fPIC -std=c11 -Wall -I$(ERTS_INCLUDE) -I$(INCLUDE)

# NOTE: this Makefile uses GNU-make syntax (ifeq, order-only prereqs). On *BSD,
# where `make` is BSD make, run it as `gmake` (the CI sets MAKE=gmake).
UNAME := $(shell uname)
ifeq ($(UNAME),Darwin)
  # enif_* symbols are resolved by the BEAM at load time.
  LDFLAGS := -dynamiclib -Wl,-undefined,dynamic_lookup -lc++
  FRAMEWORKS := -framework CoreAudio -framework AudioToolbox \
                -framework AudioUnit -framework CoreFoundation \
                -framework Security -framework CoreServices
else ifeq ($(UNAME),FreeBSD)
  # clang/libc++ toolchain; unwind lives in base (no -lunwind). pkg libs live
  # under /usr/local/lib, which isn't on the default linker search path.
  LDFLAGS := -shared -L/usr/local/lib -lasound -ldbus-1 -lc++ -lm -lpthread
  FRAMEWORKS :=
else ifeq ($(UNAME),NetBSD)
  # NetBSD's base toolchain is GCC/libstdc++ (no libc++), so link -lstdc++.
  # pkgsrc libs live under /usr/pkg/lib, which isn't on the default search path.
  LDFLAGS := -shared -L/usr/pkg/lib -lasound -ldbus-1 -lstdc++ -lm -lpthread
  FRAMEWORKS :=
else
  LDFLAGS := -shared -lasound -lunwind -ldbus-1 -lstdc++ -lm -lpthread
  FRAMEWORKS :=
endif

all: $(NIF_SO)

# Build the Rust static library if it isn't already present. This requires the
# monorepo's Cargo workspace; fail with a clear message when it's missing (e.g.
# someone forced a source build from a bare Hex package).
$(STATICLIB):
	@if [ ! -f "$(REPO_ROOT)/Cargo.toml" ]; then \
	  echo "ERROR: $(STATICLIB) is missing and no Cargo workspace was found at"; \
	  echo "       $(REPO_ROOT)."; \
	  echo ""; \
	  echo "The rockbox_ex_ffi Hex package installs a PRECOMPILED NIF and does not"; \
	  echo "build from source. Build from source only from a full rockboxd monorepo"; \
	  echo "checkout (bindings/elixir) with ROCKBOX_FFI_BUILD=1. If the precompiled"; \
	  echo "download failed, verify a prebuilt artifact exists for your OS/arch."; \
	  exit 1; \
	fi
	cd "$(REPO_ROOT)" && cargo build --release -p rockbox-ffi

$(NIF_SO): $(C_SRC) $(STATICLIB) | $(PRIV)
	$(CC) $(CFLAGS) $(C_SRC) $(STATICLIB) $(LDFLAGS) $(FRAMEWORKS) -o $@

$(PRIV):
	mkdir -p $(PRIV)

clean:
	rm -f $(NIF_SO)

.PHONY: all clean
