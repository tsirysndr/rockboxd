# Builds the shared rockbox_ffi_nif into priv/, linking the librockbox_ffi
# static archive. Gleam copies this project's priv/ into its build output, so
# the NIF loader (src/rockbox_ffi_nif.erl) finds the .so at runtime.
#
#   make                          # -> priv/rockbox_ffi_nif.so   (local dev)
#   make TARGET=x86_64-linux-gnu  # -> priv/rockbox_ffi_nif-x86_64-linux-gnu.so
#
# Gleam has NO compile step at `gleam add` time, so the published package ships
# ONE prebuilt .so per platform under priv/, named rockbox_ffi_nif-<TARGET>.so.
# CI (`bindings-gleam-release.yml`) builds each with the matching TARGET; the
# loader picks the right one by detecting the host triple, and falls back to the
# unsuffixed name a plain `make` produces for local development.
#
# NOTE: GNU-make syntax (ifeq). On *BSD run as `gmake`.

REPO_ROOT := $(abspath $(CURDIR)/../..)

# Overridable so CI / cross-builds can point at prebuilt artifacts.
INCLUDE   ?= $(REPO_ROOT)/include
STATICLIB ?= $(REPO_ROOT)/target/release/librockbox_ffi.a

CC ?= cc

ERTS_INCLUDE := $(shell erl -noshell -eval \
  'io:format("~ts", [filename:join([code:root_dir(), "erts-" ++ erlang:system_info(version), "include"])])' \
  -s init stop)

PRIV   := priv
C_SRC  := c_src/rockbox_ffi_nif.c

# TARGET, when set, suffixes the output so several arches coexist in priv/.
TARGET ?=
ifeq ($(strip $(TARGET)),)
  NIF_SO := $(PRIV)/rockbox_ffi_nif.so
else
  NIF_SO := $(PRIV)/rockbox_ffi_nif-$(TARGET).so
endif

CFLAGS := -O2 -fPIC -std=c11 -Wall -I$(ERTS_INCLUDE) -I$(INCLUDE)

UNAME := $(shell uname)
ifeq ($(UNAME),Darwin)
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

# Build the Rust static library if absent. Requires the monorepo Cargo
# workspace; fail with a clear message when it's missing.
$(STATICLIB):
	@if [ ! -f "$(REPO_ROOT)/Cargo.toml" ]; then \
	  echo "ERROR: $(STATICLIB) is missing and no Cargo workspace was found at"; \
	  echo "       $(REPO_ROOT)."; \
	  echo ""; \
	  echo "The rockbox_ffi Gleam package ships PRECOMPILED NIFs in priv/ and does"; \
	  echo "not build from source. Build from a full rockboxd monorepo checkout"; \
	  echo "(bindings/gleam) if you need a local .so."; \
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
