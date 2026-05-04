#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${VISIONCLIP_BIN_DIR:-$HOME/.local/bin}"
CONFIG_DIR="${VISIONCLIP_CONFIG_DIR:-$HOME/.config/visionclip}"
CONFIG_PATH="${VISIONCLIP_CONFIG:-$CONFIG_DIR/config.toml}"
DATA_DIR="${VISIONCLIP_DATA_DIR:-$HOME/.local/share/visionclip}"
STATE_DIR="${VISIONCLIP_STATE_DIR:-$HOME/.local/state/visionclip}"
LOG_DIR="$STATE_DIR/logs"
VENV_DIR="${VISIONCLIP_VENV_DIR:-$DATA_DIR/venv}"
PIPER_VOICE_DIR="${VISIONCLIP_PIPER_VOICE_DIR:-$DATA_DIR/piper-voices}"
STT_CACHE_DIR="${VISIONCLIP_STT_CACHE_DIR:-$DATA_DIR/stt-cache}"
HF_CACHE_DIR="${VISIONCLIP_HF_CACHE_DIR:-$DATA_DIR/huggingface}"
USER_SYSTEMD_DIR="$HOME/.config/systemd/user"

OLLAMA_MODEL="${VISIONCLIP_MODEL:-gemma4:e2b}"
HF_MODEL="${VISIONCLIP_HF_MODEL:-google/gemma-4-E2B-it}"
STT_MODEL="${VISIONCLIP_STT_MODEL:-base}"
PIPER_DEFAULT_VOICE="${VISIONCLIP_PIPER_DEFAULT_VOICE:-pt_BR-faber-medium}"
PIPER_VOICES="${VISIONCLIP_PIPER_VOICES:-pt_BR-faber-medium en_US-lessac-medium es_ES-sharvard-medium zh_CN-huayan-medium ru_RU-ruslan-medium hi_IN-pratham-medium}"
VOICE_SHORTCUT="${VISIONCLIP_VOICE_SHORTCUT:-<Super>F12}"

YES=0
SKIP_SYSTEM_PACKAGES=0
SKIP_OLLAMA_INSTALL=0
SKIP_OLLAMA_PULL=0
SKIP_HF_DOWNLOAD=0
SKIP_SHORTCUT=0
SKIP_START=0
OVERWRITE_CONFIG=0

usage() {
    cat <<'EOF'
Usage:
  scripts/install_visionclip.sh [options]

Options:
  -y, --yes                 Run non-interactively and accept safe defaults.
  --skip-system-packages    Do not install OS packages with sudo.
  --skip-ollama-install     Do not install Ollama if it is missing.
  --skip-ollama-pull        Do not pull the Ollama runtime model.
  --skip-hf-download        Do not download the official Hugging Face model cache.
  --no-shortcut             Do not install the GNOME voice shortcut.
  --no-start                Install files but do not start systemd user services.
  --overwrite-config        Replace ~/.config/visionclip/config.toml after backing it up.
  --model NAME              Ollama model used by VisionClip. Default: gemma4:e2b.
  --hf-model REPO           Hugging Face model to cache. Default: google/gemma-4-E2B-it.
  --stt-model NAME          faster-whisper model. Default: base.
  --voice-shortcut BINDING  GNOME shortcut binding. Default: <Super>F12.
  -h, --help                Show this help.

Environment:
  HF_TOKEN                  Hugging Face token. If absent, the script can ask for it.
  VISIONCLIP_PIPER_VOICES  Space-separated Piper voices to download.
EOF
}

on_error() {
    local exit_code=$?
    local line="$1"
    local command="$2"
    echo >&2
    echo "VisionClip install failed at line $line." >&2
    echo "Command: $command" >&2
    echo "Exit code: $exit_code" >&2
    echo "Logs, when available, are under: $LOG_DIR" >&2
    exit "$exit_code"
}
trap 'on_error "$LINENO" "$BASH_COMMAND"' ERR

info() {
    printf '[visionclip] %s\n' "$*"
}

warn() {
    printf '[visionclip][warn] %s\n' "$*" >&2
}

die() {
    printf '[visionclip][error] %s\n' "$*" >&2
    exit 1
}

is_interactive() {
    [[ -t 0 && -t 1 ]]
}

