#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_DIR="${VISIONCLIP_RUNTIME_DIR:-$ROOT_DIR/tools/runtime/local-stack}"
LOG_DIR="${VISIONCLIP_LOG_DIR:-$RUNTIME_DIR/logs}"
PID_DIR="${VISIONCLIP_PID_DIR:-$RUNTIME_DIR/pids}"
CONFIG_PATH="${VISIONCLIP_CONFIG:-$RUNTIME_DIR/visionclip-e2e.toml}"
PIPER_HOST="${VISIONCLIP_PIPER_HOST:-127.0.0.1}"
PIPER_PORT="${VISIONCLIP_PIPER_PORT:-5000}"
OLLAMA_URL="${VISIONCLIP_OLLAMA_URL:-http://127.0.0.1:11434}"
MODEL_NAME="${VISIONCLIP_MODEL:-gemma4:e2b}"
OCR_MODEL_NAME="${VISIONCLIP_OCR_MODEL:-glm-ocr:latest}"
PIPER_VOICE="${VISIONCLIP_PIPER_VOICE:-pt_BR-faber-medium}"
PIPER_VOICE_DIR="${VISIONCLIP_PIPER_DIR:-$ROOT_DIR/tools/piper-voices}"
PLAYER_COMMAND="${VISIONCLIP_PLAYER_COMMAND:-pw-play}"
CAPTURE_TIMEOUT_MS="${VISIONCLIP_CAPTURE_TIMEOUT_MS:-60000}"
VENV_PYTHON="${VISIONCLIP_VENV_PYTHON:-$ROOT_DIR/venv/bin/python}"
BUILD_IF_NEEDED="${VISIONCLIP_BUILD_IF_NEEDED:-1}"
WARM_MODEL="${VISIONCLIP_WARM_MODEL:-1}"

PIPER_PID_FILE="$PID_DIR/piper-http.pid"
DAEMON_PID_FILE="$PID_DIR/visionclip-daemon.pid"
PIPER_LOG="$LOG_DIR/piper-http.log"
DAEMON_LOG="$LOG_DIR/visionclip-daemon.log"

usage() {
    cat <<'EOF'
Uso:
  scripts/start_local_stack.sh

Variaveis uteis:
  VISIONCLIP_CONFIG
  VISIONCLIP_RUNTIME_DIR
  VISIONCLIP_PIPER_HOST
  VISIONCLIP_PIPER_PORT
  VISIONCLIP_MODEL
  VISIONCLIP_OCR_MODEL
  VISIONCLIP_PIPER_VOICE
  VISIONCLIP_PIPER_DIR
  VISIONCLIP_PLAYER_COMMAND
  VISIONCLIP_CAPTURE_TIMEOUT_MS
  VISIONCLIP_VENV_PYTHON
  VISIONCLIP_BUILD_IF_NEEDED=0
  VISIONCLIP_WARM_MODEL=0
EOF
}

