# Builds the shared rockbox_ffi_nif NIF into priv/, linking the librockbox_ffi
# static archive. Used by BOTH the Elixir (rockbox_ex_ffi) and Gleam
# (rockbox_ffi) bindings, which depend on this package.
#
# Usage:
#   make                          # -> priv/rockbox_ffi_nif.so   (local dev)
#   make TARGET=x86_64-linux-gnu  # -> priv/rockbox_ffi_nif-x86_64-linux-gnu.so
#
# The published Hex package does NOT bundle the .so (it exceeds Hex's 8 MB
# tarball limit). CI (`bindings-erlang-release.yml`) builds each target on its
# own native runner with the matching TARGET and uploads them to the
# `erlang-v<version>` release; the loader (src/rockbox_ffi_nif.erl) downloads
# the matching one into the user cache on first load, verifying it against
# priv/rockbox_ffi_nif.manifest. A plain `make` (unsuffixed .so in priv) is used
# for local development and is preferred over any cached download.
#
# Requires the full monorepo checkout (Cargo workspace + include/ header); it is
# not runnable from a bare Hex package. NOTE: GNU-make syntax (ifeq). On *BSD run
# as `gmake` (the CI sets MAKE=gmake).

REPO_ROOT := $(abspath $(CURDIR)/../..)

# Overridable so CI / cross-builds can point at prebuilt artifacts.
INCLUDE   ?= $(REPO_ROOT)/include
STATICLIB ?= $(REPO_ROOT)/target/release/librockbox_ffi.a

CC ?= cc

# Erlang erts include dir (holds erl_nif.h), discovered from the running erl.
ERTS_INCLUDE := $(shell erl -noshell -eval \
  'io:format("~ts", [filename:join([code:root_dir(), "erts-" ++ erlang:system_info(version), "include"])])' \
  -s init stop)

PRIV   := priv
C_SRC  := c_src/rockbox_ffi_nif.c

# TARGET, when set, suffixes the output so several arches coexist in priv/ (the
# name the loader and the release-upload step expect). It only names the output
# file — each target is built natively on its own runner, not cross-compiled.
TARGET ?=
ifeq ($(strip $(TARGET)),)
  NIF_SO := $(PRIV)/rockbox_ffi_nif.so
else
  NIF_SO := $(PRIV)/rockbox_ffi_nif-$(TARGET).so
endif

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
	  echo "The rockbox_ffi_nif Hex package downloads a PRECOMPILED NIF on first"; \
	  echo "load and does not build from source. Build from source only from a full"; \
	  echo "rockboxd monorepo checkout (bindings/erlang). If the precompiled download"; \
	  echo "failed, verify a prebuilt artifact exists for your OS/arch."; \
	  exit 1; \
	fi
	cd "$(REPO_ROOT)" && cargo build --release -p rockbox-ffi

$(NIF_SO): $(C_SRC) $(STATICLIB) | $(PRIV)
	$(CC) $(CFLAGS) $(C_SRC) $(STATICLIB) $(LDFLAGS) $(FRAMEWORKS) -o $@

$(PRIV):
	mkdir -p $(PRIV)

clean:
	rm -f $(PRIV)/rockbox_ffi_nif*.so

.PHONY: all clean
