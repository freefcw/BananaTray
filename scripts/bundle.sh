#!/usr/bin/env bash
#
# BananaTray macOS App Bundle 打包脚本
#
# 使用方法:
#   bash scripts/bundle.sh              # 默认 release 构建
#   bash scripts/bundle.sh --skip-build  # 跳过编译（使用已有二进制）
#   bash scripts/bundle.sh --dmg        # 构建 .app 并创建 DMG
#   bash scripts/bundle.sh --dmg --skip-build  # 使用已有 .app 创建 DMG
#
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

init_project_vars
parse_args "$@"
ensure_build

APP_DIR="$BUNDLE_DIR/BananaTray.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

# ------------------------------------------------------------------
# 1. 生成 .icns 图标
# ------------------------------------------------------------------
echo "🎨 生成应用图标..."
ICON_SRC="$PROJECT_DIR/src/tray/tray_icon.png"
ICONSET_DIR="$BUNDLE_DIR/AppIcon.iconset"

if [ ! -f "$ICON_SRC" ]; then
    echo "⚠️  未找到图标源文件 ${ICON_SRC}，跳过图标生成"
else
    rm -rf "$ICONSET_DIR"
    mkdir -p "$ICONSET_DIR"

    sizes=(16 32 64 128 256 512)
    for size in "${sizes[@]}"; do
        sips -z "$size" "$size" "$ICON_SRC" --out "$ICONSET_DIR/icon_${size}x${size}.png" >/dev/null 2>&1
    done
    # @2x 版本
    sips -z 32   32   "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16@2x.png"   >/dev/null 2>&1
    sips -z 64   64   "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32@2x.png"   >/dev/null 2>&1
    sips -z 128  128  "$ICON_SRC" --out "$ICONSET_DIR/icon_64x64@2x.png"   >/dev/null 2>&1
    sips -z 256  256  "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null 2>&1
    sips -z 512  512  "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null 2>&1
    sips -z 1024 1024 "$ICON_SRC" --out "$ICONSET_DIR/icon_512x512@2x.png" >/dev/null 2>&1

    iconutil -c icns "$ICONSET_DIR" -o "$BUNDLE_DIR/AppIcon.icns"
    rm -rf "$ICONSET_DIR"
    echo "✅ 图标生成完成"
fi

# ------------------------------------------------------------------
# 2. 组装 .app bundle
# ------------------------------------------------------------------
echo "📦 组装 App Bundle..."
rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# Info.plist
cp "$PROJECT_DIR/resources/Info.plist" "$CONTENTS_DIR/Info.plist"

# 二进制
cp "$BINARY" "$MACOS_DIR/$APP_NAME"
chmod +x "$MACOS_DIR/$APP_NAME"

# 图标
if [ -f "$BUNDLE_DIR/AppIcon.icns" ]; then
    cp "$BUNDLE_DIR/AppIcon.icns" "$RESOURCES_DIR/AppIcon.icns"
fi

# 运行时资源
copy_runtime_resources "$RESOURCES_DIR"

echo "✅ App Bundle 已创建: $APP_DIR"

# ------------------------------------------------------------------
# 3. Hardened Runtime 代码签名
# ------------------------------------------------------------------
# 使用 hardened runtime (--options runtime) + entitlements 签名。
# 相比简单的 ad-hoc 签名，hardened runtime 让 macOS 能更稳定地
# 记住 TCC 授权，避免每次重新构建后弹出"需要访问网络卷宗"的弹窗。
#
# 签名身份优先级：
#   1. 环境变量 CODESIGN_IDENTITY（如 "Apple Development: you@email.com (TEAMID)"）
#   2. 回退到 ad-hoc 签名 "-"
ENTITLEMENTS="$PROJECT_DIR/resources/BananaTray.entitlements"
SIGN_IDENTITY="${CODESIGN_IDENTITY:--}"
if [ "$SIGN_IDENTITY" != "-" ] && ! security find-identity -v -p codesigning | grep -Fq "\"$SIGN_IDENTITY\""; then
    echo "❌ 指定的 CODESIGN_IDENTITY 当前不是可用的 macOS 代码签名身份: $SIGN_IDENTITY"
    echo "   💡 常见原因：证书链不完整、WWDR 中间证书过期、缺少对应私钥，或私钥未授权给 codesign 访问"
    echo "   💡 运行 'security find-identity -v -p codesigning' 可查看当前可用身份"
    echo "   💡 如仅需本地测试，请先 unset CODESIGN_IDENTITY 后重试，脚本会回退到 ad-hoc 签名"
    exit 1
