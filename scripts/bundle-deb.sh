#!/usr/bin/env bash
#
# BananaTray Ubuntu .deb 打包脚本
#
# 使用方法:
#   bash scripts/bundle-deb.sh                  # 默认 release 构建
#   bash scripts/bundle-deb.sh --skip-build      # 跳过编译
#   bash scripts/bundle-deb.sh --arch arm64      # 指定架构
#
# 前置要求:
#   - dpkg-deb (Ubuntu/Debian 自带)
#
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

init_project_vars
parse_args "$@"
ensure_build

MAINTAINER="BananaTray Team <bananatray@example.com>"
DESCRIPTION="System tray application for monitoring AI coding assistant quotas"

PKG_NAME="${APP_NAME}_${VERSION}_${ARCH}"
PKG_DIR="$BUNDLE_DIR/$PKG_NAME"

# ------------------------------------------------------------------
# 1. 组装 .deb 目录结构
# ------------------------------------------------------------------
echo "📦 组装 .deb 包结构..."
rm -rf "$PKG_DIR"

# DEBIAN/control
mkdir -p "$PKG_DIR/DEBIAN"
cat > "$PKG_DIR/DEBIAN/control" <<EOF
Package: $APP_NAME
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: $MAINTAINER
Description: $DESCRIPTION
 BananaTray is a cross-platform system tray application for monitoring
 AI coding assistant quotas. It supports multiple AI providers including
 Claude, Gemini, GitHub Copilot, Codex, and more.
Depends: libgtk-3-0, libayatana-appindicator3-1
EOF

# 二进制
mkdir -p "$PKG_DIR/usr/bin"
cp "$BINARY" "$PKG_DIR/usr/bin/$APP_NAME"
chmod 755 "$PKG_DIR/usr/bin/$APP_NAME"

# .desktop 启动器
mkdir -p "$PKG_DIR/usr/share/applications"
cp "$PROJECT_DIR/resources/linux/bananatray.desktop" \
   "$PKG_DIR/usr/share/applications/$APP_NAME.desktop"

# 图标 (多尺寸)
ICON_SRC="$PROJECT_DIR/src/tray_icon.png"
if [ -f "$ICON_SRC" ]; then
    for size in 16 32 48 64 128 256; do
        ICON_DIR="$PKG_DIR/usr/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$ICON_DIR"
        if command -v convert &>/dev/null; then
            convert "$ICON_SRC" -resize "${size}x${size}" "$ICON_DIR/$APP_NAME.png"
        elif command -v sips &>/dev/null; then
            sips -z "$size" "$size" "$ICON_SRC" --out "$ICON_DIR/$APP_NAME.png" >/dev/null 2>&1
        else
            cp "$ICON_SRC" "$ICON_DIR/$APP_NAME.png"
        fi
    done
fi

# 运行时资源
copy_runtime_resources "$PKG_DIR/usr/share/$APP_NAME"

# ------------------------------------------------------------------
# 2. 构建 .deb 包
# ------------------------------------------------------------------
echo "📦 构建 .deb 包..."
DEB_FILE="$BUNDLE_DIR/${PKG_NAME}.deb"

if command -v dpkg-deb &>/dev/null; then
    dpkg-deb --build --root-owner-group "$PKG_DIR" "$DEB_FILE"
    echo "✅ .deb 包已创建: $DEB_FILE"
    echo ""
    echo "📋 包信息:"
    dpkg-deb --info "$DEB_FILE" 2>/dev/null || true
    echo ""
    echo "🚀 安装: sudo apt install ./$DEB_FILE"
else
    echo "⚠️  dpkg-deb 不可用（当前非 Debian/Ubuntu 系统）"
    echo "   目录结构已组装: $PKG_DIR"
    echo "   请在 Ubuntu 上运行: dpkg-deb --build $PKG_DIR $DEB_FILE"
fi
