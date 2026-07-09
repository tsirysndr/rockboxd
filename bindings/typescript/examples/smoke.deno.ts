// Run: deno run --allow-ffi --allow-read --allow-env examples/smoke.deno.ts
import * as api from "../src/deno.ts";
import { runSmoke } from "./smoke.shared.ts";

runSmoke(api, "deno");
