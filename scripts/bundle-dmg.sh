#!/usr/bin/env bash
#
# BananaTray DMG 打包脚本
#
# 使用方法:
#   bash scripts/bundle-dmg.sh              # 默认 release 构建 + DMG
#   bash scripts/bundle-dmg.sh --skip-build  # 跳过编译（使用已有 .app）
#   bash scripts/bundle-dmg.sh --no-sign     # 跳过代码签名
#
# 依赖:
#   - create-dmg (推荐): brew install create-dmg
#   - 或 hdiutil (系统自带，功能较基础)
#
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

init_project_vars
parse_args "$@"
ensure_build

APP_DIR="$BUNDLE_DIR/BananaTray.app"
DMG_DIR="$BUNDLE_DIR/dmg"
DMG_NAME="$APP_NAME"
DMG_PATH="$BUNDLE_DIR/$DMG_NAME.dmg"

# ------------------------------------------------------------------
# 1. 确保 .app 已构建
# ------------------------------------------------------------------
if [ ! -d "$APP_DIR" ]; then
    echo "📦 未找到 .app bundle，正在构建..."
    "$SCRIPT_DIR/bundle.sh" "$@"
fi

if [ ! -d "$APP_DIR" ]; then
    echo "❌ .app bundle 构建失败: $APP_DIR"
    exit 1
fi

echo "✅ 使用 .app bundle: $APP_DIR"

# ------------------------------------------------------------------
# 2. 准备 DMG 内容
# ------------------------------------------------------------------
echo "📁 准备 DMG 内容..."
rm -rf "$DMG_DIR"
mkdir -p "$DMG_DIR"

# 复制 .app 到 DMG 目录
cp -R "$APP_DIR" "$DMG_DIR/"

# 创建 Applications 文件夹链接（方便用户拖拽安装）
ln -s /Applications "$DMG_DIR/Applications"

# 可选：添加背景图片、许可证等资源
BACKGROUND_SRC="$PROJECT_DIR/resources/dmg-background.png"
LICENSE_FILE="$PROJECT_DIR/LICENSE"

if [ -f "$BACKGROUND_SRC" ]; then
    cp "$BACKGROUND_SRC" "$DMG_DIR/.background.png"
fi

if [ -f "$LICENSE_FILE" ]; then
    cp "$LICENSE_FILE" "$DMG_DIR/LICENSE.txt"
fi

echo "✅ DMG 内容准备完成"

# ------------------------------------------------------------------
# 3. 创建 DMG
# ------------------------------------------------------------------
echo "💿 创建 DMG..."

# 检查是否有 create-dmg
if command -v create-dmg >/dev/null 2>&1; then
    echo "   使用 create-dmg (推荐)"

    DMG_ARGS=(
        --volname "$APP_NAME"
        --volicon "$APP_DIR/Contents/Resources/AppIcon.icns"
        --window-pos 200 120
        --window-size 800 600
        --icon-size 100
        --icon "$APP_NAME.app" 200 190
        --hide-extension "$APP_NAME.app"
        --app-drop-link 600 185
        --disk-image-size 200
        --hdiutil-quiet
    )

    # 添加背景图片（如果存在）
    if [ -f "$BACKGROUND_SRC" ]; then
        DMG_ARGS+=(--background "$DMG_DIR/.background.png")
    fi

    # 添加许可证（如果存在）
    if [ -f "$LICENSE_FILE" ]; then
        DMG_ARGS+=(--license "$DMG_DIR/LICENSE.txt")
    fi

    create-dmg "${DMG_ARGS[@]}" "$DMG_PATH" "$DMG_DIR"

elif command -v hdiutil >/dev/null 2>&1; then
    echo "   使用 hdiutil (基础功能)"

    # 创建临时 DMG
    TEMP_DMG="$BUNDLE_DIR/temp.dmg"
    hdiutil create -srcfolder "$DMG_DIR" -volname "$APP_NAME" -fs HFS+ -fsargs "-c c=64,a=16,e=16" -format UDRW -size 200m "$TEMP_DMG"

    # 挂载临时 DMG
    DEVICE=$(hdiutil attach -readwrite -noverify -noautoopen "$TEMP_DMG" | egrep '^/dev/' | sed 1q | awk '{print $1}')

    # 设置外观（如果存在背景图片）
    if [ -f "$BACKGROUND_SRC" ]; then
        echo "   设置 DMG 背景..."
        cat <<EOF | osascript
tell application "Finder"
    tell disk "$APP_NAME"
        open
        set current view of container window to icon view
        set toolbar visible of container window to false
        set statusbar visible of container window to false
        set the bounds of container window to {400, 100, 1200, 700}
        set view_options to the icon view options of container window
        set arrangement of view_options to not arranged
        set icon size of view_options to 100
        set background picture of view_options to file ".background.png" of container window
        set position of item "$APP_NAME.app" of container window to {200, 190}
        set position of item "Applications" of container window to {600, 190}
        close
        open
        update without registering applications
        delay 2
    end tell
end tell
EOF
    fi

    # 卸载并转换为只读 DMG
    hdiutil detach "$DEVICE"
    hdiutil convert "$TEMP_DMG" -format UDZO -imagekey zlib-level=9 -o "$DMG_PATH"
    rm -f "$TEMP_DMG"

else
    echo "❌ 未找到 create-dmg 或 hdiutil"
    echo "   💡 请安装 create-dmg: brew install create-dmg"
    exit 1
fi

echo "✅ DMG 创建完成: $DMG_PATH"

# ------------------------------------------------------------------
# 4. 代码签名（可选）
# ------------------------------------------------------------------
if [ "$1" != "--no-sign" ] && [ "${CODESIGN_IDENTITY:-}" != "" ]; then
    echo "🔏 为 DMG 签名..."
    codesign --force --sign "$CODESIGN_IDENTITY" "$DMG_PATH"
    echo "✅ DMG 签名完成"
fi

echo ""
echo "📦 DMG 信息:"
echo "   文件: $DMG_PATH"
echo "   大小: $(du -h "$DMG_PATH" | cut -f1)"
echo ""

# 验证 DMG
if hdiutil attach -quiet -readonly -noautoopen "$DMG_PATH" 2>/dev/null; then
    echo "✅ DMG 验证通过"
    hdiutil detach "$(hdiutil info | grep "$DMG_NAME" | grep '/dev/disk' | head -1 | awk '{print $1}')" >/dev/null 2>&1
else
    echo "⚠️  DMG 验证失败，但文件已生成"
fi

echo "🚀 安装: open \"$DMG_PATH\""
