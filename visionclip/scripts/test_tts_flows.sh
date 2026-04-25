#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STACK_SCRIPT="$ROOT_DIR/scripts/start_local_stack.sh"
CONFIG_PATH="${VISIONCLIP_CONFIG:-$ROOT_DIR/tools/runtime/local-stack/visionclip-e2e.toml}"
IMAGE_PATH="${VISIONCLIP_TEST_IMAGE:-}"
AUTO_CAPTURE=1

usage() {
    cat <<'EOF'
Uso:
  scripts/test_tts_flows.sh
  scripts/test_tts_flows.sh --image /caminho/imagem.png
  scripts/test_tts_flows.sh --auto

Padrao:
  sobe a stack local, depois roda Explain, TranslatePtBr e SearchWeb com --speak.
  Sem --image, cada acao usa captura automatica e abre o seletor do GNOME/portal.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --image)
            IMAGE_PATH="${2:-}"
            if [[ -z "$IMAGE_PATH" ]]; then
                echo "Erro: --image exige um caminho." >&2
                exit 1
            fi
            AUTO_CAPTURE=0
            shift 2
            ;;
        --auto)
            IMAGE_PATH=""
            AUTO_CAPTURE=1
            shift
            ;;
        --help)
            usage
            exit 0
            ;;
        *)
            echo "Erro: opcao invalida: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ ! -x "$STACK_SCRIPT" ]]; then
    echo "Erro: script ausente ou sem permissao de execucao: $STACK_SCRIPT" >&2
    exit 1
fi

"$STACK_SCRIPT"

if [[ -n "$IMAGE_PATH" && ! -f "$IMAGE_PATH" ]]; then
    echo "Erro: imagem nao encontrada: $IMAGE_PATH" >&2
    exit 1
fi

run_action() {
    local action="$1"
    local description="$2"
    local -a cmd=("$ROOT_DIR/target/debug/visionclip" --action "$action" --speak)

    if [[ "$AUTO_CAPTURE" -eq 0 ]]; then
        cmd+=(--image "$IMAGE_PATH")
    fi

    echo
    echo "==> $description"
    echo "Comando: VISIONCLIP_CONFIG=$CONFIG_PATH ${cmd[*]}"
    VISIONCLIP_CONFIG="$CONFIG_PATH" "${cmd[@]}"
}

run_action "explain" "Explicar"
run_action "translate_ptbr" "Traduzir"
run_action "search_web" "Pesquisar"
