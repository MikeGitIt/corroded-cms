#!/usr/bin/env bash
set -euo pipefail

DATABASE_URL="${DATABASE_URL:?DATABASE_URL is required}"
UPLOAD_DIR="${UPLOAD_DIR:-uploads}"
BACKUP_DIR="${BACKUP_DIR:-backups}"
STAMP="${STAMP:-$(date -u +%Y%m%dT%H%M%SZ)}"

mkdir -p "$BACKUP_DIR"

DB_BACKUP="${BACKUP_DIR}/corroded-cms-db-${STAMP}.dump"
UPLOADS_BACKUP="${BACKUP_DIR}/corroded-cms-uploads-${STAMP}.tar.gz"

pg_dump --format=custom --no-owner --no-acl --file "$DB_BACKUP" "$DATABASE_URL"

if [[ -d "$UPLOAD_DIR" ]]; then
    tar -czf "$UPLOADS_BACKUP" -C "$UPLOAD_DIR" .
else
    mkdir -p "$UPLOAD_DIR"
    tar -czf "$UPLOADS_BACKUP" -T /dev/null
fi

printf 'Database backup: %s\n' "$DB_BACKUP"
printf 'Uploads backup: %s\n' "$UPLOADS_BACKUP"
