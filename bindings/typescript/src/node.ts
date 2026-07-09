// Node.js backend + full API, using koffi for FFI. Import under Node:
//   import { Dsp, Player, metadata } from "rockbox-ffi/node";
//
// Node has no built-in FFI, so this backend depends on `koffi`
// (https://koffi.dev) — install it with `npm install koffi`.

import { existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

import { makeApi, sineStereo } from "./api.ts";
import {
  platformLibName,
  platformPackageName,
  resolveLibPath,
  SPEC,
  type Raw,
  type Tok,
} from "./ffi.ts";

// koffi ships as CommonJS; load it via require to keep ESM interop simple.
const require = createRequire(import.meta.url);
// deno-lint-ignore no-explicit-any
const koffi: any = require("koffi");

/** Resolve the prebuilt binary from the per-platform npm package, if present. */
function bundledLibPath(): string | undefined {
  try {
    const pkg = platformPackageName(process.platform, process.arch);
    return require.resolve(`${pkg}/${platformLibName(process.platform)}`);
  } catch {
    return undefined;
  }
}

const TOK: Record<Tok, string> = {
  void: "void",
  ptr: "void *",
  u32: "uint32",
  i32: "int32",
  u64: "uint64",
  usize: "size_t",
  f32: "float",
  bool: "bool",
  strbuf: "str", // koffi marshals a JS string to a NUL-terminated char*
  i16in: "void *", // we pass a Buffer view of the Int16Array
  outsize: "void *", // we pass a Buffer.alloc(8) and read it back
};

function makeRaw(): Raw {
  const here = dirname(fileURLToPath(import.meta.url));
  const libPath = resolveLibPath(
    here,
    (k) => process.env[k],
    existsSync,
    join,
    dirname,
    bundledLibPath(),
  );

  const lib = koffi.load(libPath);
  const sym: Record<string, (...a: any[]) => any> = {};
  for (const [name, spec] of Object.entries(SPEC)) {
    sym[name] = lib.func(name, TOK[spec.ret], spec.args.map((t) => TOK[t]));
  }

  return {
    sym,
    cstr(s: string) {
      // The `strbuf` params are typed "str"; koffi marshals the JS string.
      return s;
    },
    i16in(a: Int16Array) {
      return Buffer.from(a.buffer, a.byteOffset, a.byteLength);
    },
    sizeOut() {
      const buf = Buffer.alloc(8);
      return { arg: buf, value: () => Number(buf.readBigUInt64LE(0)) };
    },
    takeString(p: unknown) {
      if (p === null || p === undefined) return null;
      // Read a NUL-terminated char* of unknown length. (koffi's "str" type
      // is unreliable for longer strings in 2.16 — this form is robust.)
      const str = koffi.decode(p, "char", -1) as string;
      sym.rb_string_free(p);
      return str;
    },
    takeI16(p: unknown, len: number) {
      const arr = koffi.decode(p, koffi.array("int16_t", len)) as number[];
      const copy = Int16Array.from(arr);
      sym.rb_buffer_free(p, len);
      return copy;
    },
    isNull(p: unknown) {
      return p === null || p === undefined;
    },
  };
}

const api = makeApi(makeRaw());
export const { abiVersion, metadata, Dsp, Player } = api;
export { sineStereo };
export * from "./enums.ts";
export type { Metadata, PlayerConfig, PlayerStatus } from "./types.ts";
