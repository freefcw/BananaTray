#!/usr/bin/env bash
#
# Run the BananaTray GNOME Shell Extension GJS integration test under an
# isolated dbus-run-session.
#
# This validates quotaClient.js against a real GJS engine + real D-Bus bus,
# covering Gio.DBusProxy, signal subscription, and schema validation paths
# that Node mock tests cannot reach.
#
# Requirements: gjs, dbus-run-session (both available via apt on Ubuntu).
# If gjs is not installed, the script exits 0 with a skip notice (CI-friendly).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

EXT_DIR="$PROJECT_DIR/gnome-shell-extension"
TEST_FILE="$EXT_DIR/tests/gjs-quota-client-integration.test.js"
MOCK_DAEMON_FILE="$EXT_DIR/tests/gjs-mock-daemon.js"
I18N_STUB="$EXT_DIR/tests/gjs-i18n-stub.js"

if ! command -v gjs >/dev/null 2>&1; then
    echo "skip: gjs not found; GJS integration test requires 'apt install gjs'"
    exit 0
fi

if ! command -v dbus-run-session >/dev/null 2>&1; then
    echo "skip: dbus-run-session not found; GJS integration test requires 'dbus' package"
    exit 0
fi

# Prepare an isolated test tree so the i18n.js stub override does not touch
# the real extension source.
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/bananatray-gjs-test.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

# Copy the extension modules that quotaClient.js imports at runtime.
# quotaClient.js only depends on gi://Gio, gi://GLib and ./i18n.js — it does
# NOT import resource:///org/gnome/shell/..., so it loads in plain GJS.
mkdir -p "$tmp_dir/gnome-shell-extension/tests"
cp "$EXT_DIR/quotaClient.js" "$tmp_dir/gnome-shell-extension/quotaClient.js"
# Overwrite i18n.js with the GJS test stub (passthrough _/ngettext).
cp "$I18N_STUB" "$tmp_dir/gnome-shell-extension/i18n.js"
# Test driver + mock daemon live under tests/; the test imports ../quotaClient.js
# and ./gjs-mock-daemon.js.
cp "$TEST_FILE" "$tmp_dir/gnome-shell-extension/tests/gjs-quota-client-integration.test.js"
cp "$MOCK_DAEMON_FILE" "$tmp_dir/gnome-shell-extension/tests/gjs-mock-daemon.js"

echo "Running GJS integration test under dbus-run-session..."
echo "  GJS:        $(gjs --version 2>&1 || echo 'unknown')"
echo "  Test dir:   $tmp_dir"
echo

# dbus-run-session provides an isolated session bus; the test driver starts
# the mock daemon and the QuotaClient in the same GJS process so they share
# that bus.
# GIO_USE_VFS=local：阻止 GIO 自动激活 org.gtk.vfs.Daemon。quotaClient.js 只用
# D-Bus，不需要 gvfs；若不设，gvfsd 会在测试结束 session bus 拆除时打印
# "A connection to the bus can't be made"，干扰 CI 日志判读。
if dbus-run-session -- env GIO_USE_VFS=local gjs --module \
    "$tmp_dir/gnome-shell-extension/tests/gjs-quota-client-integration.test.js"; then
    echo
    echo "GJS integration test: PASSED"
    exit 0
else
    exit_code=$?
    echo
    echo "GJS integration test: FAILED (exit $exit_code)"
    exit "$exit_code"
fi
