#!/usr/bin/env bash
# Ported from bindings-erlang-release.yml / build. Publishing stays in Actions.
set -euo pipefail
source /ci/scripts/common.sh
export OTP_VERSION=27
export WORKDIR=bindings/erlang

# Install Linux deps
(
apt-get update
apt-get install -y libasound2-dev libunwind-dev libdbus-1-dev pkg-config

)
load_step_env

# Build NIF (matrix target)
(
cd "${WORKDIR}"
make TARGET=${TARGET}
)
load_step_env

collect 'bindings/erlang/priv/*.so'
