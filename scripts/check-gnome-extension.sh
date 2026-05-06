#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

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

if ! command -v node >/dev/null 2>&1; then
  echo "node not found; skipping GNOME Shell Extension syntax check"
else
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
fi

if ! command -v msgfmt >/dev/null 2>&1; then
  echo "msgfmt not found; skipping GNOME Shell Extension translation check"
else
  tmp_mo="$(mktemp "${TMPDIR:-/tmp}/bananatray-i18n.XXXXXX.mo")"
  msgfmt --check --output-file="$tmp_mo" "$EXT_DIR/po/zh_CN.po"
  if ! cmp -s "$tmp_mo" "$EXT_DIR/locale/zh_CN/LC_MESSAGES/bananatray.mo"; then
    echo "error: compiled translation is stale: run msgfmt --check --output-file=$EXT_DIR/locale/zh_CN/LC_MESSAGES/bananatray.mo $EXT_DIR/po/zh_CN.po" >&2
    exit 1
  fi
  rm -f "$tmp_mo"
fi

if ! command -v xgettext >/dev/null 2>&1; then
  echo "xgettext not found; skipping GNOME Shell Extension gettext coverage check"
elif ! command -v msgcmp >/dev/null 2>&1; then
  echo "msgcmp not found; skipping GNOME Shell Extension gettext coverage check"
else
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
  schema_matches=$(rg -n 'schema_version' "$EXT_DIR/quotaClient.js" scripts/gnome-extension-mock-daemon.js src/application/selectors/dbus_dto.rs || true)
  activation_matches=$(rg -n 'StartServiceByName' "$EXT_DIR/quotaClient.js" || true)
  activation_template_matches=$(rg -n '@BANANATRAY_EXEC@' resources/linux/com.bananatray.Daemon.service resources/linux/bananatray.service || true)
  appimage_removal_matches=$(rg -nF 'remove_activation_files "$APPDIR/usr"' scripts/bundle-appimage.sh || true)
  daemon_reload_matches=$(rg -n 'systemctl --user daemon-reload' scripts/bundle-deb.sh scripts/bundle-rpm.sh || true)
else
  sync_matches=$(grep -RInE 'RemoteSync|GetAllQuotasSync|RefreshAllSync|OpenSettingsSync' "$EXT_DIR" scripts/gnome-extension-mock-daemon.js || true)
  entry_import_matches=$(grep -n "from './panelButton\\.js';" "$EXT_DIR/extension.js" || true)
  i18n_matches=$(grep -n '"gettext-domain": "bananatray"' "$EXT_DIR/metadata.json" || true)
  client_import_matches=$(grep -n "from './quotaClient\\.js';" "$EXT_DIR/panelButton.js" || true)
  schema_matches=$(grep -RIn 'schema_version' "$EXT_DIR/quotaClient.js" scripts/gnome-extension-mock-daemon.js src/application/selectors/dbus_dto.rs || true)
  activation_matches=$(grep -n 'StartServiceByName' "$EXT_DIR/quotaClient.js" || true)
  activation_template_matches=$(grep -RIn '@BANANATRAY_EXEC@' resources/linux/com.bananatray.Daemon.service resources/linux/bananatray.service || true)
  appimage_removal_matches=$(grep -nF 'remove_activation_files "$APPDIR/usr"' scripts/bundle-appimage.sh || true)
  daemon_reload_matches=$(grep -RIn 'systemctl --user daemon-reload' scripts/bundle-deb.sh scripts/bundle-rpm.sh || true)
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

if [[ -z "$schema_matches" ]]; then
  echo "error: schema_version must be present in Rust DTO, quotaClient.js, and mock daemon" >&2
  exit 1
fi

if [[ -z "$activation_matches" ]]; then
  echo "error: quotaClient.js must request D-Bus activation with StartServiceByName" >&2
  exit 1
fi

if [[ -z "$activation_template_matches" ]]; then
  echo "error: activation templates must contain @BANANATRAY_EXEC@ for install-time path substitution" >&2
  exit 1
fi

if [[ -z "$appimage_removal_matches" ]]; then
  echo "error: AppImage bundling must remove host D-Bus activation files from AppDir" >&2
  exit 1
fi

if [[ -z "$daemon_reload_matches" ]]; then
  echo "error: deb/rpm packaging scripts must run systemctl --user daemon-reload after systemd user unit changes" >&2
  exit 1
fi

echo "GNOME Shell Extension check passed"
