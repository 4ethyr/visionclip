#!/usr/bin/env bash
set -euo pipefail

EXTENSION_UUID="visionclip-status@visionclip"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_DIR="$ROOT_DIR/deploy/gnome-shell/$EXTENSION_UUID"
TARGET_DIR="${HOME}/.local/share/gnome-shell/extensions/$EXTENSION_UUID"

if [[ ! -d "$SOURCE_DIR" ]]; then
    echo "Erro: extensao GNOME nao encontrada em $SOURCE_DIR" >&2
    exit 1
fi

if ! command -v gnome-extensions >/dev/null 2>&1; then
    echo "Aviso: gnome-extensions nao encontrado; copiando a extensao sem habilitar automaticamente." >&2
fi

enable_in_gsettings() {
    if ! command -v gsettings >/dev/null 2>&1; then
        return
    fi

    local current
    current="$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || true)"
    if [[ -z "$current" || "$current" == *"'$EXTENSION_UUID'"* ]]; then
        return
    fi

    local next
    if [[ "$current" == "@as []" || "$current" == "[]" ]]; then
        next="['$EXTENSION_UUID']"
    elif [[ "$current" == \[*\] ]]; then
        next="${current%]}, '$EXTENSION_UUID']"
    else
        return
    fi

    gsettings set org.gnome.shell enabled-extensions "$next" >/dev/null 2>&1 || true
}

mkdir -p "$TARGET_DIR"
cp "$SOURCE_DIR"/metadata.json "$TARGET_DIR"/metadata.json
cp "$SOURCE_DIR"/extension.js "$TARGET_DIR"/extension.js
cp "$SOURCE_DIR"/stylesheet.css "$TARGET_DIR"/stylesheet.css

enable_in_gsettings

if command -v gnome-extensions >/dev/null 2>&1; then
    if gnome-extensions enable "$EXTENSION_UUID" 2>/dev/null; then
        echo "Indicador GNOME do VisionClip habilitado."
    else
        echo "Indicador GNOME copiado. Se ele nao aparecer agora, encerre a sessao e entre novamente, depois rode:" >&2
        echo "  gnome-extensions enable $EXTENSION_UUID" >&2
    fi
fi

echo "Extensao instalada em: $TARGET_DIR"