confirm() {
    local prompt="$1"
    local default="${2:-y}"
    local suffix="[Y/n]"
    if [[ "$default" == "n" ]]; then
        suffix="[y/N]"
    fi

    if [[ "$YES" == "1" ]]; then
        [[ "$default" == "y" ]]
        return
    fi

    if ! is_interactive; then
        return 1
    fi

    local answer
    read -r -p "$prompt $suffix " answer
    answer="${answer,,}"
    if [[ -z "$answer" ]]; then
        [[ "$default" == "y" ]]
        return
    fi
    [[ "$answer" == "y" || "$answer" == "yes" || "$answer" == "s" || "$answer" == "sim" ]]
}

prompt_secret() {
    local prompt="$1"
    local value=""
    if is_interactive; then
        read -r -s -p "$prompt" value
        echo >&2
    fi
    printf '%s' "$value"
}

run() {
    info "running: $*"
    "$@"
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -y|--yes)
                YES=1
                ;;
            --skip-system-packages)
                SKIP_SYSTEM_PACKAGES=1
                ;;
            --skip-ollama-install)
                SKIP_OLLAMA_INSTALL=1
                ;;
            --skip-ollama-pull)
                SKIP_OLLAMA_PULL=1
                ;;
            --skip-hf-download)
                SKIP_HF_DOWNLOAD=1
                ;;
            --no-shortcut)
                SKIP_SHORTCUT=1
                ;;
            --no-start)
                SKIP_START=1
                ;;
            --overwrite-config)
                OVERWRITE_CONFIG=1
                ;;
            --model)
                shift
                [[ $# -gt 0 ]] || die "--model requires a value"
                OLLAMA_MODEL="$1"
                ;;
            --hf-model)
                shift
                [[ $# -gt 0 ]] || die "--hf-model requires a value"
                HF_MODEL="$1"
                ;;
            --stt-model)
                shift
                [[ $# -gt 0 ]] || die "--stt-model requires a value"
                STT_MODEL="$1"
                ;;
            --voice-shortcut)
                shift
                [[ $# -gt 0 ]] || die "--voice-shortcut requires a value"
                VOICE_SHORTCUT="$1"
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                die "unknown option: $1"
                ;;
        esac
        shift
    done
}

detect_package_manager() {
    if command -v apt-get >/dev/null 2>&1; then
        echo apt
    elif command -v dnf >/dev/null 2>&1; then
        echo dnf
    elif command -v pacman >/dev/null 2>&1; then
        echo pacman
    elif command -v zypper >/dev/null 2>&1; then
        echo zypper
    else
        echo unknown
    fi
}

install_optional_package() {
    local manager="$1"
    local package="$2"
    case "$manager" in
        apt)
            sudo apt-get install -y "$package" || warn "optional package failed: $package"
            ;;
        dnf)
            sudo dnf install -y "$package" || warn "optional package failed: $package"
            ;;
        pacman)
            sudo pacman -S --needed --noconfirm "$package" || warn "optional package failed: $package"
            ;;
        zypper)
            sudo zypper install -y "$package" || warn "optional package failed: $package"
            ;;
    esac
}

