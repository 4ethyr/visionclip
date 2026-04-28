#!/usr/bin/env bash
set -euo pipefail

if ! git rev-parse --show-toplevel >/dev/null 2>&1; then
  echo "guard_no_secrets: not inside a git repository" >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

patterns=(
  'sk-or-v1-[A-Za-z0-9_-]{20,}'
  'OPENROUTER_API_KEY[[:space:]]*='
)

files=()
while IFS= read -r -d '' file; do
  files+=("$file")
done < <(
  {
    git diff --cached --name-only -z
    git ls-files -z
  } | sort -zu
)

if ((${#files[@]} == 0)); then
  exit 0
fi

existing_files=()
for file in "${files[@]}"; do
  if [[ -f "$file" ]]; then
    existing_files+=("$file")
  fi
done

if ((${#existing_files[@]} == 0)); then
  exit 0
fi

for pattern in "${patterns[@]}"; do
  if grep -I -E -n -- "$pattern" "${existing_files[@]}" >/dev/null; then
    echo "guard_no_secrets: potential secret detected in tracked or staged files" >&2
    echo "Refusing to continue. Move secrets to ignored local files or environment variables." >&2
    exit 1
  fi
done