if [[ "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

require_file() {
    local path="$1"
    local label="$2"
    if [[ ! -e "$path" ]]; then
        echo "Erro: $label nao encontrado em $path" >&2
        exit 1
    fi
}

require_command() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Erro: comando obrigatorio ausente: $cmd" >&2
        exit 1
    fi
}

ensure_pid_not_running() {
    local pid_file="$1"
    if [[ ! -f "$pid_file" ]]; then
        return
    fi

    local pid
    pid="$(cat "$pid_file")"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
        return
    fi

    rm -f "$pid_file"
}

start_bg() {
    local log_file="$1"
    local pid_file="$2"
    shift 2

    nohup "$@" >"$log_file" 2>&1 &
    local pid=$!
    echo "$pid" >"$pid_file"
}

wait_for_http() {
    local url="$1"
    local label="$2"
    local attempts="${3:-30}"

    for _ in $(seq 1 "$attempts"); do
        if curl -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done

    echo "Erro: $label nao respondeu em tempo util: $url" >&2
    return 1
}

ensure_build() {
    if [[ "$BUILD_IF_NEEDED" != "1" ]]; then
        return
    fi

    if [[ -x "$ROOT_DIR/target/debug/visionclip" && -x "$ROOT_DIR/target/debug/visionclip-daemon" ]]; then
        return
    fi

    echo "Binaries ausentes. Rodando cargo build --workspace..."
    (
        cd "$ROOT_DIR"
        cargo build --workspace
    )
}

ensure_piper_voice() {
    local model_path="$PIPER_VOICE_DIR/$PIPER_VOICE.onnx"
    local config_path="$model_path.json"

    if [[ -f "$model_path" && -f "$config_path" ]]; then
        return
    fi

    echo "Baixando voz Piper $PIPER_VOICE para $PIPER_VOICE_DIR..."
    "$VENV_PYTHON" -m piper.download_voices "$PIPER_VOICE" --download_dir "$PIPER_VOICE_DIR"
}

write_config() {
    mkdir -p "$(dirname "$CONFIG_PATH")"

    cat >"$CONFIG_PATH" <<EOF
[general]
default_action = "explain"
log_level = "info"

[capture]
backend = "auto"
prefer_portal = true
capture_timeout_ms = $CAPTURE_TIMEOUT_MS

[infer]
backend = "ollama"
base_url = "$OLLAMA_URL"
model = "$MODEL_NAME"
ocr_model = "$OCR_MODEL_NAME"
keep_alive = "15m"
temperature = 0.1
thinking_default = ""

[search]
enabled = true
base_url = "https://www.google.com/search"
request_timeout_ms = 10000
max_results = 3
open_browser = true

[audio]
enabled = true
backend = "piper_http"
base_url = "http://$PIPER_HOST:$PIPER_PORT"
default_voice = ""
speak_actions = ["TranslatePtBr", "Explain", "SearchWeb"]
player_command = "$PLAYER_COMMAND"

[ui]
overlay = "compact"
show_notification = true
EOF
}

warm_model_if_enabled() {
    if [[ "$WARM_MODEL" != "1" ]]; then
        return
    fi

    echo "Aquecendo modelo Ollama $MODEL_NAME..."
    curl -fsS "$OLLAMA_URL/api/chat" \
        -H 'Content-Type: application/json' \
        -d "{\"model\":\"$MODEL_NAME\",\"stream\":false,\"keep_alive\":\"15m\",\"messages\":[{\"role\":\"user\",\"content\":\"Reply only with OK.\"}]}" \
        >/dev/null
}

mkdir -p "$RUNTIME_DIR" "$LOG_DIR" "$PID_DIR" "$PIPER_VOICE_DIR"

require_command curl
require_file "$VENV_PYTHON" "Python do venv do Piper"
ensure_build

"$VENV_PYTHON" -c 'import flask, piper' >/dev/null
ensure_piper_voice
write_config

ensure_pid_not_running "$PIPER_PID_FILE"
ensure_pid_not_running "$DAEMON_PID_FILE"

if ! curl -fsS "http://$PIPER_HOST:$PIPER_PORT/voices" >/dev/null 2>&1; then
    echo "Subindo Piper HTTP em http://$PIPER_HOST:$PIPER_PORT ..."
    start_bg \
        "$PIPER_LOG" \
        "$PIPER_PID_FILE" \
        "$VENV_PYTHON" -m piper.http_server \
        -m "$PIPER_VOICE_DIR/$PIPER_VOICE.onnx" \
        --host "$PIPER_HOST" \
        --port "$PIPER_PORT"
    wait_for_http "http://$PIPER_HOST:$PIPER_PORT/voices" "Piper HTTP"
else
    echo "Piper HTTP ja esta respondendo em http://$PIPER_HOST:$PIPER_PORT"
fi

warm_model_if_enabled

if ! pgrep -af "$ROOT_DIR/target/debug/visionclip-daemon" >/dev/null 2>&1; then
    echo "Subindo visionclip-daemon..."
    start_bg \
        "$DAEMON_LOG" \
        "$DAEMON_PID_FILE" \
        env "VISIONCLIP_CONFIG=$CONFIG_PATH" "$ROOT_DIR/target/debug/visionclip-daemon"
    sleep 1
else
    echo "visionclip-daemon ja esta em execucao"
fi

echo
echo "Stack local pronta."
echo "Config: $CONFIG_PATH"
echo "Piper log: $PIPER_LOG"
echo "Daemon log: $DAEMON_LOG"
echo
echo "Exemplos:"
echo "  VISIONCLIP_CONFIG=$CONFIG_PATH $ROOT_DIR/target/debug/visionclip --action explain --speak"
echo "  VISIONCLIP_CONFIG=$CONFIG_PATH $ROOT_DIR/target/debug/visionclip --action translate_ptbr --speak"
echo "  VISIONCLIP_CONFIG=$CONFIG_PATH $ROOT_DIR/target/debug/visionclip --action search_web --speak"