install_system_packages() {
    if [[ "$SKIP_SYSTEM_PACKAGES" == "1" ]]; then
        warn "skipping system package installation"
        return
    fi

    local manager
    manager="$(detect_package_manager)"
    if [[ "$manager" == "unknown" ]]; then
        warn "no supported package manager found; install Rust build tools, Python venv, GTK4 dev files, PipeWire tools, xdg-utils, libnotify, poppler-utils, espeak-ng and libsndfile manually"
        return
    fi

    if ! confirm "Install VisionClip OS dependencies with sudo using $manager?" "y"; then
        warn "system packages were not installed"
        return
    fi

    case "$manager" in
        apt)
            run sudo apt-get update
            run sudo apt-get install -y \
                build-essential pkg-config curl ca-certificates git \
                python3 python3-venv python3-pip \
                libgtk-4-dev libglib2.0-bin xdg-utils libnotify-bin \
                espeak-ng libsndfile1
            for package in pipewire pipewire-bin wireplumber xdg-desktop-portal xdg-desktop-portal-gnome poppler-utils ffmpeg grim maim gnome-screenshot wl-clipboard; do
                install_optional_package "$manager" "$package"
            done
            ;;
        dnf)
            run sudo dnf install -y \
                gcc gcc-c++ make pkgconf-pkg-config curl ca-certificates git \
                python3 python3-pip python3-virtualenv \
                gtk4-devel glib2 xdg-utils libnotify \
                espeak-ng libsndfile
            for package in pipewire-utils wireplumber xdg-desktop-portal xdg-desktop-portal-gnome poppler-utils ffmpeg grim maim gnome-screenshot wl-clipboard; do
                install_optional_package "$manager" "$package"
            done
            ;;
        pacman)
            run sudo pacman -Syu --needed --noconfirm \
                base-devel pkgconf curl ca-certificates git \
                python python-pip python-virtualenv \
                gtk4 glib2 xdg-utils libnotify \
                espeak-ng libsndfile
            for package in pipewire wireplumber xdg-desktop-portal xdg-desktop-portal-gnome poppler ffmpeg grim maim wl-clipboard; do
                install_optional_package "$manager" "$package"
            done
            ;;
        zypper)
            run sudo zypper install -y \
                gcc gcc-c++ make pkg-config curl ca-certificates git \
                python3 python3-pip python3-virtualenv \
                gtk4-devel glib2-tools xdg-utils libnotify-tools \
                espeak-ng libsndfile1
            for package in pipewire wireplumber xdg-desktop-portal poppler-tools ffmpeg grim maim wl-clipboard; do
                install_optional_package "$manager" "$package"
            done
            ;;
    esac
}

ensure_rust() {
    if command -v cargo >/dev/null 2>&1; then
        return
    fi

    if ! confirm "Rust/cargo was not found. Install Rust with rustup?" "y"; then
        die "cargo is required to build VisionClip"
    fi

    local installer
    installer="$(mktemp)"
    run curl --proto '=https' --tlsv1.2 -fsSL -o "$installer" https://sh.rustup.rs
    run sh "$installer" -y
    rm -f "$installer"
    # shellcheck disable=SC1091
    [[ -f "$HOME/.cargo/env" ]] && source "$HOME/.cargo/env"
    command -v cargo >/dev/null 2>&1 || die "cargo is still unavailable after rustup install"
}

ensure_ollama() {
    if command -v ollama >/dev/null 2>&1; then
        return
    fi

    if [[ "$SKIP_OLLAMA_INSTALL" == "1" ]]; then
        die "ollama is required but missing; install it or remove --skip-ollama-install"
    fi

    if ! confirm "Ollama was not found. Install Ollama using the official installer?" "y"; then
        die "ollama is required for the local provider"
    fi

    local installer
    installer="$(mktemp)"
    run curl -fsSL -o "$installer" https://ollama.com/install.sh
    run sh "$installer"
    rm -f "$installer"
    command -v ollama >/dev/null 2>&1 || die "ollama is still unavailable after install"
}

wait_for_ollama() {
    for _ in $(seq 1 30); do
        if curl -fsS http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    return 1
}

ensure_ollama_running() {
    mkdir -p "$LOG_DIR"
    if wait_for_ollama; then
        return
    fi

    if command -v systemctl >/dev/null 2>&1 && systemctl list-unit-files ollama.service >/dev/null 2>&1; then
        info "starting system Ollama service"
        sudo systemctl enable --now ollama.service || true
        if wait_for_ollama; then
            return
        fi
    fi

    info "starting ollama serve in user background"
    nohup ollama serve >"$LOG_DIR/ollama.log" 2>&1 < /dev/null &
    echo "$!" >"$STATE_DIR/ollama.pid"
    if ! wait_for_ollama; then
        tail -n 80 "$LOG_DIR/ollama.log" >&2 || true
        die "Ollama did not become ready at http://127.0.0.1:11434"
    fi
}

pull_ollama_model() {
    if [[ "$SKIP_OLLAMA_PULL" == "1" ]]; then
        warn "skipping ollama pull for $OLLAMA_MODEL"
        return
    fi

    if ollama show "$OLLAMA_MODEL" >/dev/null 2>&1; then
        info "Ollama model already available: $OLLAMA_MODEL"
        return
    fi

    run ollama pull "$OLLAMA_MODEL"
}

