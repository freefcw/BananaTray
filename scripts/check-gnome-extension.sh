#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

file_contains_pattern() {
  local file="$1"
  local pattern="$2"

  if command -v rg >/dev/null 2>&1; then
    rg -q -- "$pattern" "$file"
  else
    grep -Eq -- "$pattern" "$file"
  fi
}

require_file_pattern() {
  local root_dir="$1"
  local relative_path="$2"
  local pattern="$3"
  local contract="$4"

  if ! file_contains_pattern "$root_dir/$relative_path" "$pattern"; then
    echo "error: $relative_path must contain $contract" >&2
    return 1
  fi
}

SKIPPED_CHECKS=()

# 工具缺失处理：GNOME_CHECK_STRICT=1 时报错退出（CI 门禁），
# 否则记录到 SKIPPED_CHECKS 并在结尾汇总提示，
# 避免工具缺失导致核心校验被跳过后仍输出 "passed" 的假阳性。
tool_available_or_skip() {
  local tool="$1"
  local check_name="$2"

  if command -v "$tool" >/dev/null 2>&1; then
    return 0
  fi
  if [[ "${GNOME_CHECK_STRICT:-0}" == "1" ]]; then
    echo "error: $tool not found; required for: $check_name (GNOME_CHECK_STRICT=1)" >&2
    exit 1
  fi
  echo "warning: $tool not found; skipping $check_name" >&2
  SKIPPED_CHECKS+=("$check_name")
  return 1
}

check_gnome_packaging_contracts() {
  local root_dir="${1:?project root is required}"

  require_file_pattern "$root_dir" \
    "gnome-shell-extension/quotaClient.js" \
    'schema_version' \
    "the schema_version contract" || return 1
  require_file_pattern "$root_dir" \
    "scripts/gnome-extension-mock-daemon.js" \
    'schema_version' \
    "the schema_version contract" || return 1
  require_file_pattern "$root_dir" \
    "src/application/selectors/dbus_dto.rs" \
    'schema_version' \
    "the schema_version contract" || return 1

  require_file_pattern "$root_dir" \
    "resources/linux/com.bananatray.Daemon.service" \
    '@BANANATRAY_EXEC@' \
    "the @BANANATRAY_EXEC@ install-time placeholder" || return 1
  require_file_pattern "$root_dir" \
    "resources/linux/bananatray.service" \
    '@BANANATRAY_EXEC@' \
    "the @BANANATRAY_EXEC@ install-time placeholder" || return 1

  require_file_pattern "$root_dir" \
    "scripts/bundle-deb.sh" \
    'systemctl[[:space:]]+--user[[:space:]]+daemon-reload' \
    "systemctl --user daemon-reload" || return 1
  require_file_pattern "$root_dir" \
    "scripts/bundle-rpm.sh" \
    'systemctl[[:space:]]+--user[[:space:]]+daemon-reload' \
    "systemctl --user daemon-reload" || return 1
}

# 允许负例测试复用上面的逐文件契约检查，而不执行完整 GNOME 门禁。
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
  return 0
fi

cd "$PROJECT_DIR"

EXT_DIR="gnome-shell-extension"
required_files=(
  "metadata.json"
  "extension.js"
  "i18n.js"
  "panelButton.js"
  "quotaClient.js"
  "quotaPresentation.js"
  "quotaWidgets.js"
  "po/zh_CN.po"
  "locale/zh_CN/LC_MESSAGES/bananatray.mo"
  "stylesheet.css"
  "icons/bananatray-symbolic.svg"
)
required_activation_files=(
  "resources/linux/com.bananatray.Daemon.service"
  "resources/linux/bananatray.service"
)

for file in "${required_files[@]}"; do
  if [[ ! -f "$EXT_DIR/$file" ]]; then
    echo "error: missing GNOME Shell Extension file: $EXT_DIR/$file" >&2
    exit 1
  fi
done

for file in "${required_activation_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "error: missing GNOME D-Bus activation file: $file" >&2
    exit 1
  fi
done

