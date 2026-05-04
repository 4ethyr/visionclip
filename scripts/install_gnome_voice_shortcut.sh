#!/usr/bin/env bash
set -euo pipefail

CUSTOM_KEYBINDINGS_SCHEMA="org.gnome.settings-daemon.plugins.media-keys"
WM_KEYBINDINGS_SCHEMA="org.gnome.desktop.wm.keybindings"
USER_BIN_DIR="${HOME}/.local/bin"
VISIONCLIP_BIN="${VISIONCLIP_BIN:-${USER_BIN_DIR}/visionclip}"
LOG_DIR="${HOME}/.local/state/visionclip"

VOICE_AGENT_BINDING="${1:-${VISIONCLIP_VOICE_SHORTCUT:-<Super>space}}"
CAPTURE_EXPLAIN_BINDING="${VISIONCLIP_CAPTURE_EXPLAIN_SHORTCUT:-<Super>1}"
CAPTURE_TRANSLATE_BINDING="${VISIONCLIP_CAPTURE_TRANSLATE_SHORTCUT:-<Super>2}"
VOICE_SEARCH_BINDING="${VISIONCLIP_VOICE_SEARCH_SHORTCUT:-<Super>3}"
BOOK_READ_BINDING="${VISIONCLIP_BOOK_READ_SHORTCUT:-<Super>4}"
BOOK_TRANSLATE_READ_BINDING="${VISIONCLIP_BOOK_TRANSLATE_READ_SHORTCUT:-<Super>5}"

OLD_KEYBINDING_PATHS=(
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search/"
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search-shift/"
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search-super-alt-v/"
)

SHORTCUT_PATHS=(
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-agent/"
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-capture-explain/"
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-capture-translate/"
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-voice-search/"
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-book-read/"
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/visionclip-book-translate-read/"
)

normalize_binding() {
    local value="$1"
    local lowered="${value,,}"
    case "$lowered" in
        "windows+space"|"win+space"|"super+space"|"meta+space")
            echo "<Mod4>space"
            ;;
        "windows+1"|"win+1"|"super+1"|"meta+1")
            echo "<Mod4>1"
            ;;
        "windows+2"|"win+2"|"super+2"|"meta+2")
            echo "<Mod4>2"
            ;;
        "windows+3"|"win+3"|"super+3"|"meta+3")
            echo "<Mod4>3"
            ;;
        "windows+4"|"win+4"|"super+4"|"meta+4")
            echo "<Mod4>4"
            ;;
        "windows+5"|"win+5"|"super+5"|"meta+5")
            echo "<Mod4>5"
            ;;
        "windows+space+1"|"win+space+1"|"super+space+1"|"meta+space+1")
            echo "<Mod4>1"
            ;;
        "windows+space+2"|"win+space+2"|"super+space+2"|"meta+space+2")
            echo "<Mod4>2"
            ;;
        "windows+space+3"|"win+space+3"|"super+space+3"|"meta+space+3")
            echo "<Mod4>3"
            ;;
        "windows+space+4"|"win+space+4"|"super+space+4"|"meta+space+4")
            echo "<Mod4>4"
            ;;
        "windows+space+5"|"win+space+5"|"super+space+5"|"meta+space+5")
            echo "<Mod4>5"
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

filter_custom_keybinding_paths() {
    local current="$1"
    shift
    python3 - "$current" "$@" <<'PY'
import ast
import sys

raw = sys.argv[1]
remove = set(sys.argv[2:])
if raw.startswith("@as "):
    raw = raw[4:]
try:
    paths = ast.literal_eval(raw)
except Exception:
    paths = []
paths = [path for path in paths if path not in remove]
print("[" + ", ".join(repr(path) for path in paths) + "]")
PY
}

remove_accelerators_from_list() {
    local current="$1"
    shift
    python3 - "$current" "$@" <<'PY'
import ast
import sys

raw = sys.argv[1]
remove = {value.replace("<Super>", "<Mod4>").lower() for value in sys.argv[2:] if value}
if raw.startswith("@as "):
    raw = raw[4:]
try:
    values = ast.literal_eval(raw)
except Exception:
    values = []
filtered = [
    value for value in values
    if str(value).replace("<Super>", "<Mod4>").lower() not in remove
]
print("[" + ", ".join(repr(value) for value in filtered) + "]")
PY
}

require_command() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "Erro: comando obrigatorio ausente: $cmd" >&2
        exit 1
    fi
}

