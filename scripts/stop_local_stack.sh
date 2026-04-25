#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_DIR="${VISIONCLIP_RUNTIME_DIR:-$ROOT_DIR/tools/runtime/local-stack}"
PID_DIR="${VISIONCLIP_PID_DIR:-$RUNTIME_DIR/pids}"

usage() {
    cat <<'EOF'
Uso:
  scripts/stop_local_stack.sh

Derruba os processos iniciados por scripts/start_local_stack.sh usando os arquivos PID.
EOF
}

if [[ "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

stop_from_pid_file() {
    local pid_file="$1"
    local label="$2"

    if [[ ! -f "$pid_file" ]]; then
        echo "$label: nenhum pid file em $pid_file"
        return
    fi

    local pid
    pid="$(cat "$pid_file")"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
        kill "$pid"
        echo "$label encerrado (pid $pid)"
    else
        echo "$label: processo nao estava ativo"
    fi

    rm -f "$pid_file"
}

stop_from_pid_file "$PID_DIR/piper-http.pid" "Piper HTTP"
stop_from_pid_file "$PID_DIR/visionclip-daemon.pid" "visionclip-daemon"