if tool_available_or_skip node "GNOME Shell Extension syntax/contract/unit checks (node)"; then
  tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/bananatray-gjs-check.XXXXXX")"
  trap 'rm -rf "$tmp_dir"' EXIT

  cp "$EXT_DIR/extension.js" "$tmp_dir/extension.mjs"
  cp "$EXT_DIR/i18n.js" "$tmp_dir/i18n.js"
  cp "$EXT_DIR/panelButton.js" "$tmp_dir/panelButton.js"
  cp "$EXT_DIR/quotaClient.js" "$tmp_dir/quotaClient.js"
  cp "$EXT_DIR/quotaPresentation.js" "$tmp_dir/quotaPresentation.js"
  cp "$EXT_DIR/quotaWidgets.js" "$tmp_dir/quotaWidgets.js"
  cp scripts/gnome-extension-mock-daemon.js "$tmp_dir/gnome-extension-mock-daemon.mjs"

  node --check "$tmp_dir/extension.mjs"
  node --check "$tmp_dir/i18n.js"
  node --check "$tmp_dir/panelButton.js"
  node --check "$tmp_dir/quotaClient.js"
  node --check "$tmp_dir/quotaPresentation.js"
  node --check "$tmp_dir/quotaWidgets.js"
  node --check "$tmp_dir/gnome-extension-mock-daemon.mjs"
  node scripts/check-gnome-dbus-contract.mjs

  # Unit tests for pure presentation functions
  node --import ./gnome-shell-extension/tests/register.mjs \
       --test ./gnome-shell-extension/tests/*.test.mjs
fi

if tool_available_or_skip msgfmt "GNOME Shell Extension translation check (msgfmt)"; then
  tmp_mo="$(mktemp "${TMPDIR:-/tmp}/bananatray-i18n.XXXXXX.mo")"
  msgfmt --check --output-file="$tmp_mo" "$EXT_DIR/po/zh_CN.po"
  if ! cmp -s "$tmp_mo" "$EXT_DIR/locale/zh_CN/LC_MESSAGES/bananatray.mo"; then
    echo "error: compiled translation is stale: run msgfmt --check --output-file=$EXT_DIR/locale/zh_CN/LC_MESSAGES/bananatray.mo $EXT_DIR/po/zh_CN.po" >&2
    exit 1
  fi
  rm -f "$tmp_mo"
fi

if tool_available_or_skip xgettext "GNOME Shell Extension gettext coverage check (xgettext+msgcmp)" \
  && tool_available_or_skip msgcmp "GNOME Shell Extension gettext coverage check (xgettext+msgcmp)"; then
  tmp_pot="$(mktemp "${TMPDIR:-/tmp}/bananatray-i18n.XXXXXX.pot")"
  tmp_metadata_js="$(mktemp "${TMPDIR:-/tmp}/bananatray-i18n-metadata.XXXXXX.js")"
  description="$(sed -n 's/^  "description": "\(.*\)",$/\1/p' "$EXT_DIR/metadata.json")"
  printf "_('%s');\n" "$description" > "$tmp_metadata_js"
  xgettext \
    --language=JavaScript \
    --from-code=UTF-8 \
    --keyword=_ \
    --keyword=ngettext:1,2 \
    --add-comments=Translators: \
    --output="$tmp_pot" \
    "$EXT_DIR"/*.js \
    "$tmp_metadata_js"
  if ! msgcmp --no-fuzzy-matching "$EXT_DIR/po/zh_CN.po" "$tmp_pot"; then
    echo "error: gettext strings and $EXT_DIR/po/zh_CN.po are out of sync" >&2
    rm -f "$tmp_pot" "$tmp_metadata_js"
    exit 1
  fi
  rm -f "$tmp_pot" "$tmp_metadata_js"
fi

if command -v rg >/dev/null 2>&1; then
  sync_matches=$(rg -n 'RemoteSync|GetAllQuotasSync|RefreshAllSync|OpenSettingsSync' "$EXT_DIR" scripts/gnome-extension-mock-daemon.js || true)
  entry_import_matches=$(rg -n "from './panelButton\\.js';" "$EXT_DIR/extension.js" || true)
  i18n_matches=$(rg -n '"gettext-domain": "bananatray"' "$EXT_DIR/metadata.json" || true)
  client_import_matches=$(rg -n "from './quotaClient\\.js';" "$EXT_DIR/panelButton.js" || true)
  activation_matches=$(rg -n 'StartServiceByName' "$EXT_DIR/quotaClient.js" || true)
  appimage_removal_matches=$(rg -nF 'remove_activation_files "$APPDIR/usr"' scripts/bundle-appimage.sh || true)
else
  sync_matches=$(grep -RInE 'RemoteSync|GetAllQuotasSync|RefreshAllSync|OpenSettingsSync' "$EXT_DIR" scripts/gnome-extension-mock-daemon.js || true)
  entry_import_matches=$(grep -n "from './panelButton\\.js';" "$EXT_DIR/extension.js" || true)
  i18n_matches=$(grep -n '"gettext-domain": "bananatray"' "$EXT_DIR/metadata.json" || true)
  client_import_matches=$(grep -n "from './quotaClient\\.js';" "$EXT_DIR/panelButton.js" || true)
  activation_matches=$(grep -n 'StartServiceByName' "$EXT_DIR/quotaClient.js" || true)
  appimage_removal_matches=$(grep -nF 'remove_activation_files "$APPDIR/usr"' scripts/bundle-appimage.sh || true)
fi

if [[ -n "$sync_matches" ]]; then
  echo "$sync_matches"
  echo
  echo "error: synchronous D-Bus calls are forbidden in the GNOME Shell Extension"
  exit 1
fi

if [[ -z "$entry_import_matches" ]]; then
  echo "error: extension.js must import ./panelButton.js" >&2
  exit 1
fi

if [[ -z "$i18n_matches" ]]; then
  echo "error: metadata.json must declare gettext-domain \"bananatray\"" >&2
  exit 1
fi

if [[ -z "$client_import_matches" ]]; then
  echo "error: panelButton.js must import ./quotaClient.js" >&2
  exit 1
fi

if [[ -z "$activation_matches" ]]; then
  echo "error: quotaClient.js must request D-Bus activation with StartServiceByName" >&2
  exit 1
fi

if [[ -z "$appimage_removal_matches" ]]; then
  echo "error: AppImage bundling must remove host D-Bus activation files from AppDir" >&2
  exit 1
fi

check_gnome_packaging_contracts "$PROJECT_DIR"

# GJS 真实 D-Bus 集成测试（需要 gjs + dbus-run-session）。
# 环境未安装 gjs 时默认 skip 并记录（GNOME_CHECK_STRICT=1 时转为失败）。
# ci.yml 会安装 gjs + dbus 并开启 STRICT，使本步骤在 CI 上总是执行该集成测试。
if tool_available_or_skip gjs "GJS D-Bus integration test (gjs+dbus-run-session)" \
  && tool_available_or_skip dbus-run-session "GJS D-Bus integration test (gjs+dbus-run-session)"; then
  bash "$SCRIPT_DIR/test-gnome-extension-gjs.sh"
fi

if ((${#SKIPPED_CHECKS[@]})); then
  echo "warning: the following checks were skipped (missing tools):" >&2
  printf '  - %s\n' "${SKIPPED_CHECKS[@]}" >&2
  echo "GNOME Shell Extension check passed (${#SKIPPED_CHECKS[@]} check(s) skipped)"
else
  echo "GNOME Shell Extension check passed"
fi
