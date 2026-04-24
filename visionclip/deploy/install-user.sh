#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USER_SYSTEMD_DIR="${HOME}/.config/systemd/user"
CONFIG_DIR="${HOME}/.config/visionclip"
LEGACY_CONFIG_DIR="${HOME}/.config/ai-snap"
BIN_DIR="${HOME}/.local/bin"

mkdir -p "$USER_SYSTEMD_DIR" "$CONFIG_DIR" "$BIN_DIR"

cp "$ROOT_DIR/deploy/systemd/visionclip-daemon.service" "$USER_SYSTEMD_DIR/visionclip-daemon.service"
cp "$ROOT_DIR/deploy/systemd/piper-http.service" "$USER_SYSTEMD_DIR/piper-http.service"

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

if [[ -f "$USER_SYSTEMD_DIR/ai-daemon.service" ]]; then
  echo "Serviço legado detectado: $USER_SYSTEMD_DIR/ai-daemon.service"
  echo "Revise se deseja desabilitar o serviço antigo com: systemctl --user disable --now ai-daemon.service"
fi