ensure_python_runtime() {
    command -v python3 >/dev/null 2>&1 || die "python3 is required"
    mkdir -p "$DATA_DIR"
    if [[ ! -x "$VENV_DIR/bin/python" ]]; then
        run python3 -m venv "$VENV_DIR"
    fi
    run "$VENV_DIR/bin/python" -m pip install --upgrade pip wheel
    run "$VENV_DIR/bin/python" -m pip install --upgrade \
        Flask piper-tts faster-whisper huggingface_hub
}

download_piper_voice() {
    local voice="$1"
    [[ -n "$voice" ]] || return

    local model_path="$PIPER_VOICE_DIR/$voice.onnx"
    local config_path="$model_path.json"
    if [[ -f "$model_path" && -f "$config_path" ]]; then
        info "Piper voice already available: $voice"
        return
    fi

    info "downloading Piper voice: $voice"
    if ! "$VENV_DIR/bin/python" -m piper.download_voices "$voice" --download_dir "$PIPER_VOICE_DIR"; then
        if [[ "$voice" == "$PIPER_DEFAULT_VOICE" ]]; then
            die "failed to download required Piper voice: $voice"
        fi
        warn "failed to download optional Piper voice: $voice"
    fi
}

download_piper_voices() {
    mkdir -p "$PIPER_VOICE_DIR"
    local voice
    for voice in $PIPER_VOICES; do
        download_piper_voice "$voice"
    done
}

download_hf_model_if_requested() {
    if [[ "$SKIP_HF_DOWNLOAD" == "1" ]]; then
        warn "skipping Hugging Face model cache download"
        return
    fi

    local should_download=0
    if [[ "$YES" == "1" ]]; then
        should_download=1
    elif confirm "Download the official Hugging Face cache for $HF_MODEL? This is large and separate from Ollama." "y"; then
        should_download=1
    fi
    [[ "$should_download" == "1" ]] || return

    local token="${HF_TOKEN:-}"
    if [[ -z "$token" ]]; then
        token="$(prompt_secret "Hugging Face token, input hidden. Leave empty to skip: ")"
    fi
    if [[ -z "$token" ]]; then
        warn "no Hugging Face token provided; skipping $HF_MODEL cache download"
        return
    fi

    mkdir -p "$HF_CACHE_DIR"
    info "downloading Hugging Face model cache: $HF_MODEL"
    if ! HF_MODEL="$HF_MODEL" HF_CACHE_DIR="$HF_CACHE_DIR" HF_HOME="$HF_CACHE_DIR" HF_TOKEN="$token" "$VENV_DIR/bin/python" <<'PY'; then
import os
import sys
from huggingface_hub import snapshot_download

try:
    snapshot_download(
        repo_id=os.environ["HF_MODEL"],
        cache_dir=os.environ["HF_CACHE_DIR"],
        token=os.environ["HF_TOKEN"],
        resume_download=True,
    )
except Exception as error:
    print(error, file=sys.stderr)
    raise SystemExit(1)
PY
        warn "Hugging Face download failed. Check token permissions and accept the model terms on https://huggingface.co/$HF_MODEL if required."
    fi
}

detect_player_command() {
    for command in pw-play paplay aplay; do
        if command -v "$command" >/dev/null 2>&1; then
            echo "$command"
            return
        fi
    done
    echo "pw-play"
}

toml_escape() {
    local value="$1"
    value="${value//\\/\\\\}"
    value="${value//\"/\\\"}"
    printf '%s' "$value"
}