write_wrapper() {
    local path="$1"
    local label="$2"
    shift 2
    local args=("$@")
    local rendered_args
    printf -v rendered_args ' %q' "${args[@]}"
    rendered_args="${rendered_args# }"

    cat >"$path" <<EOF
#!/usr/bin/env bash
set -euo pipefail

LOG_DIR="${LOG_DIR}"
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

interrupt_visionclip_tts() {
    if ! command -v pgrep >/dev/null 2>&1 || ! command -v kill >/dev/null 2>&1; then
        return
    fi

    local line pid command_line
    while IFS= read -r line; do
        pid="\${line%% *}"
        command_line="\${line#* }"
        case "\$pid" in
            ''|*[!0-9]*) continue ;;
        esac
        if [[ "\$pid" == "\$\$" ]]; then
            continue
        fi
        case "\$command_line" in
            *pw-play*"visionclip-"*.wav*|*paplay*"visionclip-"*.wav*|*aplay*"visionclip-"*.wav*)
                kill -INT "\$pid" 2>/dev/null || true
                ;;
        esac
    done < <(pgrep -af 'visionclip-.*\\.wav' 2>/dev/null || true)
}

{
    printf '%s ${label} invoked\n' "\$(date --iso-8601=seconds)"
    printf 'binary=%s args=%s\n' "$VISIONCLIP_BIN" "$rendered_args"
    printf 'env DISPLAY=%s WAYLAND_DISPLAY=%s XDG_SESSION_TYPE=%s XDG_CURRENT_DESKTOP=%s XDG_RUNTIME_DIR=%s\n' "\${DISPLAY:-}" "\${WAYLAND_DISPLAY:-}" "\${XDG_SESSION_TYPE:-}" "\${XDG_CURRENT_DESKTOP:-}" "\${XDG_RUNTIME_DIR:-}"
} >>"\$LOG_FILE"

interrupt_visionclip_tts

if [[ -t 1 || -t 2 ]]; then
    exec "$VISIONCLIP_BIN" $rendered_args "\$@"
else
    exec "$VISIONCLIP_BIN" $rendered_args "\$@" >>"\$LOG_FILE" 2>&1
fi
EOF
    chmod +x "$path"
}

configure_shortcut() {
    local path="$1"
    local name="$2"
    local command="$3"
    local binding="$4"
    local schema="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:${path}"

    gsettings set "$schema" name "$name"
    gsettings set "$schema" command "$command"
    gsettings set "$schema" binding "$binding"
}

clear_conflicting_gnome_bindings() {
    local bindings=("$@")
    local key current updated

    for key in switch-input-source switch-input-source-backward; do
        current="$(gsettings get "$WM_KEYBINDINGS_SCHEMA" "$key" 2>/dev/null || true)"
        if [[ -z "$current" ]]; then
            continue
        fi
        updated="$(remove_accelerators_from_list "$current" "${bindings[@]}")"
        if [[ "$updated" != "$current" ]]; then
            gsettings set "$WM_KEYBINDINGS_SCHEMA" "$key" "$updated"
            echo "Removido conflito GNOME $key: $current -> $updated"
        fi
    done
}

mkdir -p "$USER_BIN_DIR"
require_command gsettings
require_command python3

if [[ ! -x "$VISIONCLIP_BIN" ]]; then
    echo "Erro: binario do VisionClip nao encontrado em $VISIONCLIP_BIN" >&2
    echo "Compile e copie o binario para ~/.local/bin/visionclip antes de instalar o atalho." >&2
    exit 1
fi

if [[ "${VISIONCLIP_ALLOW_CHORD_HINT:-1}" == "1" ]]; then
    echo "Nota: GNOME custom shortcuts nao suportam chording real como Windows+Space+1."
    echo "Instalando Windows+Space para voz e Windows+1..5 para os modos."
fi

VOICE_AGENT_WRAPPER="${USER_BIN_DIR}/visionclip-voice-agent"
CAPTURE_EXPLAIN_WRAPPER="${USER_BIN_DIR}/visionclip-capture-explain"
CAPTURE_TRANSLATE_WRAPPER="${USER_BIN_DIR}/visionclip-capture-translate"
VOICE_SEARCH_WRAPPER="${USER_BIN_DIR}/visionclip-voice-search"
BOOK_READ_WRAPPER="${USER_BIN_DIR}/visionclip-book-read"
BOOK_TRANSLATE_READ_WRAPPER="${USER_BIN_DIR}/visionclip-book-translate-read"

write_wrapper "$VOICE_AGENT_WRAPPER" "visionclip voice agent" --voice-agent --speak
write_wrapper "$CAPTURE_EXPLAIN_WRAPPER" "visionclip capture explain" --action explain --speak
write_wrapper "$CAPTURE_TRANSLATE_WRAPPER" "visionclip capture translate" --action translate_ptbr --speak
write_wrapper "$VOICE_SEARCH_WRAPPER" "visionclip voice search" --voice-search --speak
write_wrapper "$BOOK_READ_WRAPPER" "visionclip book read voice mode" --voice-agent --speak
write_wrapper "$BOOK_TRANSLATE_READ_WRAPPER" "visionclip translated book read voice mode" --voice-agent --speak

