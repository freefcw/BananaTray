#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# Secret/token previews must not slice UTF-8 strings with byte indexes.
# Use providers::common::secret::mask_secret_preview instead.
pattern='&?[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*\[[[:space:]]*(\.\.[[:space:]]*[0-9]+|[0-9]+[[:space:]]*\.\.[[:space:]]*[0-9]*|[A-Za-z_][A-Za-z0-9_]*\.len\(\)[[:space:]]*-[[:space:]]*[0-9]+[[:space:]]*\.\.)[[:space:]]*\]'

if command -v rg >/dev/null 2>&1; then
  matches=$(rg -n --type rust "$pattern" src/providers --no-heading || true)
else
  matches=$(grep -REn "$pattern" src/providers --include='*.rs' || true)
fi

if [ -n "$matches" ]; then
  echo "$matches"
  echo
  echo "error: forbidden direct byte slicing in provider secret/token previews"
  echo "use providers::common::secret::mask_secret_preview for masked previews"
  exit 1
fi

echo "Provider secret byte-slicing check passed"
