#!/usr/bin/env bash
set -euo pipefail

DATABASE_URL="${DATABASE_URL:?DATABASE_URL is required}"
UPLOAD_DIR="${UPLOAD_DIR:-uploads}"
DB_BACKUP="${DB_BACKUP:?DB_BACKUP is required}"
UPLOADS_BACKUP="${UPLOADS_BACKUP:-}"

pg_restore --clean --if-exists --no-owner --no-acl --dbname "$DATABASE_URL" "$DB_BACKUP"

if [[ -n "$UPLOADS_BACKUP" ]]; then
    mkdir -p "$UPLOAD_DIR"
    tar -xzf "$UPLOADS_BACKUP" -C "$UPLOAD_DIR"
fi

printf 'Restored database from %s\n' "$DB_BACKUP"
if [[ -n "$UPLOADS_BACKUP" ]]; then
    printf 'Restored uploads from %s into %s\n' "$UPLOADS_BACKUP" "$UPLOAD_DIR"
fi
