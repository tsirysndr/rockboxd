// Runtime-agnostic FFI description: one symbol table with abstract type
// tokens that each backend (bun.ts / deno.ts) maps to its own FFI types.

/** Abstract argument/return type tokens used by the symbol spec. */
export type Tok =
  | "void"
  | "ptr" // opaque handle / returned pointer
  | "u32"
  | "i32"
  | "u64"
  | "usize"
  | "f32"
  | "bool"
  | "strbuf" // input: a JS string marshalled as `const char *`
  | "i16in" // input: an Int16Array marshalled as `const int16_t *`
  | "outsize"; // input: a `size_t *` out-parameter

export interface SymSpec {
  args: Tok[];
  ret: Tok;
}

// Mirrors include/rockbox_ffi.h. Keep in sync with the C ABI.
export const SPEC: Record<string, SymSpec> = {
  rb_ffi_abi_version: { args: [], ret: "u32" },
  rb_string_free: { args: ["ptr"], ret: "void" },
  rb_buffer_free: { args: ["ptr", "usize"], ret: "void" },

  rb_dsp_new: { args: ["u32"], ret: "ptr" },
  rb_dsp_free: { args: ["ptr"], ret: "void" },
  rb_dsp_set_input_frequency: { args: ["ptr", "u32"], ret: "void" },
  rb_dsp_flush: { args: ["ptr"], ret: "void" },
  rb_dsp_eq_enable: { args: ["ptr", "bool"], ret: "void" },
  rb_dsp_set_tone: { args: ["ptr", "i32", "i32"], ret: "void" },
  rb_dsp_set_tone_cutoffs: { args: ["ptr", "i32", "i32"], ret: "void" },
  rb_dsp_set_surround: { args: ["ptr", "i32", "i32", "i32", "i32"], ret: "void" },
  rb_dsp_set_channel_config: { args: ["ptr", "i32"], ret: "void" },
  rb_dsp_set_stereo_width: { args: ["ptr", "i32"], ret: "void" },
  rb_dsp_set_compressor: {
    args: ["ptr", "i32", "i32", "i32", "i32", "i32", "i32"],
    ret: "void",
  },
  rb_dsp_set_replaygain: { args: ["ptr", "i32", "bool", "f32"], ret: "void" },
  rb_dsp_set_replaygain_gains: {
    args: ["ptr", "f32", "f32", "f32", "f32"],
    ret: "void",
  },
  rb_dsp_set_replaygain_gains_raw: {
    args: ["ptr", "u64", "u64", "u64", "u64"],
    ret: "void",
  },
  rb_dsp_set_eq_band: { args: ["ptr", "usize", "i32", "f32", "f32"], ret: "void" },
  rb_dsp_set_eq_precut: { args: ["ptr", "f32"], ret: "void" },
  rb_dsp_process: { args: ["ptr", "i16in", "usize", "outsize"], ret: "ptr" },

  rb_meta_read_json: { args: ["strbuf"], ret: "ptr" },
  rb_meta_probe: { args: ["strbuf"], ret: "ptr" },

  rb_player_new: { args: [], ret: "ptr" },
  rb_player_new_with_config: {
    args: [
      "u32", "f32", "f32", "i32", "f32", "bool", "i32", "u32", "u32", "u32",
      "u32", "i32",
    ],
    ret: "ptr",
  },
  rb_player_free: { args: ["ptr"], ret: "void" },
  rb_player_set_queue_json: { args: ["ptr", "strbuf"], ret: "void" },
  rb_player_enqueue: { args: ["ptr", "strbuf"], ret: "void" },
  rb_player_play: { args: ["ptr"], ret: "void" },
  rb_player_pause: { args: ["ptr"], ret: "void" },
  rb_player_toggle: { args: ["ptr"], ret: "void" },
  rb_player_stop: { args: ["ptr"], ret: "void" },
  rb_player_next: { args: ["ptr"], ret: "void" },
  rb_player_previous: { args: ["ptr"], ret: "void" },
  rb_player_skip_to: { args: ["ptr", "usize"], ret: "void" },
  rb_player_seek_ms: { args: ["ptr", "u64"], ret: "void" },
  rb_player_set_volume: { args: ["ptr", "f32"], ret: "void" },
  rb_player_set_crossfade: {
    args: ["ptr", "i32", "u32", "u32", "u32", "u32", "i32"],
    ret: "void",
  },
  rb_player_set_replaygain: { args: ["ptr", "i32", "f32", "bool"], ret: "void" },
  rb_player_volume: { args: ["ptr"], ret: "f32" },
  rb_player_sample_rate: { args: ["ptr"], ret: "u32" },
  rb_player_status_json: { args: ["ptr"], ret: "ptr" },
};

/**
 * The low-level backend a runtime loader must provide. High-level classes in
 * api.ts are written once against this interface.
 */
export interface Raw {
  /** The dlopen'd symbol table; call e.g. `sym.rb_dsp_new(44100)`. */
  sym: Record<string, (...a: any[]) => any>;
  /** Marshal a JS string as a `const char *` argument. */
  cstr(s: string): unknown;
  /** Marshal an Int16Array as a `const int16_t *` argument. */
  i16in(a: Int16Array): unknown;
  /** Allocate a `size_t` out-parameter; `value()` reads it after the call. */
  sizeOut(): { arg: unknown; value(): number };
  /** Read a returned `char *` into a string and free it (null => null). */
  takeString(p: unknown): string | null;
  /** Read a returned `int16_t *` of `len` samples into a copy and free it. */
  takeI16(p: unknown, len: number): Int16Array;
  /** True if a returned handle/pointer is NULL. */
  isNull(p: unknown): boolean;
}

const LIB_NAMES = ["librockbox_ffi.dylib", "librockbox_ffi.so", "rockbox_ffi.dll"];

/**
 * Locate the shared library: `ROCKBOX_FFI_LIB` env var first, then by walking
 * up from `startDir` to a `target/release` directory. `exists` and the path
 * ops are injected so this works under both Bun and Deno.
 */
export function resolveLibPath(
  startDir: string,
  env: (k: string) => string | undefined,
  exists: (p: string) => boolean,
  join: (...p: string[]) => string,
  dirname: (p: string) => string,
): string {
  const override = env("ROCKBOX_FFI_LIB");
  if (override) return override;

  let dir = startDir;
  const tried: string[] = [];
  for (let i = 0; i < 40; i++) {
    const rel = join(dir, "target", "release");
    for (const name of LIB_NAMES) {
      const p = join(rel, name);
      tried.push(p);
      if (exists(p)) return p;
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  throw new Error(
    "could not locate librockbox_ffi. Set ROCKBOX_FFI_LIB or run " +
      "`cargo build --release -p rockbox-ffi`. Tried:\n  " + tried.join("\n  "),
  );
}
