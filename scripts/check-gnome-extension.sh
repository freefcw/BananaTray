#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

EXT_DIR="gnome-shell-extension"
required_files=(
  "metadata.json"
  "extension.js"
  "quotaClient.js"
  "stylesheet.css"
  "icons/bananatray-symbolic.svg"
)

for file in "${required_files[@]}"; do
  if [[ ! -f "$EXT_DIR/$file" ]]; then
    echo "error: missing GNOME Shell Extension file: $EXT_DIR/$file" >&2
    exit 1
  fi
done

if ! command -v node >/dev/null 2>&1; then
  echo "node not found; skipping GNOME Shell Extension syntax check"
else
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/bananatray-gjs-check.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT

  cp "$EXT_DIR/extension.js" "$tmp_dir/extension.mjs"
  cp "$EXT_DIR/quotaClient.js" "$tmp_dir/quotaClient.js"
  cp scripts/gnome-extension-mock-daemon.js "$tmp_dir/gnome-extension-mock-daemon.mjs"

  node --check "$tmp_dir/extension.mjs"
  node --check "$tmp_dir/quotaClient.js"
  node --check "$tmp_dir/gnome-extension-mock-daemon.mjs"
fi

if command -v rg >/dev/null 2>&1; then
  sync_matches=$(rg -n 'RemoteSync|GetAllQuotasSync|RefreshAllSync|OpenSettingsSync' "$EXT_DIR" scripts/gnome-extension-mock-daemon.js || true)
  import_matches=$(rg -n "from './quotaClient\\.js';" "$EXT_DIR/extension.js" || true)
  schema_matches=$(rg -n 'schema_version' "$EXT_DIR/quotaClient.js" scripts/gnome-extension-mock-daemon.js src/application/selectors/dbus_dto.rs || true)
else
  sync_matches=$(grep -RInE 'RemoteSync|GetAllQuotasSync|RefreshAllSync|OpenSettingsSync' "$EXT_DIR" scripts/gnome-extension-mock-daemon.js || true)
  import_matches=$(grep -n "from './quotaClient\\.js';" "$EXT_DIR/extension.js" || true)
  schema_matches=$(grep -RIn 'schema_version' "$EXT_DIR/quotaClient.js" scripts/gnome-extension-mock-daemon.js src/application/selectors/dbus_dto.rs || true)
fi

if [[ -n "$sync_matches" ]]; then
  echo "$sync_matches"
  echo
  echo "error: synchronous D-Bus calls are forbidden in the GNOME Shell Extension"
  exit 1
fi

if [[ -z "$import_matches" ]]; then
  echo "error: extension.js must import ./quotaClient.js" >&2
  exit 1
fi

if [[ -z "$schema_matches" ]]; then
  echo "error: schema_version must be present in Rust DTO, quotaClient.js, and mock daemon" >&2
  exit 1
fi

echo "GNOME Shell Extension check passed"
