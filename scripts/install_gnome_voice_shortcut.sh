#!/usr/bin/env bash
set -euo pipefail

SHORTCUT_NAME="${VISIONCLIP_VOICE_SHORTCUT_NAME:-VisionClip Voice Search}"
SHORTCUT_BINDING="${1:-${VISIONCLIP_VOICE_SHORTCUT:-<Super>F12}}"
SECONDARY_SHORTCUT_BINDING="${VISIONCLIP_VOICE_SECONDARY_SHORTCUT:-<Super><Shift>F12}"
TERTIARY_SHORTCUT_BINDING="${VISIONCLIP_VOICE_TERTIARY_SHORTCUT:-<Super><Alt>v}"
CUSTOM_KEYBINDINGS_SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
CUSTOM_KEYBINDING_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search/"
CUSTOM_KEYBINDING_SCHEMA="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${CUSTOM_KEYBINDING_PATH}"
SECONDARY_CUSTOM_KEYBINDING_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search-shift/"
SECONDARY_CUSTOM_KEYBINDING_SCHEMA="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${SECONDARY_CUSTOM_KEYBINDING_PATH}"
TERTIARY_CUSTOM_KEYBINDING_PATH="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search-super-alt-v/"
TERTIARY_CUSTOM_KEYBINDING_SCHEMA="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${TERTIARY_CUSTOM_KEYBINDING_PATH}"
USER_BIN_DIR="${HOME}/.local/bin"
VISIONCLIP_BIN="${VISIONCLIP_BIN:-${USER_BIN_DIR}/visionclip}"
WRAPPER_PATH="${USER_BIN_DIR}/visionclip-voice-search"

normalize_binding() {
    local value="$1"
    local lowered="${value,,}"
    case "$lowered" in
        "/+f12"|"slash+f12"|"slash + f12")
            echo "<Mod4>F12"
            ;;
        "shift+capslk"|"shift + capslk"|"shift+capslock"|"shift + capslock"|"shift+caps_lock"|"shift + caps_lock")
            echo "<Shift>Caps_Lock"
            ;;
        *)
            echo "${value//<Super>/<Mod4>}"
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

LOG_DIR="\${HOME}/.local/state/visionclip"
LOG_FILE="\${LOG_DIR}/voice-shortcut.log"
mkdir -p "\$LOG_DIR"

import_user_env_var() {
    local key="\$1"
    if [[ -n "\${!key:-}" ]]; then
        return
    fi
    if ! command -v systemctl >/dev/null 2>&1; then
        return
    fi

    local line
    while IFS= read -r line; do
        case "\$line" in
            "\${key}="*)
                export "\$line"
                return
                ;;
        esac
    done < <(systemctl --user show-environment 2>/dev/null || true)
}

for key in DISPLAY WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS; do
    import_user_env_var "\$key"
done

export PATH="\${PATH:-\${HOME}/.local/bin:/usr/local/bin:/usr/bin:/bin}"

{
    printf '%s visionclip voice shortcut invoked\n' "\$(date --iso-8601=seconds)"
    printf 'binary=%s\n' "$VISIONCLIP_BIN"
    printf 'env DISPLAY=%s WAYLAND_DISPLAY=%s XDG_SESSION_TYPE=%s XDG_CURRENT_DESKTOP=%s XDG_RUNTIME_DIR=%s\n' "\${DISPLAY:-}" "\${WAYLAND_DISPLAY:-}" "\${XDG_SESSION_TYPE:-}" "\${XDG_CURRENT_DESKTOP:-}" "\${XDG_RUNTIME_DIR:-}"
} >>"\$LOG_FILE"

if [[ -t 1 || -t 2 ]]; then
    exec "$VISIONCLIP_BIN" --voice-agent --speak "\$@"
else
    exec "$VISIONCLIP_BIN" --voice-agent --speak "\$@" >>"\$LOG_FILE" 2>&1
fi
EOF
chmod +x "$WRAPPER_PATH"

RESOLVED_BINDING="$(normalize_binding "$SHORTCUT_BINDING")"
RESOLVED_SECONDARY_BINDING="$(normalize_binding "$SECONDARY_SHORTCUT_BINDING")"
RESOLVED_TERTIARY_BINDING="$(normalize_binding "$TERTIARY_SHORTCUT_BINDING")"
case "${SHORTCUT_BINDING,,}" in
    "/+f12"|"slash+f12"|"slash + f12")
    echo "Aviso: o GNOME nao aceita o atalho global '$SHORTCUT_BINDING' com duas teclas normais."
    echo "Usando '$RESOLVED_BINDING' como padrao compativel."
        ;;
esac

CURRENT_BINDINGS="$(gsettings get "$CUSTOM_KEYBINDINGS_SCHEMA" custom-keybindings)"
UPDATED_BINDINGS="$(append_custom_keybinding_path "$CURRENT_BINDINGS" "$CUSTOM_KEYBINDING_PATH")"
UPDATED_BINDINGS="$(append_custom_keybinding_path "$UPDATED_BINDINGS" "$SECONDARY_CUSTOM_KEYBINDING_PATH")"
UPDATED_BINDINGS="$(append_custom_keybinding_path "$UPDATED_BINDINGS" "$TERTIARY_CUSTOM_KEYBINDING_PATH")"

gsettings set "$CUSTOM_KEYBINDINGS_SCHEMA" custom-keybindings "$UPDATED_BINDINGS"
gsettings set "$CUSTOM_KEYBINDING_SCHEMA" name "$SHORTCUT_NAME"
gsettings set "$CUSTOM_KEYBINDING_SCHEMA" command "$WRAPPER_PATH"
gsettings set "$CUSTOM_KEYBINDING_SCHEMA" binding "$RESOLVED_BINDING"
gsettings set "$SECONDARY_CUSTOM_KEYBINDING_SCHEMA" name "$SHORTCUT_NAME (fallback)"
gsettings set "$SECONDARY_CUSTOM_KEYBINDING_SCHEMA" command "$WRAPPER_PATH"
gsettings set "$SECONDARY_CUSTOM_KEYBINDING_SCHEMA" binding "$RESOLVED_SECONDARY_BINDING"
gsettings set "$TERTIARY_CUSTOM_KEYBINDING_SCHEMA" name "$SHORTCUT_NAME (fallback alt)"
gsettings set "$TERTIARY_CUSTOM_KEYBINDING_SCHEMA" command "$WRAPPER_PATH"
gsettings set "$TERTIARY_CUSTOM_KEYBINDING_SCHEMA" binding "$RESOLVED_TERTIARY_BINDING"

if command -v systemctl >/dev/null 2>&1; then
    systemctl --user import-environment DISPLAY WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS PATH >/dev/null 2>&1 || true
    systemctl --user restart org.gnome.SettingsDaemon.MediaKeys.target >/dev/null 2>&1 || \
        systemctl --user start org.gnome.SettingsDaemon.MediaKeys.target >/dev/null 2>&1 || true
fi

echo "Atalho do VisionClip configurado."
echo "Binding: $RESOLVED_BINDING"
echo "Fallback binding: $RESOLVED_SECONDARY_BINDING"
echo "Fallback alt binding: $RESOLVED_TERTIARY_BINDING"
echo "Command: $WRAPPER_PATH"
echo "Log: ${HOME}/.local/state/visionclip/voice-shortcut.log"
