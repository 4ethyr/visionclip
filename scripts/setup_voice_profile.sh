#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONFIG_DIR="${VISIONCLIP_CONFIG_DIR:-$HOME/.config/visionclip}"
CONFIG_PATH="${VISIONCLIP_CONFIG:-$CONFIG_DIR/config.toml}"
DATA_DIR="${VISIONCLIP_DATA_DIR:-$HOME/.local/share/visionclip}"
USER_SYSTEMD_DIR="$HOME/.config/systemd/user"

VISIONCLIP_BIN="${VISIONCLIP_BIN:-}"
SAMPLES="${VISIONCLIP_VOICE_PROFILE_SAMPLES:-10}"
SAMPLE_SECONDS="${VISIONCLIP_VOICE_PROFILE_SAMPLE_SECONDS:-10}"
LABEL="${VISIONCLIP_VOICE_PROFILE_LABEL:-main}"
THRESHOLD="${VISIONCLIP_SPEAKER_VERIFICATION_THRESHOLD:-0.72}"
MIN_SAMPLES="${VISIONCLIP_SPEAKER_VERIFICATION_MIN_SAMPLES:-3}"
WAKE_ENABLED=""
SPEAKER_VERIFICATION_ENABLED="1"
RECORD_NOW=""
YES=0
DRY_RUN=0
NO_ENROLL=0
NO_START=0

ENROLLMENT_PHRASES=(
    "Key, abra o terminal."
    "Key, abra o YouTube."
    "Key, abra o livro Black Hat Python."
    "Key, abra o livro Programming TypeScript."
    "Key, pesquise sobre Rust async no Linux."
    "Key, traduza essa tela."
    "Key, explique esse erro."
    "Key, continue a leitura do livro."
    "Key, pause a leitura."
    "Key, open the book Grey Hat Python."
)

usage() {
    cat <<'EOF'
Usage:
  scripts/setup_voice_profile.sh [options]

Onboarding guiado de perfil de voz local para o VisionClip.

Options:
  -y, --yes                         Aceita padrões seguros em modo não interativo.
  --dry-run                         Mostra mudanças sem gravar config nem áudio.
  --samples N                       Número de frases para gravar, 1..10. Padrão: 10.
  --sample-seconds N                Tempo por frase, 3..20 segundos. Padrão: 10.
  --label NAME                      Rótulo do perfil. Padrão: main.
  --threshold VALUE                 Limiar do gate de locutor, 0.50..0.99. Padrão: 0.72.
  --min-samples N                   Mínimo de amostras salvo na config. Padrão: 3.
  --enable-wake                     Habilita escuta passiva por "Key".
  --disable-wake                    Desabilita escuta passiva por "Key".
  --enable-speaker-verification     Habilita verificação local de locutor. Padrão.
  --disable-speaker-verification    Mantém verificação de locutor desabilitada.
  --no-enroll                       Só atualiza config; não grava amostras.
  --no-start                        Não reinicia/habilita serviços systemd de usuário.
  -h, --help                        Mostra esta ajuda.

Environment:
  VISIONCLIP_BIN                    Caminho do binário visionclip.
  VISIONCLIP_CONFIG                 Caminho do config.toml.
  VISIONCLIP_VOICE_PROFILE_SAMPLES  Número padrão de amostras.
  VISIONCLIP_VOICE_PROFILE_SAMPLE_SECONDS
                                  Tempo padrão por frase.
  VISIONCLIP_ALLOW_NONINTERACTIVE_ENROLL=1
                                  Permite gravar mesmo sem TTY interativo.

Privacy:
  Este script grava somente com consentimento explícito. O VisionClip salva
  um perfil acústico derivado em ~/.local/share/visionclip/voice-profile.json.
  WAVs temporários são removidos pela CLI após cada amostra. Isso é um filtro
  local de conveniência para wake word, não autenticação biométrica forte.
EOF
}

info() {
    printf '[visionclip-voice-setup] %s\n' "$*"
}

warn() {
    printf '[visionclip-voice-setup][warn] %s\n' "$*" >&2
}

