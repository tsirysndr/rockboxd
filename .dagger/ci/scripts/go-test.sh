#!/usr/bin/env bash
# Ported from bindings-go-test.yml / test. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh

# Install Linux audio deps
(
apt-get update
apt-get install -y libasound2-dev pkg-config

)
load_step_env

# Build librockbox_ffi
(
cargo build --release -p rockbox-ffi
)
load_step_env

# go vet
(
cd "bindings/go"
go vet -unsafeptr=false ./...
)
load_step_env

# go test
(
cd "bindings/go"
go test -v ./...
)
load_step_env

collect
