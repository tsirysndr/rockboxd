// Run: npx tsx examples/smoke.node.ts   (or: node --import tsx examples/smoke.node.ts)
import * as api from "../src/node.ts";
import { runSmoke } from "./smoke.shared.ts";

runSmoke(api, "node");
