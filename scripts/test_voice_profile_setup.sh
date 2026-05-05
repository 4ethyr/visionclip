#!/usr/bin/env bash
set -Eeuo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP_DIR="$(mktemp -d)"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

assert_contains() {
    local file="$1"
    local expected="$2"
    if ! grep -Fqx "$expected" "$file"; then
        echo "expected line not found: $expected" >&2
        echo "--- $file ---" >&2
        cat "$file" >&2
        exit 1
    fi
}

bash -n "$ROOT_DIR/scripts/setup_voice_profile.sh"

cat >"$TMP_DIR/config.toml" <<'EOF'
[general]
log_level = "info"

[voice]
enabled = false
wake_word_enabled = false
transcribe_command = "demo"
EOF

VISIONCLIP_CONFIG="$TMP_DIR/config.toml" \
VISIONCLIP_CONFIG_DIR="$TMP_DIR" \
VISIONCLIP_DATA_DIR="$TMP_DIR/data" \
VISIONCLIP_BIN="/bin/true" \
    "$ROOT_DIR/scripts/setup_voice_profile.sh" \
        --yes \
        --enable-wake \
        --no-enroll \
        --no-start \
        --samples 4 \
        >"$TMP_DIR/setup.log"

assert_contains "$TMP_DIR/config.toml" "enabled = true"
assert_contains "$TMP_DIR/config.toml" "wake_word_enabled = true"
assert_contains "$TMP_DIR/config.toml" "wake_block_during_playback = true"
assert_contains "$TMP_DIR/config.toml" "speaker_verification_enabled = true"
assert_contains "$TMP_DIR/config.toml" "speaker_verification_threshold = 0.72"
assert_contains "$TMP_DIR/config.toml" "speaker_verification_min_samples = 3"
assert_contains "$TMP_DIR/config.toml" "overlay_enabled = false"
assert_contains "$TMP_DIR/config.toml" 'transcribe_command = "demo"'

if ! find "$TMP_DIR" -maxdepth 1 -name 'config.toml.bak.*' | grep -q .; then
    echo "expected config backup was not created" >&2
    exit 1
fi

echo "voice profile setup script test passed"
