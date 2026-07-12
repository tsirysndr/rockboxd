// Runtime auto-detecting entry point.
//
// Prefer importing the runtime-specific module directly (`rockbox-ffi/bun` or
// `./src/deno.ts`) — those load synchronously. This helper exists for code
// that wants a single import and can await initialization.

import type { makeApi } from "./api.ts";

export * from "./enums.ts";
export { sineStereo } from "./api.ts";
export type { M3uEntry, Metadata, PlayerConfig, PlayerStatus, RepeatName, ResumeState } from "./types.ts";

type Api = ReturnType<typeof makeApi>;

let cached: Api | undefined;

/**
 * Load the binding for the current runtime (Bun or Deno). Uses a dynamic
 * import so the other runtime's FFI module is never parsed.
 */
export async function load(): Promise<Api> {
  if (cached) return cached;
  // deno-lint-ignore no-explicit-any
  const g = globalThis as any;
  let mod: {
    abiVersion: Api["abiVersion"];
    metadata: Api["metadata"];
    Dsp: Api["Dsp"];
    Player: Api["Player"];
    loadResume: Api["loadResume"];
    m3uRead: Api["m3uRead"];
    m3uWrite: Api["m3uWrite"];
    isUrl: Api["isUrl"];
  };
  if (typeof g.Bun !== "undefined") {
    mod = await import("./bun.ts");
  } else if (typeof g.Deno !== "undefined") {
    mod = await import("./deno.ts");
  } else if (typeof g.process !== "undefined" && g.process.versions?.node) {
    mod = await import("./node.ts");
  } else {
    throw new Error("unsupported runtime: need Bun, Deno, or Node.js");
  }
  cached = {
    abiVersion: mod.abiVersion,
    metadata: mod.metadata,
    Dsp: mod.Dsp,
    Player: mod.Player,
    loadResume: mod.loadResume,
    m3uRead: mod.m3uRead,
    m3uWrite: mod.m3uWrite,
    isUrl: mod.isUrl,
  };
  return cached;
}