RESOLVED_VOICE_AGENT_BINDING="$(normalize_binding "$VOICE_AGENT_BINDING")"
RESOLVED_CAPTURE_EXPLAIN_BINDING="$(normalize_binding "$CAPTURE_EXPLAIN_BINDING")"
RESOLVED_CAPTURE_TRANSLATE_BINDING="$(normalize_binding "$CAPTURE_TRANSLATE_BINDING")"
RESOLVED_VOICE_SEARCH_BINDING="$(normalize_binding "$VOICE_SEARCH_BINDING")"
RESOLVED_BOOK_READ_BINDING="$(normalize_binding "$BOOK_READ_BINDING")"
RESOLVED_BOOK_TRANSLATE_READ_BINDING="$(normalize_binding "$BOOK_TRANSLATE_READ_BINDING")"

clear_conflicting_gnome_bindings \
    "$RESOLVED_VOICE_AGENT_BINDING" \
    "$RESOLVED_CAPTURE_EXPLAIN_BINDING" \
    "$RESOLVED_CAPTURE_TRANSLATE_BINDING" \
    "$RESOLVED_VOICE_SEARCH_BINDING" \
    "$RESOLVED_BOOK_READ_BINDING" \
    "$RESOLVED_BOOK_TRANSLATE_READ_BINDING"

CURRENT_BINDINGS="$(gsettings get "$CUSTOM_KEYBINDINGS_SCHEMA" custom-keybindings)"
UPDATED_BINDINGS="$(filter_custom_keybinding_paths "$CURRENT_BINDINGS" "${OLD_KEYBINDING_PATHS[@]}")"
for path in "${SHORTCUT_PATHS[@]}"; do
    UPDATED_BINDINGS="$(append_custom_keybinding_path "$UPDATED_BINDINGS" "$path")"
done
gsettings set "$CUSTOM_KEYBINDINGS_SCHEMA" custom-keybindings "$UPDATED_BINDINGS"

configure_shortcut "${SHORTCUT_PATHS[0]}" "VisionClip Voice Agent" "$VOICE_AGENT_WRAPPER" "$RESOLVED_VOICE_AGENT_BINDING"
configure_shortcut "${SHORTCUT_PATHS[1]}" "VisionClip Explain Screen" "$CAPTURE_EXPLAIN_WRAPPER" "$RESOLVED_CAPTURE_EXPLAIN_BINDING"
configure_shortcut "${SHORTCUT_PATHS[2]}" "VisionClip Translate Screen" "$CAPTURE_TRANSLATE_WRAPPER" "$RESOLVED_CAPTURE_TRANSLATE_BINDING"
configure_shortcut "${SHORTCUT_PATHS[3]}" "VisionClip Voice Search" "$VOICE_SEARCH_WRAPPER" "$RESOLVED_VOICE_SEARCH_BINDING"
configure_shortcut "${SHORTCUT_PATHS[4]}" "VisionClip Book Read" "$BOOK_READ_WRAPPER" "$RESOLVED_BOOK_READ_BINDING"
configure_shortcut "${SHORTCUT_PATHS[5]}" "VisionClip Book Translate Read" "$BOOK_TRANSLATE_READ_WRAPPER" "$RESOLVED_BOOK_TRANSLATE_READ_BINDING"

if command -v systemctl >/dev/null 2>&1; then
    systemctl --user import-environment DISPLAY WAYLAND_DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_RUNTIME_DIR DBUS_SESSION_BUS_ADDRESS PATH >/dev/null 2>&1 || true
    systemctl --user restart org.gnome.SettingsDaemon.MediaKeys.target >/dev/null 2>&1 || \
        systemctl --user start org.gnome.SettingsDaemon.MediaKeys.target >/dev/null 2>&1 || true
fi

echo "Atalhos do VisionClip configurados."
echo "Voice agent: $RESOLVED_VOICE_AGENT_BINDING -> $VOICE_AGENT_WRAPPER"
echo "Explain screen: $RESOLVED_CAPTURE_EXPLAIN_BINDING -> $CAPTURE_EXPLAIN_WRAPPER"
echo "Translate screen: $RESOLVED_CAPTURE_TRANSLATE_BINDING -> $CAPTURE_TRANSLATE_WRAPPER"
echo "Voice search: $RESOLVED_VOICE_SEARCH_BINDING -> $VOICE_SEARCH_WRAPPER"
echo "Book read voice mode: $RESOLVED_BOOK_READ_BINDING -> $BOOK_READ_WRAPPER"
echo "Translated book read voice mode: $RESOLVED_BOOK_TRANSLATE_READ_BINDING -> $BOOK_TRANSLATE_READ_WRAPPER"
echo "Log: ${HOME}/.local/state/visionclip/voice-shortcut.log"
