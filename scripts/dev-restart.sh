#!/usr/bin/env bash
set -euo pipefail

ENV_FILE="${ENV_FILE:-.env}"
PORT="${PORT:-3000}"
BIN="${BIN:-target/debug/corroded-cms}"

load_env_file() {
    local file="$1"
    [[ -f "$file" ]] || return 0

    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ -z "$line" || "$line" == \#* ]] && continue
        [[ "$line" == *=* ]] || continue

        local key="${line%%=*}"
        local value="${line#*=}"
        [[ "$key" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || continue
        export "${key}=${value}"
    done <"$file"

    PORT="${PORT:-3000}"
}

stop_existing_server() {
    local pids
    pids="$(lsof -tiTCP:"$PORT" -sTCP:LISTEN 2>/dev/null || true)"
    [[ -n "$pids" ]] || return 0

    for pid in $pids; do
        local command
        command="$(ps -p "$pid" -o comm= 2>/dev/null || true)"
        case "$command" in
            *corroded-cms*|*corroded-*)
                printf 'Stopping %s on port %s\n' "$command" "$PORT"
                kill "$pid"
                ;;
            *)
                printf 'Port %s is used by %s (pid %s); not stopping it.\n' "$PORT" "${command:-unknown}" "$pid" >&2
                exit 1
                ;;
        esac
    done
}

wait_for_port() {
    local attempts=20
    while (( attempts > 0 )); do
        if ! lsof -tiTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
        attempts=$((attempts - 1))
    done

    printf 'Timed out waiting for port %s to become available.\n' "$PORT" >&2
    exit 1
}

load_env_file "$ENV_FILE"
stop_existing_server
wait_for_port

printf 'Starting %s on port %s\n' "$BIN" "$PORT"
exec "$BIN" serve
