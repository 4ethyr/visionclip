#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USER_SYSTEMD_DIR="${HOME}/.config/systemd/user"
CONFIG_DIR="${HOME}/.config/visionclip"
LEGACY_CONFIG_DIR="${HOME}/.config/ai-snap"
BIN_DIR="${HOME}/.local/bin"
VOICE_DIR="${ROOT_DIR}/tools/piper-voices"

mkdir -p "$USER_SYSTEMD_DIR" "$CONFIG_DIR" "$BIN_DIR"

cp "$ROOT_DIR/deploy/systemd/visionclip-daemon.service" "$USER_SYSTEMD_DIR/visionclip-daemon.service"
cp "$ROOT_DIR/deploy/systemd/piper-http.service" "$USER_SYSTEMD_DIR/piper-http.service"

cat >"$BIN_DIR/visionclip-voice-search" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "${HOME}/.local/state/visionclip"
printf '%s visionclip voice shortcut invoked\n' "$(date --iso-8601=seconds)" >>"${HOME}/.local/state/visionclip/voice-shortcut.log"
exec "${HOME}/.local/bin/visionclip" --voice-agent --speak "$@"
EOF
chmod +x "$BIN_DIR/visionclip-voice-search"

cat >"$BIN_DIR/visionclip-piper-http" <<EOF
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$ROOT_DIR"
VOICE_DIR="\${VISIONCLIP_PIPER_VOICE_DIR:-$VOICE_DIR}"
VOICE_PATH="\${VISIONCLIP_PIPER_VOICE:-}"
PIPER_PORT="\${VISIONCLIP_PIPER_PORT:-5000}"
PIPER_PYTHON="\${VISIONCLIP_PIPER_PYTHON:-}"

if [[ -z "\$PIPER_PYTHON" ]]; then
  if [[ -x "\$ROOT_DIR/venv/bin/python" ]]; then
    PIPER_PYTHON="\$ROOT_DIR/venv/bin/python"
  elif [[ -x "\$ROOT_DIR/.venv/bin/python" ]]; then
    PIPER_PYTHON="\$ROOT_DIR/.venv/bin/python"
  else
    PIPER_PYTHON="/usr/bin/python3"
  fi
fi

if [[ -z "\$VOICE_PATH" ]]; then
  if [[ -f "\$VOICE_DIR/pt_BR-faber-medium.onnx" ]]; then
    VOICE_PATH="\$VOICE_DIR/pt_BR-faber-medium.onnx"
  fi
fi

if [[ -z "\$VOICE_PATH" ]]; then
  if [[ -d "\$VOICE_DIR" ]]; then
    mapfile -t voices < <(find "\$VOICE_DIR" -maxdepth 1 -type f -name '*.onnx' | sort)
    if [[ "\${#voices[@]}" -gt 0 ]]; then
      VOICE_PATH="\${voices[0]}"
    fi
  fi
fi

if [[ -z "\$VOICE_PATH" ]]; then
  echo "Nenhuma voz Piper encontrada. Defina VISIONCLIP_PIPER_VOICE ou adicione uma voz em \$VOICE_DIR." >&2
  exit 1
fi

exec "\$PIPER_PYTHON" -m piper.http_server \
  -m "\$VOICE_PATH" \
  --data-dir "\$VOICE_DIR" \
  --download-dir "\$VOICE_DIR" \
  --host 127.0.0.1 \
  --port "\$PIPER_PORT"
EOF
chmod +x "$BIN_DIR/visionclip-piper-http"

if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
  if [[ -f "$LEGACY_CONFIG_DIR/config.toml" ]]; then
    cp "$LEGACY_CONFIG_DIR/config.toml" "$CONFIG_DIR/config.toml"
    echo "Config legada migrada de $LEGACY_CONFIG_DIR/config.toml"
  else
    cp "$ROOT_DIR/examples/config.toml" "$CONFIG_DIR/config.toml"
  fi
fi

echo "Copie os binários compilados para: $BIN_DIR"
echo "Depois rode: systemctl --user daemon-reload"
echo "Depois rode: systemctl --user enable --now visionclip-daemon.service"
echo "Se quiser o TTS em systemd: systemctl --user enable --now piper-http.service"
echo "Para instalar o atalho de voz no GNOME: $ROOT_DIR/scripts/install_gnome_voice_shortcut.sh"

if [[ -f "$USER_SYSTEMD_DIR/ai-daemon.service" ]]; then
  echo "Serviço legado detectado: $USER_SYSTEMD_DIR/ai-daemon.service"
  echo "Revise se deseja desabilitar o serviço antigo com: systemctl --user disable --now ai-daemon.service"
fi
