#!/usr/bin/env node
/**
 * Postinstall: download the rockbox CLI binaries from GitHub releases.
 *
 * Resolves the latest release of tsirysndr/rockboxd (or the tag pinned via
 * ROCKBOX_VERSION), picks the tarball matching the current platform, verifies
 * its sha256 against the published .sha256 asset, and extracts the `rockbox`
 * and `rockboxd` binaries into ./native.
 */

"use strict";

const crypto = require("crypto");
const fs = require("fs");
const https = require("https");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const REPO = "tsirysndr/rockboxd";
const NATIVE_DIR = path.join(__dirname, "native");
const BINARIES = ["rockbox", "rockboxd"];
const MAX_REDIRECTS = 10;

function platformSlug() {
  const platform = os.platform();
  const arch = os.arch();
  const table = {
    "darwin-arm64": "aarch64-darwin",
    "darwin-x64": "x86_64-darwin",
    "linux-arm64": "aarch64-linux",
    "linux-x64": "amd64-linux",
    "freebsd-x64": "x86_64-freebsd",
  };
  const slug = table[`${platform}-${arch}`];
  if (!slug) {
    console.error(
      `@rockboxd/cli: no prebuilt binaries for ${platform}-${arch}.\n` +
        `See https://github.com/${REPO} for building from source.`
    );
    process.exit(1);
  }
  return slug;
}

function requestHeaders() {
  const headers = {
    "User-Agent": "@rockboxd/cli npm installer",
    Accept: "application/octet-stream, application/json",
  };
  const token = process.env.GITHUB_TOKEN || process.env.GH_TOKEN;
  if (token) headers.Authorization = `Bearer ${token}`;
  return headers;
}

function fetch(url, redirects = 0) {
  return new Promise((resolve, reject) => {
    if (redirects > MAX_REDIRECTS) {
      reject(new Error(`too many redirects fetching ${url}`));
      return;
    }
    https
      .get(url, { headers: requestHeaders() }, (res) => {
        if (
          res.statusCode >= 300 &&
          res.statusCode < 400 &&
          res.headers.location
        ) {
          res.resume();
          resolve(fetch(new URL(res.headers.location, url).href, redirects + 1));
          return;
        }
        if (res.statusCode !== 200) {
          res.resume();
          reject(new Error(`GET ${url} failed with HTTP ${res.statusCode}`));
          return;
        }
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => resolve(Buffer.concat(chunks)));
        res.on("error", reject);
      })
      .on("error", reject);
  });
}

async function resolveVersion() {
  const pinned = process.env.ROCKBOX_VERSION;
  if (pinned) return pinned.replace(/^v/, "");
  const body = await fetch(
    `https://api.github.com/repos/${REPO}/releases/latest`
  );
  const release = JSON.parse(body.toString("utf8"));
  if (!release.tag_name) {
    throw new Error(`could not resolve latest release of ${REPO}`);
  }
  return release.tag_name;
}

async function verifyChecksum(tarball, checksumUrl, assetName) {
  let expected;
  try {
    const body = await fetch(checksumUrl);
    expected = body.toString("utf8").trim().split(/\s+/)[0].toLowerCase();
  } catch (err) {
    console.warn(
      `@rockboxd/cli: checksum file unavailable (${err.message}), skipping verification`
    );
    return;
  }
  const actual = crypto.createHash("sha256").update(tarball).digest("hex");
  if (actual !== expected) {
    throw new Error(
      `sha256 mismatch for ${assetName}: expected ${expected}, got ${actual}`
    );
  }
}

function extract(tarballPath) {
  fs.mkdirSync(NATIVE_DIR, { recursive: true });
  const result = spawnSync(
    "tar",
    ["-xzf", tarballPath, "-C", NATIVE_DIR, ...BINARIES],
    { stdio: ["ignore", "inherit", "inherit"] }
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`tar exited with status ${result.status}`);
  }
  for (const name of BINARIES) {
    const bin = path.join(NATIVE_DIR, name);
    if (!fs.existsSync(bin)) {
      throw new Error(`archive did not contain the ${name} binary`);
    }
    fs.chmodSync(bin, 0o755);
  }
}

async function main() {
  const slug = platformSlug();
  const version = await resolveVersion();
  const assetName = `rockbox_${version}_${slug}.tar.gz`;
  const base = `https://github.com/${REPO}/releases/download/${version}`;

  console.log(`@rockboxd/cli: downloading ${assetName} (${version})...`);
  const tarball = await fetch(`${base}/${assetName}`);
  await verifyChecksum(tarball, `${base}/${assetName}.sha256`, assetName);

  const tmp = path.join(
    os.tmpdir(),
    `rockboxd-cli-${process.pid}-${assetName}`
  );
  fs.writeFileSync(tmp, tarball);
  try {
    extract(tmp);
  } finally {
    fs.rmSync(tmp, { force: true });
  }
  console.log(
    `@rockboxd/cli: installed ${BINARIES.join(", ")} ${version} to ${NATIVE_DIR}`
  );
}

main().catch((err) => {
  console.error(`@rockboxd/cli: install failed: ${err.message}`);
  process.exit(1);
});
