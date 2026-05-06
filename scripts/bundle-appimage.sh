#!/usr/bin/env bash
#
# BananaTray AppImage 打包脚本
#
# 使用方法:
#   bash scripts/bundle-appimage.sh              # 默认 release 构建
#   bash scripts/bundle-appimage.sh --skip-build  # 跳过编译
#
# 前置要求:
#   - appimagetool (自动下载或手动安装)
#     https://github.com/AppImage/appimagetool/releases
#
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

init_project_vars
parse_args "$@"
ensure_build

APP_DISPLAY_NAME="BananaTray"
APPDIR="$BUNDLE_DIR/$APP_DISPLAY_NAME.AppDir"

# ------------------------------------------------------------------
# 1. 确保 appimagetool 可用
# ------------------------------------------------------------------
APPIMAGETOOL=""
if command -v appimagetool &>/dev/null; then
    APPIMAGETOOL="appimagetool"
else
    APPIMAGETOOL_PATH="$BUNDLE_DIR/appimagetool"
    if [ ! -f "$APPIMAGETOOL_PATH" ]; then
        echo "📥 下载 appimagetool..."
        ARCH_SUFFIX="$(uname -m)"
        DOWNLOAD_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH_SUFFIX}.AppImage"
        mkdir -p "$BUNDLE_DIR"
        curl -fSL "$DOWNLOAD_URL" -o "$APPIMAGETOOL_PATH" 2>/dev/null || {
            echo "❌ 下载 appimagetool 失败"
            echo "   请手动安装: https://github.com/AppImage/appimagetool/releases"
            exit 1
        }
        chmod +x "$APPIMAGETOOL_PATH"
    fi
    APPIMAGETOOL="$APPIMAGETOOL_PATH"
fi

# ------------------------------------------------------------------
# 2. 组装 AppDir
# ------------------------------------------------------------------
echo "📦 组装 AppDir..."
rm -rf "$APPDIR"

# 标准安装树 (usr/bin, desktop, icons, metainfo, resources)
assemble_install_tree "$APPDIR"
remove_activation_files "$APPDIR/usr"

# AppImage 额外要求：顶层 .desktop 和 .png（appimagetool 规范）
cp "$PROJECT_DIR/resources/linux/bananatray.desktop" "$APPDIR/$APP_NAME.desktop"
ICON_SRC="$PROJECT_DIR/src/tray/tray_icon.png"
if [ -f "$ICON_SRC" ]; then
    cp "$ICON_SRC" "$APPDIR/$APP_NAME.png"
fi

# AppRun 启动脚本
cat > "$APPDIR/AppRun" <<'APPRUN_EOF'
#!/bin/bash
SELF_DIR="$(dirname "$(readlink -f "$0")")"
export PATH="$SELF_DIR/usr/bin:$PATH"
export BANANATRAY_RESOURCES="$SELF_DIR/usr/share/bananatray"
exec "$SELF_DIR/usr/bin/bananatray" "$@"
APPRUN_EOF
chmod +x "$APPDIR/AppRun"

# ------------------------------------------------------------------
# 3. 构建 AppImage
# ------------------------------------------------------------------
echo "📦 构建 AppImage..."
APPIMAGE_FILE="$BUNDLE_DIR/${APP_DISPLAY_NAME}-${VERSION}-$(uname -m).AppImage"

if [ -n "$APPIMAGETOOL" ]; then
    ARCH="$(uname -m)" "$APPIMAGETOOL" "$APPDIR" "$APPIMAGE_FILE" 2>/dev/null || {
        ARCH="$(uname -m)" "$APPIMAGETOOL" --no-appstream "$APPDIR" "$APPIMAGE_FILE"
    }
    echo "✅ AppImage 已创建: $APPIMAGE_FILE"
    echo ""
    echo "🚀 运行: chmod +x $APPIMAGE_FILE && ./$APPIMAGE_FILE"
else
    echo "⚠️  appimagetool 不可用"
    echo "   AppDir 已组装: $APPDIR"
    echo "   请安装后运行: appimagetool $APPDIR $APPIMAGE_FILE"
fi
