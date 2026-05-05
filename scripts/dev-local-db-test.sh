#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

source "${SCRIPT_DIR}/dev-local-env.sh"

export TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"

exec cargo test -p corroded-cms --test db_integration
