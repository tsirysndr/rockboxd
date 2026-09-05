#!/usr/bin/env node
/**
 * Single dispatcher for both bin entries. Both `rockbox` and `rockboxd`
 * point here: pointing every bin at one file is what lets npx run
 * `npx @rockboxd/cli` at all (it refuses packages with several distinct
 * bin targets). The invoked name picks the native binary; anything else
 * (including plain `npx @rockboxd/cli`) defaults to the rockboxd daemon.
 */

"use strict";

const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const invoked = path.basename(process.argv[1] || "", ".js");
const name = invoked === "rockbox" ? "rockbox" : "rockboxd";

const bin = path.join(__dirname, "..", "native", name);
if (!fs.existsSync(bin)) {
  console.error(
    `@rockboxd/cli: ${name} binary not found. Reinstall the package ` +
      "(the postinstall step downloads it), or run `node install.js` " +
      "inside the package directory."
  );
  process.exit(1);
}

const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });
child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code === null ? 1 : code);
});
child.on("error", (err) => {
  console.error(`@rockboxd/cli: failed to launch ${name}: ${err.message}`);
  process.exit(1);
});
