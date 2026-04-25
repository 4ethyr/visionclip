#!/usr/bin/env bash
set -euo pipefail

SHORTCUT_NAME="${VISIONCLIP_VOICE_SHORTCUT_NAME:-VisionClip Voice Search}"
SHORTCUT_BINDING="${1:-${VISIONCLIP_VOICE_SHORTCUT:-<Super>F12}}"
CUSTOM_KEYBINDINGS_SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
CUSTOM_KEYBINDING_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search/"
CUSTOM_KEYBINDING_SCHEMA="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${CUSTOM_KEYBINDING_PATH}"
USER_BIN_DIR="${HOME}/.local/bin"
VISIONCLIP_BIN="${VISIONCLIP_BIN:-${USER_BIN_DIR}/visionclip}"
WRAPPER_PATH="${USER_BIN_DIR}/visionclip-voice-search"

normalize_binding() {
    local value="$1"
    local lowered="${value,,}"
    case "$lowered" in
        "/+f12"|"slash+f12"|"slash + f12")
            echo "<Super><Alt>F12"
            ;;
        *)
            echo "$value"
            ;;
    esac
}

append_custom_keybinding_path() {
    local current="$1"
    local path="$2"

    if [[ "$current" == "@as []" || "$current" == "[]" ]]; then
        printf "['%s']" "$path"
        return
    fi

    if [[ "$current" == *"'$path'"* ]]; then
        printf '%s' "$current"
        return
    fi

    local trimmed="${current%]}"
    printf "%s, '%s']" "$trimmed" "$path"
}

require_command() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Erro: comando obrigatorio ausente: $cmd" >&2
        exit 1
    fi
}

mkdir -p "$USER_BIN_DIR"
require_command gsettings

if [[ ! -x "$VISIONCLIP_BIN" ]]; then
    echo "Erro: binario do VisionClip nao encontrado em $VISIONCLIP_BIN" >&2
    echo "Compile e copie o binario para ~/.local/bin/visionclip antes de instalar o atalho." >&2
    exit 1
fi

cat >"$WRAPPER_PATH" <<EOF
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "\${HOME}/.local/state/visionclip"
printf '%s visionclip voice shortcut invoked\n' "\$(date --iso-8601=seconds)" >>"\${HOME}/.local/state/visionclip/voice-shortcut.log"
exec "$VISIONCLIP_BIN" --voice-agent --speak "\$@"
EOF
chmod +x "$WRAPPER_PATH"

RESOLVED_BINDING="$(normalize_binding "$SHORTCUT_BINDING")"
if [[ "$RESOLVED_BINDING" != "$SHORTCUT_BINDING" ]]; then
    echo "Aviso: o GNOME nao aceita o atalho global '$SHORTCUT_BINDING' com duas teclas normais."
    echo "Usando '$RESOLVED_BINDING' como padrao compativel."
fi

CURRENT_BINDINGS="$(gsettings get "$CUSTOM_KEYBINDINGS_SCHEMA" custom-keybindings)"
UPDATED_BINDINGS="$(append_custom_keybinding_path "$CURRENT_BINDINGS" "$CUSTOM_KEYBINDING_PATH")"

gsettings set "$CUSTOM_KEYBINDINGS_SCHEMA" custom-keybindings "$UPDATED_BINDINGS"
gsettings set "$CUSTOM_KEYBINDING_SCHEMA" name "$SHORTCUT_NAME"
gsettings set "$CUSTOM_KEYBINDING_SCHEMA" command "$WRAPPER_PATH"
gsettings set "$CUSTOM_KEYBINDING_SCHEMA" binding "$RESOLVED_BINDING"

echo "Atalho do VisionClip configurado."
echo "Binding: $RESOLVED_BINDING"
echo "Command: $WRAPPER_PATH"