die() {
    printf '[visionclip-voice-setup][error] %s\n' "$*" >&2
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

run() {
    if [[ "$DRY_RUN" == "1" ]]; then
        printf '[visionclip-voice-setup] dry-run:'
        printf ' %q' "$@"
        printf '\n'
    else
        info "running: $*"
        "$@"
    fi
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -y|--yes)
                YES=1
                ;;
            --dry-run)
                DRY_RUN=1
                ;;
            --samples)
                SAMPLES="${2:?missing value for --samples}"
                shift
                ;;
            --sample-seconds)
                SAMPLE_SECONDS="${2:?missing value for --sample-seconds}"
                shift
                ;;
            --label)
                LABEL="${2:?missing value for --label}"
                shift
                ;;
            --threshold)
                THRESHOLD="${2:?missing value for --threshold}"
                shift
                ;;
            --min-samples)
                MIN_SAMPLES="${2:?missing value for --min-samples}"
                shift
                ;;
            --enable-wake)
                WAKE_ENABLED="1"
                ;;
            --disable-wake)
                WAKE_ENABLED="0"
                ;;
            --enable-speaker-verification)
                SPEAKER_VERIFICATION_ENABLED="1"
                ;;
            --disable-speaker-verification)
                SPEAKER_VERIFICATION_ENABLED="0"
                ;;
            --no-enroll)
                NO_ENROLL=1
                ;;
            --no-start)
                NO_START=1
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

validate_number_config() {
    [[ "$SAMPLES" =~ ^[0-9]+$ ]] || die "--samples must be an integer"
    [[ "$SAMPLE_SECONDS" =~ ^[0-9]+$ ]] || die "--sample-seconds must be an integer"
    [[ "$MIN_SAMPLES" =~ ^[0-9]+$ ]] || die "--min-samples must be an integer"
    (( SAMPLES >= 1 && SAMPLES <= 10 )) || die "--samples deve ficar entre 1 e 10"
    (( SAMPLE_SECONDS >= 3 && SAMPLE_SECONDS <= 20 )) || die "--sample-seconds deve ficar entre 3 e 20"
    (( MIN_SAMPLES >= 1 && MIN_SAMPLES <= 10 )) || die "--min-samples deve ficar entre 1 e 10"
    if ! python3 - "$THRESHOLD" <<'PY'
import sys
try:
    value = float(sys.argv[1])
except ValueError:
    raise SystemExit(1)
if not 0.50 <= value <= 0.99:
    raise SystemExit(1)
PY
    then
        die "--threshold deve ser um número entre 0.50 e 0.99"
    fi
}

resolve_visionclip_bin() {
    if [[ -n "$VISIONCLIP_BIN" ]]; then
        [[ -x "$VISIONCLIP_BIN" ]] || die "VISIONCLIP_BIN is not executable: $VISIONCLIP_BIN"
        return
    fi

    if command -v visionclip >/dev/null 2>&1; then
        VISIONCLIP_BIN="$(command -v visionclip)"
    elif [[ -x "$HOME/.local/bin/visionclip" ]]; then
        VISIONCLIP_BIN="$HOME/.local/bin/visionclip"
    elif [[ -x "$ROOT_DIR/target/debug/visionclip" ]]; then
        VISIONCLIP_BIN="$ROOT_DIR/target/debug/visionclip"
    elif [[ -x "$ROOT_DIR/target/release/visionclip" ]]; then
        VISIONCLIP_BIN="$ROOT_DIR/target/release/visionclip"
    else
        die "visionclip binary not found. Run scripts/install_visionclip.sh or set VISIONCLIP_BIN=/path/to/visionclip"
    fi
}

print_intro() {
    cat <<'EOF'

Onboarding de voz do VisionClip

Este fluxo guiado vai:
  1. habilitar input de voz local no ~/.config/visionclip/config.toml;
  2. opcionalmente habilitar escuta passiva por "Key";
  3. opcionalmente gravar ate 10 frases curtas com a sua voz;
  4. habilitar verificação local de locutor para reduzir ativações por
     pessoas próximas ou áudio/vídeo tocando no sistema.

Orientações para gravação:
  - pause YouTube, música e chamadas durante o cadastro;
  - fale naturalmente, perto do microfone;
  - use frases com "Key" e comandos reais em mais de um idioma se esse for
    seu uso normal;
  - cada amostra tera ate 10 segundos por padrao;
  - leia uma frase por vez e aguarde o pedido da proxima frase.

O perfil é local-first. Este script não envia áudio nem perfil para provedores
cloud. Isso não é autenticação biométrica forte; é um gate local prático.

EOF
}