write_config() {
    mkdir -p "$CONFIG_DIR"

    if [[ -f "$CONFIG_PATH" && "$OVERWRITE_CONFIG" != "1" ]]; then
        if ! confirm "Replace existing config at $CONFIG_PATH after creating a backup?" "y"; then
            warn "keeping existing config: $CONFIG_PATH"
            return
        fi
    fi

    if [[ -f "$CONFIG_PATH" ]]; then
        cp "$CONFIG_PATH" "$CONFIG_PATH.bak.$(date +%Y%m%d%H%M%S)"
    fi

    local escaped_model escaped_transcribe escaped_player
    escaped_model="$(toml_escape "$OLLAMA_MODEL")"
    escaped_player="$(toml_escape "$(detect_player_command)")"
    escaped_transcribe="$(toml_escape "$VENV_DIR/bin/python $ROOT_DIR/tools/stt/faster_whisper_transcribe.py {wav_path} --model $STT_MODEL --language auto --beam-size 5 --vad-filter true --cache-dir $STT_CACHE_DIR")"

    cat >"$CONFIG_PATH" <<EOF
[general]
default_action = "translate_ptbr"
log_level = "info"

[capture]
backend = "auto"
prefer_portal = true
capture_timeout_ms = 60000

[infer]
backend = "ollama"
base_url = "http://127.0.0.1:11434"
model = "$escaped_model"
ocr_model = "$escaped_model"
embedding_model = ""
keep_alive = "15m"
temperature = 0.1
thinking_default = ""
context_window_tokens = 8192

[providers]
route_mode = "local_first"
sensitive_data_mode = "local_only"
ollama_enabled = true
cloud_enabled = false

[search]
enabled = true
base_url = "https://www.google.com/search"
fallback_enabled = true
fallback_base_url = "https://html.duckduckgo.com/html/"
request_timeout_ms = 10000
max_results = 3
open_browser = true
rendered_ai_overview_listener = true
rendered_ai_overview_wait_ms = 12000
rendered_ai_overview_poll_interval_ms = 3000

[audio]
enabled = true
backend = "piper_http"
base_url = "http://127.0.0.1:5000"
default_voice = "$PIPER_DEFAULT_VOICE"
speak_actions = ["TranslatePtBr", "Explain", "SearchWeb", "OpenApplication", "OpenUrl", "OpenDocument"]
player_command = "$escaped_player"
request_timeout_ms = 60000
playback_timeout_ms = 120000

[audio.voices]
"pt-BR" = "pt_BR-faber-medium"
en = "en_US-lessac-medium"
es = "es_ES-sharvard-medium"
zh = "zh_CN-huayan-medium"
ru = "ru_RU-ruslan-medium"
hi = "hi_IN-pratham-medium"

[voice]
enabled = true
backend = "auto"
target = ""
overlay_enabled = true
shortcut = "$VOICE_SHORTCUT"
record_duration_ms = 4000
sample_rate_hz = 16000
channels = 1
record_command = ""
transcribe_command = "$escaped_transcribe"
transcribe_timeout_ms = 120000

[documents]
enabled = true
chunk_chars = 3200
chunk_overlap_chars = 320
cache_translations = true
cache_audio = true

[ui]
overlay = "compact"
show_notification = true
EOF
}

build_and_install_binaries() {
    run cargo build --release --workspace --features gtk-overlay
    mkdir -p "$BIN_DIR"
    run install -Dm755 "$ROOT_DIR/target/release/visionclip" "$BIN_DIR/visionclip"
    run install -Dm755 "$ROOT_DIR/target/release/visionclip-daemon" "$BIN_DIR/visionclip-daemon"
    run install -Dm755 "$ROOT_DIR/target/release/visionclip-config" "$BIN_DIR/visionclip-config"
}

write_piper_wrapper() {
    mkdir -p "$BIN_DIR"
    cat >"$BIN_DIR/visionclip-piper-http" <<EOF
#!/usr/bin/env bash
set -euo pipefail

PIPER_PYTHON="${VENV_DIR}/bin/python"
VOICE_DIR="\${VISIONCLIP_PIPER_VOICE_DIR:-${PIPER_VOICE_DIR}}"
VOICE_PATH="\${VISIONCLIP_PIPER_VOICE:-\$VOICE_DIR/${PIPER_DEFAULT_VOICE}.onnx}"
PIPER_PORT="\${VISIONCLIP_PIPER_PORT:-5000}"

if [[ ! -f "\$VOICE_PATH" ]]; then
  echo "Piper voice not found: \$VOICE_PATH" >&2
  echo "Run: ${ROOT_DIR}/scripts/install_visionclip.sh --skip-system-packages --skip-ollama-pull --skip-hf-download" >&2
  exit 1
fi

exec "\$PIPER_PYTHON" -m piper.http_server \\
  -m "\$VOICE_PATH" \\
  --data-dir "\$VOICE_DIR" \\
  --download-dir "\$VOICE_DIR" \\
  --host 127.0.0.1 \\
  --port "\$PIPER_PORT"
EOF
    chmod +x "$BIN_DIR/visionclip-piper-http"
}