fi
if [ "$SIGN_IDENTITY" = "-" ]; then
    echo "🔏 Hardened Runtime 代码签名 (ad-hoc)..."
    echo "   💡 设置 CODESIGN_IDENTITY 环境变量可使用 Apple Developer 证书签名"
else
    echo "🔏 Hardened Runtime 代码签名..."
    echo "   🔑 使用证书: $SIGN_IDENTITY"
fi
codesign --force --deep --sign "$SIGN_IDENTITY" \
    --options runtime \
    --entitlements "$ENTITLEMENTS" \
    "$APP_DIR"
echo "✅ 签名完成"

echo ""
echo "📂 目录结构:"
echo "   $APP_DIR/"
echo "   ├── Contents/"
echo "   │   ├── Info.plist"
echo "   │   ├── MacOS/$APP_NAME"
echo "   │   └── Resources/"
echo "   │       ├── AppIcon.icns"
echo "   │       └── src/icons/ ($(ls "$RESOURCES_DIR/src/icons/" | wc -l | tr -d ' ') 个 SVG)"
# ------------------------------------------------------------------
# 4. DMG 创建（可选）
# ------------------------------------------------------------------
if [[ "$*" == *"--dmg"* ]]; then
    echo ""
    echo "� 创建 DMG..."

    # 检查依赖
    if ! command -v create-dmg >/dev/null 2>&1 && ! command -v hdiutil >/dev/null 2>&1; then
        echo "❌ 未找到 create-dmg 或 hdiutil"
        echo "   💡 请安装 create-dmg: brew install create-dmg"
        exit 1
    fi

    DMG_DIR="$BUNDLE_DIR/dmg"
    DMG_PATH="$BUNDLE_DIR/$APP_NAME.dmg"

    # 准备 DMG 内容
    echo "📁 准备 DMG 内容..."
    rm -rf "$DMG_DIR"
    mkdir -p "$DMG_DIR"

    # 复制 .app 和创建 Applications 链接
    cp -R "$APP_DIR" "$DMG_DIR/"
    ln -s /Applications "$DMG_DIR/Applications"

    # 可选资源
    BACKGROUND_SRC="$PROJECT_DIR/resources/dmg-background.png"
    LICENSE_FILE="$PROJECT_DIR/LICENSE"

    if [ -f "$BACKGROUND_SRC" ]; then
        cp "$BACKGROUND_SRC" "$DMG_DIR/.background.png"
    fi

    if [ -f "$LICENSE_FILE" ]; then
        cp "$LICENSE_FILE" "$DMG_DIR/LICENSE.txt"
    fi

    # 创建 DMG
    # 清理已存在的文件
    rm -f "$DMG_PATH"

    if command -v create-dmg >/dev/null 2>&1; then
        echo "   使用 create-dmg..."
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

        [ -f "$BACKGROUND_SRC" ] && DMG_ARGS+=(--background "$DMG_DIR/.background.png")
        [ -f "$LICENSE_FILE" ] && DMG_ARGS+=(--license "$DMG_DIR/LICENSE.txt")

        create-dmg "${DMG_ARGS[@]}" "$DMG_PATH" "$DMG_DIR"
    else
        echo "   使用 hdiutil..."
        TEMP_DMG="$BUNDLE_DIR/temp.dmg"
        rm -f "$TEMP_DMG"
        hdiutil create -srcfolder "$DMG_DIR" -volname "$APP_NAME" -fs HFS+ -fsargs "-c c=64,a=16,e=16" -format UDRW -size 200m "$TEMP_DMG"

        DEVICE=$(hdiutil attach -readwrite -noverify -noautoopen "$TEMP_DMG" | egrep '^/dev/' | sed 1q | awk '{print $1}')
        hdiutil detach "$DEVICE"
        hdiutil convert "$TEMP_DMG" -format UDZO -imagekey zlib-level=9 -o "$DMG_PATH"
        rm -f "$TEMP_DMG"
    fi

    echo "✅ DMG 创建完成: $DMG_PATH"
    echo "   大小: $(du -h "$DMG_PATH" | cut -f1)"
fi

echo ""
echo "�� 运行: open \"$APP_DIR\""
if [[ "$*" != *"--dmg"* ]]; then
    echo ""
    echo "💿 创建 DMG: bash scripts/bundle.sh --dmg"
fi
