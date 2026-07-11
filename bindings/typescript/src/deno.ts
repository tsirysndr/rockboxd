// Deno backend + full API. Import from here under Deno:
//   import { Dsp, Player, metadata } from "./src/deno.ts";

import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { makeApi, sineStereo } from "./api.ts";
import { resolveLibPath, SPEC, type Raw, type Tok } from "./ffi.ts";

// deno-lint-ignore no-explicit-any
const D = (globalThis as any).Deno;

const TOK: Record<Tok, string> = {
  void: "void",
  ptr: "pointer",
  u32: "u32",
  i32: "i32",
  u64: "u64",
  usize: "usize",
  f32: "f32",
  bool: "bool",
  strbuf: "buffer",
  i16in: "buffer",
  outsize: "buffer",
};

function makeRaw(): Raw {
  const here = dirname(fileURLToPath(import.meta.url));
  const libPath = resolveLibPath(
    here,
    (k) => D.env.get(k),
    existsSync,
    join,
    dirname,
  );

  const defs: Record<string, { parameters: string[]; result: string }> = {};
  for (const [name, spec] of Object.entries(SPEC)) {
    defs[name] = { parameters: spec.args.map((t) => TOK[t]), result: TOK[spec.ret] };
  }
  const lib = D.dlopen(libPath, defs);
  const sym = lib.symbols as Record<string, (...a: any[]) => any>;

  const enc = new TextEncoder();

  return {
    sym,
    cstr(s: string) {
      return enc.encode(s + "\0");
    },
    i16in(a: Int16Array) {
      return a;
    },
    sizeOut() {
      const buf = new Uint8Array(8);
      const view = new DataView(buf.buffer);
      return { arg: buf, value: () => Number(view.getBigUint64(0, true)) };
    },
    takeString(p: unknown) {
      if (p === null) return null;
      const str = new D.UnsafePointerView(p).getCString();
      sym.rb_string_free(p);
      return str;
    },
    takeI16(p: unknown, len: number) {
      const ab = new D.UnsafePointerView(p).getArrayBuffer(len * 2);
      const copy = new Int16Array(len);
      copy.set(new Int16Array(ab));
      sym.rb_buffer_free(p, len);
      return copy;
    },
    isNull(p: unknown) {
      return p === null || p === undefined;
    },
  };
}

const api = makeApi(makeRaw());
export const { abiVersion, metadata, Dsp, Player, loadResume, m3uRead, m3uWrite, isUrl } = api;
export { sineStereo };
export * from "./enums.ts";
export type { M3uEntry, Metadata, PlayerConfig, PlayerStatus, ResumeState } from "./types.ts";
