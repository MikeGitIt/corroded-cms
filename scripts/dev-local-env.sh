#!/usr/bin/env bash
set -euo pipefail

DB_USER="${CORRODED_CMS_DB_USER:-${USER:-$(id -un)}}"
DEFAULT_DATABASE_URL="postgres://${DB_USER}@127.0.0.1:5432/corroded_cms"

export ENV_FILE="${ENV_FILE:-/private/tmp/corroded-cms-no-env}"
export DATABASE_URL="${CORRODED_CMS_DATABASE_URL:-$DEFAULT_DATABASE_URL}"
export BASE_URL="${CORRODED_CMS_BASE_URL:-http://127.0.0.1:3000}"
export SESSION_SECRET="${CORRODED_CMS_SESSION_SECRET:-dev-only-change-me-to-at-least-32-bytes}"
export UPLOAD_DIR="${CORRODED_CMS_UPLOAD_DIR:-uploads}"
export ENVIRONMENT="${CORRODED_CMS_ENVIRONMENT:-development}"
export SITE_NAME="${CORRODED_CMS_SITE_NAME:-GigaTier Technologies}"
export SITE_DESCRIPTION="${CORRODED_CMS_SITE_DESCRIPTION:-Autonomous C/C++ to Rust transpilation. Verified, validated, delivered.}"
export THEME="${CORRODED_CMS_THEME:-gigatier}"
export RUST_LOG="${CORRODED_CMS_RUST_LOG:-corroded_cms=info,tower_http=info}"
export PORT="${CORRODED_CMS_PORT:-3000}"
