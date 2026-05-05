#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BIN="${BIN:-target/debug/corroded-cms}"

source "${SCRIPT_DIR}/dev-local-env.sh"

exec "$BIN" migrate
