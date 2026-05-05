#!/usr/bin/env bash
set -euo pipefail

ENV_FILE="${ENV_FILE:-.env}"
PORT="${PORT:-3000}"
BIN="${BIN:-target/debug/corroded-cms}"
DETACH="${DETACH:-0}"
PID_FILE="${PID_FILE:-}"
LOG_FILE="${LOG_FILE:-}"

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

configure_runtime_files() {
    PID_FILE="${PID_FILE:-/private/tmp/corroded-cms-${PORT}.pid}"
    LOG_FILE="${LOG_FILE:-/private/tmp/corroded-cms-${PORT}.log}"
}

stop_existing_server() {
    local listeners
    listeners="$(lsof -nP -iTCP:"$PORT" -sTCP:LISTEN -F pc 2>/dev/null || true)"
    [[ -n "$listeners" ]] || return 0

    local pid=""
    local command=""

    while IFS= read -r field || [[ -n "$field" ]]; do
        case "$field" in
            p*)
                if [[ -n "$pid" ]]; then
                    stop_listener "$pid" "$command"
                fi
                pid="${field#p}"
                command=""
                ;;
            c*)
                command="${field#c}"
                ;;
        esac
    done <<<"$listeners"

    if [[ -n "$pid" ]]; then
        stop_listener "$pid" "$command"
    fi
}

stop_listener() {
    local pid="$1"
    local command="$2"

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

wait_for_start() {
    local pid="$1"
    local attempts=40
    while (( attempts > 0 )); do
        if lsof -tiTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
            return 0
        fi
        if ! kill -0 "$pid" >/dev/null 2>&1; then
            printf 'Server exited before listening on port %s. Log: %s\n' "$PORT" "$LOG_FILE" >&2
            exit 1
        fi
        sleep 0.25
        attempts=$((attempts - 1))
    done

    printf 'Timed out waiting for server to listen on port %s. Log: %s\n' "$PORT" "$LOG_FILE" >&2
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

start_detached() {
    mkdir -p "$(dirname "$PID_FILE")" "$(dirname "$LOG_FILE")"
    : >"$LOG_FILE"

    printf 'Starting %s on port %s in background\n' "$BIN" "$PORT"
    nohup "$BIN" serve >>"$LOG_FILE" 2>&1 </dev/null &
    local pid="$!"
    disown "$pid" 2>/dev/null || true
    printf '%s\n' "$pid" >"$PID_FILE"

    wait_for_start "$pid"
    printf 'Server started with pid %s\n' "$pid"
    printf 'Log: %s\n' "$LOG_FILE"
    printf 'Pid: %s\n' "$PID_FILE"
}

load_env_file "$ENV_FILE"
configure_runtime_files
stop_existing_server
wait_for_port

if is_truthy "$DETACH"; then
    start_detached
else
    printf 'Starting %s on port %s\n' "$BIN" "$PORT"
    exec "$BIN" serve
fi