choose_wake_mode() {
    if [[ -n "$WAKE_ENABLED" ]]; then
        return
    fi

    if confirm "Habilitar escuta passiva por \"Key\"?" "y"; then
        WAKE_ENABLED="1"
    else
        WAKE_ENABLED="0"
    fi
}

choose_recording() {
    if [[ "$NO_ENROLL" == "1" ]]; then
        RECORD_NOW="0"
        return
    fi

    if [[ -n "$RECORD_NOW" ]]; then
        return
    fi

    if ! is_interactive && [[ "${VISIONCLIP_ALLOW_NONINTERACTIVE_ENROLL:-0}" != "1" ]]; then
        warn "sem TTY interativo; gravação de amostras será pulada"
        RECORD_NOW="0"
        return
    fi

    if confirm "Gravar o perfil local de locutor agora?" "y"; then
        RECORD_NOW="1"
    else
        RECORD_NOW="0"
    fi
}

patch_voice_config() {
    mkdir -p "$CONFIG_DIR" "$DATA_DIR"

    info "Config path: $CONFIG_PATH"
    info "voice.enabled=true"
    info "voice.wake_word_enabled=$(bool_word "$WAKE_ENABLED")"
    info "voice.wake_block_during_playback=true"
    info "voice.speaker_verification_enabled=$(bool_word "$SPEAKER_VERIFICATION_ENABLED")"
    info "voice.speaker_verification_threshold=$THRESHOLD"
    info "voice.speaker_verification_min_samples=$MIN_SAMPLES"
    info "voice.overlay_enabled=false"

    if [[ "$DRY_RUN" == "1" ]]; then
        return
    fi

    CONFIG_PATH="$CONFIG_PATH" \
    VOICE_ENABLED="true" \
    WAKE_WORD_ENABLED="$(bool_word "$WAKE_ENABLED")" \
    WAKE_BLOCK_DURING_PLAYBACK="true" \
    SPEAKER_VERIFICATION_ENABLED="$(bool_word "$SPEAKER_VERIFICATION_ENABLED")" \
    SPEAKER_VERIFICATION_THRESHOLD="$THRESHOLD" \
    SPEAKER_VERIFICATION_MIN_SAMPLES="$MIN_SAMPLES" \
    OVERLAY_ENABLED="false" \
    python3 <<'PY'
from __future__ import annotations

import os
import shutil
import tempfile
import time
from pathlib import Path

path = Path(os.environ["CONFIG_PATH"]).expanduser()
path.parent.mkdir(parents=True, exist_ok=True)
text = path.read_text(encoding="utf-8") if path.exists() else ""
original = text

values = {
    "enabled": os.environ["VOICE_ENABLED"],
    "wake_word_enabled": os.environ["WAKE_WORD_ENABLED"],
    "wake_block_during_playback": os.environ["WAKE_BLOCK_DURING_PLAYBACK"],
    "speaker_verification_enabled": os.environ["SPEAKER_VERIFICATION_ENABLED"],
    "speaker_verification_threshold": os.environ["SPEAKER_VERIFICATION_THRESHOLD"],
    "speaker_verification_min_samples": os.environ["SPEAKER_VERIFICATION_MIN_SAMPLES"],
    "overlay_enabled": os.environ["OVERLAY_ENABLED"],
}


def find_section(lines: list[str], name: str) -> tuple[int, int]:
    start = -1
    end = len(lines)
    header = f"[{name}]"
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped == header:
            start = index
            continue
        if start != -1 and stripped.startswith("[") and stripped.endswith("]"):
            end = index
            break
    if start == -1:
        if lines and lines[-1].strip():
            lines.append("")
        lines.append(header)
        start = len(lines) - 1
        end = len(lines)
    return start, end


def set_key(lines: list[str], section_start: int, section_end: int, key: str, value: str) -> int:
    prefix = f"{key} ="
    rendered = f"{key} = {value}"
    for index in range(section_start + 1, section_end):
        if lines[index].strip().startswith(prefix):
            lines[index] = rendered
            return section_end
    lines.insert(section_end, rendered)
    return section_end + 1


lines = text.splitlines()
section_start, section_end = find_section(lines, "voice")
for key, value in values.items():
    section_end = set_key(lines, section_start, section_end, key, value)

updated = "\n".join(lines).rstrip() + "\n"
if updated == original:
    raise SystemExit(0)

if path.exists():
    backup = path.with_suffix(f".toml.bak.{int(time.time())}")
    shutil.copy2(path, backup)
    print(f"backup={backup}")

with tempfile.NamedTemporaryFile("w", encoding="utf-8", dir=path.parent, delete=False) as handle:
    handle.write(updated)
    tmp_name = handle.name
Path(tmp_name).replace(path)
PY
}

