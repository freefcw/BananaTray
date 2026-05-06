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

# DEBIAN/postinst — 安装后更新 desktop 数据库和图标缓存
cat > "$PKG_DIR/DEBIAN/postinst" <<'POSTINST_EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications || true
fi
if [ -d /usr/share/icons/hicolor ]; then
    touch --no-create /usr/share/icons/hicolor || true
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache /usr/share/icons/hicolor || true
    fi
fi
if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload >/dev/null 2>&1 || true
fi
POSTINST_EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

# DEBIAN/postrm — 卸载后清理
cat > "$PKG_DIR/DEBIAN/postrm" <<'POSTRM_EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database /usr/share/applications || true
fi
if [ -d /usr/share/icons/hicolor ]; then
    touch --no-create /usr/share/icons/hicolor || true
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache /usr/share/icons/hicolor || true
    fi
fi
if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload >/dev/null 2>&1 || true
fi
POSTRM_EOF
chmod 755 "$PKG_DIR/DEBIAN/postrm"

# 标准安装树 (usr/bin, desktop, icons, metainfo, resources)
assemble_install_tree "$PKG_DIR"

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
