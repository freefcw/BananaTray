#!/usr/bin/env bash
#
# BananaTray macOS App Bundle 打包脚本
#
# 使用方法:
#   bash scripts/bundle.sh              # 默认 release 构建
#   bash scripts/bundle.sh --skip-build  # 跳过编译（使用已有二进制）
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
echo ""
echo "🚀 运行: open \"$APP_DIR\""