write_fallback_voice_wrapper() {
    mkdir -p "$BIN_DIR"
    cat >"$BIN_DIR/visionclip-voice-search" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
LOG_DIR="${HOME}/.local/state/visionclip"
LOG_FILE="${LOG_DIR}/voice-shortcut.log"
mkdir -p "$LOG_DIR"
printf '%s visionclip voice shortcut invoked\n' "$(date --iso-8601=seconds)" >>"$LOG_FILE"
if command -v pgrep >/dev/null 2>&1 && command -v kill >/dev/null 2>&1; then
    while IFS= read -r line; do
        pid="${line%% *}"
        command_line="${line#* }"
        case "$pid" in
            ''|*[!0-9]*) continue ;;
        esac
        if [[ "$pid" == "$$" ]]; then
            continue
        fi
        case "$command_line" in
            *pw-play*"visionclip-"*.wav*|*paplay*"visionclip-"*.wav*|*aplay*"visionclip-"*.wav*)
                kill -INT "$pid" 2>/dev/null || true
                ;;
        esac
    done < <(pgrep -af 'visionclip-.*\.wav' 2>/dev/null || true)
fi
exec "${HOME}/.local/bin/visionclip" --voice-agent --speak "$@" >>"$LOG_FILE" 2>&1
EOF
    chmod +x "$BIN_DIR/visionclip-voice-search"
}

install_systemd_units() {
    mkdir -p "$USER_SYSTEMD_DIR"
    run cp "$ROOT_DIR/deploy/systemd/visionclip-daemon.service" "$USER_SYSTEMD_DIR/visionclip-daemon.service"
    run cp "$ROOT_DIR/deploy/systemd/piper-http.service" "$USER_SYSTEMD_DIR/piper-http.service"
    if command -v systemctl >/dev/null 2>&1; then
        systemctl --user import-environment DISPLAY WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS PATH >/dev/null 2>&1 || true
        run systemctl --user daemon-reload
        if [[ "$SKIP_START" != "1" ]]; then
            run systemctl --user enable --now piper-http.service
            run systemctl --user enable --now visionclip-daemon.service
        fi
    else
        warn "systemctl not found; user services were copied but not enabled"
    fi
}

install_gnome_shortcut() {
    if [[ "$SKIP_SHORTCUT" == "1" ]]; then
        return
    fi
    if ! command -v gsettings >/dev/null 2>&1; then
        warn "gsettings not found; skipping GNOME shortcut"
        return
    fi
    run bash "$ROOT_DIR/scripts/install_gnome_voice_shortcut.sh" "$VOICE_SHORTCUT"
}

run_doctor_checks() {
    if [[ "$SKIP_START" == "1" ]]; then
        return
    fi
    if command -v "$BIN_DIR/visionclip-config" >/dev/null 2>&1; then
        "$BIN_DIR/visionclip-config" doctor || warn "visionclip-config doctor reported problems"
    fi
    if command -v "$BIN_DIR/visionclip" >/dev/null 2>&1; then
        "$BIN_DIR/visionclip" --doctor || warn "visionclip --doctor reported problems"
    fi
}

main() {
    parse_args "$@"

    mkdir -p "$BIN_DIR" "$CONFIG_DIR" "$DATA_DIR" "$STATE_DIR" "$LOG_DIR" "$PIPER_VOICE_DIR" "$STT_CACHE_DIR"

    install_system_packages
    ensure_rust
    ensure_ollama
    ensure_python_runtime
    download_piper_voices
    ensure_ollama_running
    pull_ollama_model
    download_hf_model_if_requested
    write_config
    build_and_install_binaries
    write_piper_wrapper
    write_fallback_voice_wrapper
    install_systemd_units
    install_gnome_shortcut
    run_doctor_checks

    echo
    info "VisionClip installation finished."
    info "Binaries: $BIN_DIR"
    info "Config: $CONFIG_PATH"
    info "Piper voices: $PIPER_VOICE_DIR"
    info "Try: visionclip --voice-agent --voice-transcript 'Abra o terminal' --speak"
}

main "$@"
