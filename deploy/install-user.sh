#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cat >&2 <<'EOF'
[visionclip] deploy/install-user.sh is kept for compatibility.
[visionclip] The supported installer is scripts/install_visionclip.sh.
EOF

exec "$ROOT_DIR/scripts/install_visionclip.sh" "$@"