bool_word() {
    if [[ "$1" == "1" || "$1" == "true" ]]; then
        printf 'true'
    else
        printf 'false'
    fi
}

install_wake_unit_if_available() {
    if [[ "$NO_START" == "1" || "$DRY_RUN" == "1" ]]; then
        return
    fi
    if [[ ! -f "$ROOT_DIR/deploy/systemd/visionclip-wake-listener.service" ]]; then
        warn "wake listener systemd unit not found in deploy/systemd"
        return
    fi
    mkdir -p "$USER_SYSTEMD_DIR"
    cp "$ROOT_DIR/deploy/systemd/visionclip-wake-listener.service" \
        "$USER_SYSTEMD_DIR/visionclip-wake-listener.service"
}

restart_wake_service() {
    if [[ "$NO_START" == "1" ]]; then
        info "Reinício de serviço pulado porque --no-start foi usado."
        return
    fi
    if ! command -v systemctl >/dev/null 2>&1; then
        warn "systemctl not found; start passive wake manually with: $VISIONCLIP_BIN --wake-listener --speak"
        return
    fi

    install_wake_unit_if_available
    run systemctl --user daemon-reload

    if [[ "$WAKE_ENABLED" == "1" ]]; then
        run systemctl --user enable visionclip-wake-listener.service
        run systemctl --user restart visionclip-wake-listener.service
    else
        run systemctl --user disable --now visionclip-wake-listener.service
    fi
}

record_profile() {
    if [[ "$RECORD_NOW" != "1" ]]; then
        info "Gravação do perfil de locutor pulada."
        return
    fi

    print_enrollment_phrases

    cat <<EOF

A CLI vai gravar $SAMPLES amostra(s), com ate $SAMPLE_SECONDS segundos por frase.
Depois que cada amostra for validada, ela pedira a proxima frase no terminal.
Se alguma amostra ficar baixa ou curta demais, rode o script novamente em um
ambiente mais silencioso ou aumente o ganho do microfone.

EOF

    local enroll_args=(
        voice
        enroll
        --samples "$SAMPLES"
        --label "$LABEL"
        --sample-duration-ms "$((SAMPLE_SECONDS * 1000))"
    )
    local index
    for ((index = 0; index < SAMPLES && index < ${#ENROLLMENT_PHRASES[@]}; index++)); do
        enroll_args+=(--phrase "${ENROLLMENT_PHRASES[$index]}")
    done

    run "$VISIONCLIP_BIN" "${enroll_args[@]}"
    run "$VISIONCLIP_BIN" voice status
}

print_enrollment_phrases() {
    cat <<EOF

Frases que serao usadas para afinar a escuta local:
EOF
    local index
    for ((index = 0; index < SAMPLES && index < ${#ENROLLMENT_PHRASES[@]}; index++)); do
        printf '  %2d. %s\n' "$((index + 1))" "${ENROLLMENT_PHRASES[$index]}"
    done
}

print_summary() {
    cat <<EOF

Onboarding de voz concluído.

Config:
  $CONFIG_PATH

Perfil:
  $DATA_DIR/voice-profile.json

Escuta passiva:
  $(if [[ "$WAKE_ENABLED" == "1" ]]; then printf 'habilitada'; else printf 'desabilitada'; fi)

Próximas verificações:
  visionclip voice status
  visionclip --doctor
  journalctl --user -u visionclip-wake-listener.service -f

EOF
}

main() {
    parse_args "$@"
    command -v python3 >/dev/null 2>&1 || die "python3 is required"
    validate_number_config
    resolve_visionclip_bin
    print_intro
    choose_wake_mode
    choose_recording
    patch_voice_config
    record_profile
    restart_wake_service
    print_summary
}

main "$@"
