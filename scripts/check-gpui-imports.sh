#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# 优先使用 rg，回退到 grep
if command -v rg >/dev/null 2>&1; then
  # --type rust 仅匹配 .rs 文件；--no-heading 便于阅读
  matches=$(rg -n --type rust '^\s*use gpui::\*;' src --no-heading || true)
else
  matches=$(grep -rn '^\s*use gpui::\*;' src --include='*.rs' || true)
fi

if [ -n "$matches" ]; then
  echo "$matches"
  echo
  echo "error: forbidden GPUI glob import found in src/"
  echo "use explicit gpui imports instead"
  exit 1
fi

echo "GPUI glob import check passed"
