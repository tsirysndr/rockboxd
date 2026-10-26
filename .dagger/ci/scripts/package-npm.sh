#!/usr/bin/env bash
# Ported from bindings-release.yml / npm. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Stamp npm versions from release tag
(
cd "bindings/typescript"
set -euxo pipefail
# Bump the main package, every per-platform package, and the main
# package's optionalDependencies (which must pin the exact per-platform
# version) all to the tag — otherwise `npm install rockbox-ffi` pulls a
# stale prebuilt-binary package.
node -e '
  const fs = require("fs");
  const v = process.env.BINDING_VERSION;
  const main = "package.json";
  const m = JSON.parse(fs.readFileSync(main, "utf8"));
  m.version = v;
  for (const k of Object.keys(m.optionalDependencies || {})) {
    m.optionalDependencies[k] = v;
  }
  fs.writeFileSync(main, JSON.stringify(m, null, 2) + "\n");
  for (const d of fs.readdirSync("npm", { withFileTypes: true })) {
    if (!d.isDirectory()) continue;
    const p = `npm/${d.name}/package.json`;
    if (!fs.existsSync(p)) continue;
    const j = JSON.parse(fs.readFileSync(p, "utf8"));
    j.version = v;
    fs.writeFileSync(p, JSON.stringify(j, null, 2) + "\n");
  }
  console.log("stamped npm packages to", v);
'

)
load_step_env

# Build dist, stage binaries, pack tarballs
(
cd "bindings/typescript"
set -euxo pipefail
bun install
bun run build
declare -A LIB=(
  [darwin-arm64]=librockbox_ffi.dylib
  [darwin-x64]=librockbox_ffi.dylib
  [linux-x64]=librockbox_ffi.so
  [linux-arm64]=librockbox_ffi.so
  [freebsd-x64]=librockbox_ffi.so
  [netbsd-x64]=librockbox_ffi.so
)
dest="$GITHUB_WORKSPACE/bindings/typescript/tarballs"
mkdir -p "$dest"
for target in darwin-arm64 darwin-x64 linux-x64 linux-arm64 freebsd-x64 netbsd-x64; do
  src="$GITHUB_WORKSPACE/artifacts/librockbox_ffi-${target}/${LIB[$target]}"
  if [ ! -f "$src" ]; then echo "skip ${target}: no artifact"; continue; fi
  cp "$src" "npm/${target}/${LIB[$target]}"
  (cd "npm/${target}" && npm pack --pack-destination "$dest")
done
npm pack --pack-destination "$dest"
ls -l "$dest"

)
load_step_env

collect 'bindings/typescript/tarballs/*.tgz'
