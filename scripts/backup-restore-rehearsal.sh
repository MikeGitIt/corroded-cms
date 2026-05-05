#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

POSTGRES_IMAGE="${POSTGRES_IMAGE:-postgres:17-alpine}"
CONTAINER_NAME="${CONTAINER_NAME:-corroded-cms-backup-rehearsal}"
DB_USER="${DB_USER:-corroded}"
DB_PASSWORD="${DB_PASSWORD:-corroded}"
SOURCE_DB="${SOURCE_DB:-corroded_cms}"
RESTORE_DB="${RESTORE_DB:-corroded_cms_restore}"
MARKER_ID="${MARKER_ID:-mvp-release}"
MARKER_NOTE="${MARKER_NOTE:-backup restore rehearsal}"
KEEP_REHEARSAL_ARTIFACTS="${KEEP_REHEARSAL_ARTIFACTS:-0}"
WORK_DIR="${WORK_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/corroded-cms-rehearsal.XXXXXX")}"
STAMP="${STAMP:-rehearsal-$(date -u +%Y%m%dT%H%M%SZ)}"

SHIM_DIR="${WORK_DIR}/bin"
SOURCE_UPLOADS="${WORK_DIR}/source-uploads"
RESTORE_UPLOADS="${WORK_DIR}/restore-uploads"
BACKUP_DIR="${WORK_DIR}/backups"
DB_BACKUP="${BACKUP_DIR}/corroded-cms-db-${STAMP}.dump"
UPLOADS_BACKUP="${BACKUP_DIR}/corroded-cms-uploads-${STAMP}.tar.gz"

container_started=0

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

is_truthy() {
    case "${1:-}" in
        1|true|TRUE|yes|YES|on|ON)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

find_tool() {
    local base="$1"
    if command -v "$base" >/dev/null 2>&1; then
        command -v "$base"
        return 0
    fi
    if command -v "${base}-17" >/dev/null 2>&1; then
        command -v "${base}-17"
        return 0
    fi
    fail "could not find ${base} or ${base}-17"
}

cleanup() {
    if (( container_started == 1 )); then
        docker rm -f "$CONTAINER_NAME" >/dev/null 2>&1 || true
    fi
    if ! is_truthy "$KEEP_REHEARSAL_ARTIFACTS"; then
        rm -rf "$WORK_DIR"
    else
        printf 'Kept rehearsal artifacts: %s\n' "$WORK_DIR"
    fi
}
trap cleanup EXIT

PSQL_CMD="$(find_tool psql)"
PG_DUMP_CMD="$(find_tool pg_dump)"
PG_RESTORE_CMD="$(find_tool pg_restore)"

mkdir -p "$SHIM_DIR" "$SOURCE_UPLOADS" "$RESTORE_UPLOADS" "$BACKUP_DIR"
ln -sf "$PG_DUMP_CMD" "${SHIM_DIR}/pg_dump"
ln -sf "$PG_RESTORE_CMD" "${SHIM_DIR}/pg_restore"

if docker ps -a --format '{{.Names}}' | grep -Fxq "$CONTAINER_NAME"; then
    printf 'Removing existing rehearsal container: %s\n' "$CONTAINER_NAME"
    docker rm -f "$CONTAINER_NAME" >/dev/null
fi

printf 'Starting disposable PostgreSQL container: %s\n' "$CONTAINER_NAME"
docker run --rm -d \
    --name "$CONTAINER_NAME" \
    -e "POSTGRES_USER=${DB_USER}" \
    -e "POSTGRES_PASSWORD=${DB_PASSWORD}" \
    -e "POSTGRES_DB=${SOURCE_DB}" \
    -p 127.0.0.1::5432 \
    "$POSTGRES_IMAGE" >/dev/null
container_started=1

docker exec "$CONTAINER_NAME" sh -lc "until pg_isready -U '${DB_USER}' -d '${SOURCE_DB}' >/dev/null 2>&1; do sleep 1; done"

HOST_PORT="$(docker port "$CONTAINER_NAME" 5432/tcp | sed -n 's/^127\.0\.0\.1:\([0-9][0-9]*\)$/\1/p' | head -n 1)"
[[ -n "$HOST_PORT" ]] || fail "could not determine mapped PostgreSQL port"

SOURCE_URL="postgres://${DB_USER}:${DB_PASSWORD}@127.0.0.1:${HOST_PORT}/${SOURCE_DB}"
ADMIN_URL="postgres://${DB_USER}:${DB_PASSWORD}@127.0.0.1:${HOST_PORT}/postgres"
RESTORE_URL="postgres://${DB_USER}:${DB_PASSWORD}@127.0.0.1:${HOST_PORT}/${RESTORE_DB}"

printf 'Seeding source database marker\n'
"$PSQL_CMD" "$SOURCE_URL" -v ON_ERROR_STOP=1 \
    -c "CREATE TABLE backup_restore_rehearsal (id text PRIMARY KEY, note text NOT NULL, created_at timestamptz NOT NULL DEFAULT now())" \
    -c "INSERT INTO backup_restore_rehearsal (id, note) VALUES ('${MARKER_ID}', '${MARKER_NOTE}')"

printf 'rehearsal upload asset\n' >"${SOURCE_UPLOADS}/rehearsal.txt"

printf 'Running backup script\n'
PATH="${SHIM_DIR}:$PATH" \
    DATABASE_URL="$SOURCE_URL" \
    UPLOAD_DIR="$SOURCE_UPLOADS" \
    BACKUP_DIR="$BACKUP_DIR" \
    STAMP="$STAMP" \
    "${SCRIPT_DIR}/backup.sh"

[[ -s "$DB_BACKUP" ]] || fail "database backup was not created: $DB_BACKUP"
[[ -s "$UPLOADS_BACKUP" ]] || fail "uploads backup was not created: $UPLOADS_BACKUP"

printf 'Creating restore database\n'
"$PSQL_CMD" "$ADMIN_URL" -v ON_ERROR_STOP=1 -c "CREATE DATABASE ${RESTORE_DB}"

printf 'Running restore script\n'
PATH="${SHIM_DIR}:$PATH" \
    DATABASE_URL="$RESTORE_URL" \
    UPLOAD_DIR="$RESTORE_UPLOADS" \
    DB_BACKUP="$DB_BACKUP" \
    UPLOADS_BACKUP="$UPLOADS_BACKUP" \
    "${SCRIPT_DIR}/restore.sh"

RESTORED_NOTE="$("$PSQL_CMD" "$RESTORE_URL" -At -v ON_ERROR_STOP=1 -c "SELECT note FROM backup_restore_rehearsal WHERE id = '${MARKER_ID}'")"
[[ "$RESTORED_NOTE" == "$MARKER_NOTE" ]] || fail "restored marker mismatch: ${RESTORED_NOTE}"

RESTORED_UPLOAD="$(cat "${RESTORE_UPLOADS}/rehearsal.txt")"
[[ "$RESTORED_UPLOAD" == "rehearsal upload asset" ]] || fail "restored upload mismatch"

printf 'Backup/restore rehearsal passed.\n'
