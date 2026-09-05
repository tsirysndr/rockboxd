#!/usr/bin/env node
"use strict";

const fs = require("fs");
const path = require("path");
const { spawn } = require("child_process");

const bin = path.join(__dirname, "..", "native", "rockboxd");
if (!fs.existsSync(bin)) {
  console.error(
    "@rockboxd/cli: rockboxd binary not found. Reinstall the package " +
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
  console.error(`@rockboxd/cli: failed to launch rockboxd: ${err.message}`);
  process.exit(1);
});
