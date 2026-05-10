#!/usr/bin/env bash
#
# BananaTray GNOME Shell Extension — 热重载开发脚本
#
# 监控 gnome-shell-extension/ 目录文件变化，自动复制到用户扩展目录，
# 并通过 gnome-extensions disable/enable 触发模块重新加载。
# GNOME 45+ ESM 扩展在 disable/enable 周期会完全卸载并重新导入模块。
#
# 用法:
#   bash scripts/dev-gnome-extension-watch.sh              # 使用 inotifywait 或 fswatch
#   bash scripts/dev-gnome-extension-watch.sh --debounce 2 # 设置去抖秒数（默认 1）
#   bash scripts/dev-gnome-extension-watch.sh --once        # 只同步一次，不持续监控
#
# 适用场景:
#   - 在真实桌面会话上迭代 JS/CSS，不需要注销/重新登录
#   - Wayland 主会话没有 Alt+F2 → r，也无需 nested shell
#
# 注意:
#   - X11 上非常可靠
#   - Wayland 上 GNOME 45+ 大部分版本支持 disable/enable 热重载
#   - 如果某些 GNOME 版本 disable/enable 未生效，仍需注销重登
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

UUID="bananatray@bananatray.github.io"
SRC="$PROJECT_DIR/gnome-shell-extension"
DEST="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"
DEBOUNCE=1
ONCE=false

usage() {
    cat <<'EOF'
Usage: bash scripts/dev-gnome-extension-watch.sh [OPTIONS]

Options:
  --debounce SECONDS  Debounce interval between reloads (default: 1)
  --once              Copy and reload once, then exit (no file watching)
  -h, --help          Show this help

Requires inotifywait (inotify-tools) or fswatch for continuous watching.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debounce)
            DEBOUNCE="${2:?--debounce requires a number}"
            shift 2
            ;;
        --once)
            ONCE=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

reload() {
    echo "[$(date +%T)] Copying extension files → $DEST"
    mkdir -p "$DEST"
    cp -a "$SRC/." "$DEST/"

    echo "[$(date +%T)] Reloading extension (disable → enable)..."
    gnome-extensions disable "$UUID" 2>/dev/null || true
    sleep 0.3
    gnome-extensions enable "$UUID"

    # 验证加载状态
    local state
    state=$(gnome-extensions info "$UUID" 2>/dev/null | grep -oP '(?<=State: )\S+' || echo "UNKNOWN")
    if [[ "$state" == "ACTIVE" ]]; then
        echo "[$(date +%T)] ✅ Extension reloaded (State: ACTIVE)"
    else
        echo "[$(date +%T)] ⚠️  Extension state: $state — 如果非 ACTIVE，可能需要注销重登"
    fi
    echo ""
}

# Wayland 检测提示
if [[ "${XDG_SESSION_TYPE:-}" == "wayland" ]]; then
    echo "ℹ️  Wayland 会话检测到。disable/enable 热重载在大部分 GNOME 45+ 版本上有效。"
    echo "   如果重载后扩展未更新，请注销并重新登录。"
    echo ""
fi

# 初始安装
reload

if [[ "$ONCE" == "true" ]]; then
    echo "Done (--once mode)."
    exit 0
fi

# 持续监控
echo "👀 Watching $SRC for changes (debounce: ${DEBOUNCE}s)..."
echo "   Press Ctrl+C to stop."
echo ""

if command -v inotifywait >/dev/null 2>&1; then
    while inotifywait -r -e modify,create,delete,move "$SRC" \
        --exclude '\.git|__pycache__' \
        --quiet --timeout 0; do
        sleep "$DEBOUNCE"
        reload
    done
elif command -v fswatch >/dev/null 2>&1; then
    fswatch --latency "$DEBOUNCE" -o "$SRC" \
        --exclude '\.git' --exclude '__pycache__' | while read -r _; do
        reload
    done
else
    echo "❌ 需要安装 inotify-tools 或 fswatch 才能持续监控文件变化。" >&2
    echo "" >&2
    echo "   Ubuntu/Debian:  sudo apt install inotify-tools" >&2
    echo "   Fedora:         sudo dnf install inotify-tools" >&2
    echo "   macOS:          brew install fswatch" >&2
    echo "" >&2
    echo "   或者使用 --once 模式进行单次同步。" >&2
    exit 1
fi
