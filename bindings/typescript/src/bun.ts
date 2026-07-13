// Bun backend + full API. Import from here under Bun:
//   import { Dsp, Player, metadata } from "rockbox-ffi/bun";

import {
  CString,
  dlopen,
  FFIType,
  type Pointer,
  ptr,
  toArrayBuffer,
} from "bun:ffi";
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { makeApi, sineStereo } from "./api.ts";
import {
  platformLibName,
  platformPackageName,
  resolveLibPath,
  SPEC,
  type Raw,
  type Tok,
} from "./ffi.ts";

/** Resolve the prebuilt binary from the per-platform npm package, if present. */
function bundledLibPath(): string | undefined {
  try {
    const require = createRequire(import.meta.url);
    const pkg = platformPackageName(process.platform, process.arch);
    return require.resolve(`${pkg}/${platformLibName(process.platform)}`);
  } catch {
    return undefined;
  }
}

const TOK: Record<Tok, FFIType> = {
  void: FFIType.void,
  ptr: FFIType.ptr,
  u32: FFIType.u32,
  i32: FFIType.i32,
  u64: FFIType.u64,
  usize: FFIType.u64,
  f32: FFIType.f32,
  bool: FFIType.bool,
  strbuf: FFIType.ptr,
  i16in: FFIType.ptr,
  outsize: FFIType.ptr,
  outu32: FFIType.ptr,
  outi32: FFIType.ptr,
};

function makeRaw(): Raw {
  const here = dirname(fileURLToPath(import.meta.url));
  const libPath = resolveLibPath(
    here,
    (k) => Bun.env[k],
    existsSync,
    join,
    dirname,
    bundledLibPath(),
  );

  const symbols: Record<string, { args: FFIType[]; returns: FFIType }> = {};
  for (const [name, spec] of Object.entries(SPEC)) {
    symbols[name] = { args: spec.args.map((t) => TOK[t]), returns: TOK[spec.ret] };
  }
  const lib = dlopen(libPath, symbols);
  const sym = lib.symbols as Record<string, (...a: any[]) => any>;

  // Keep marshalled input buffers alive across the (synchronous) call by
  // holding a reference on the caller's stack — Bun's `ptr()` does not copy.
  const enc = new TextEncoder();

  return {
    sym,
    cstr(s: string) {
      const buf = enc.encode(s + "\0");
      // Attach to a WeakRef-free holder: return the pointer but stash the
      // buffer on the returned Number via closure is impossible, so we rely
      // on synchronous FFI — the buffer is created fresh here and the call
      // happens immediately after in the same tick.
      return ptr(buf);
    },
    i16in(a: Int16Array) {
      return ptr(a);
    },
    sizeOut() {
      const arr = new BigUint64Array(1);
      return { arg: ptr(arr), value: () => Number(arr[0]) };
    },
    u32Out() {
      const arr = new Uint32Array(1);
      return { arg: ptr(arr), value: () => arr[0] };
    },
    i32Out() {
      const arr = new Int32Array(1);
      return { arg: ptr(arr), value: () => arr[0] };
    },
    takeString(p: unknown) {
      if (p === null || p === 0) return null;
      // Bun pointers are numbers at runtime; the typed API wants `Pointer`.
      const str = new CString(p as Pointer).toString();
      sym.rb_string_free(p);
      return str;
    },
    takeI16(p: unknown, len: number) {
      const ab = toArrayBuffer(p as Pointer, 0, len * 2);
      const copy = new Int16Array(len);
      copy.set(new Int16Array(ab));
      sym.rb_buffer_free(p, len);
      return copy;
    },
    isNull(p: unknown) {
      return p === null || p === 0 || p === undefined;
    },
  };
}

const api = makeApi(makeRaw());
export const { abiVersion, metadata, Dsp, Decoder, Player, loadResume, m3uRead, m3uWrite, isUrl } = api;
export { sineStereo };
export * from "./enums.ts";
export type { M3uEntry, Metadata, PlayerConfig, PlayerStatus, ResumeState } from "./types.ts";
