#!/usr/bin/env bash
#
# BananaTray macOS App Bundle 打包脚本
#
# 使用方法:
#   bash scripts/bundle.sh          # 默认 release 构建
#   bash scripts/bundle.sh --skip-build  # 跳过编译（使用已有二进制）
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_NAME="BananaTray"
BUNDLE_ID="com.bananatray.app"
BINARY_NAME="bananatray"
VERSION="0.1.0"

RELEASE_DIR="$PROJECT_DIR/target/release"
BUNDLE_DIR="$RELEASE_DIR/bundle"
APP_DIR="$BUNDLE_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

SKIP_BUILD=false
for arg in "$@"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=true ;;
    esac
done

# ------------------------------------------------------------------
# 1. 编译 release 二进制
# ------------------------------------------------------------------
if [ "$SKIP_BUILD" = false ]; then
    echo "🔨 正在编译 release 版本..."
    cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
    echo "✅ 编译完成"
else
    echo "⏭️  跳过编译步骤"
fi

if [ ! -f "$RELEASE_DIR/$BINARY_NAME" ]; then
    echo "❌ 未找到 release 二进制: $RELEASE_DIR/$BINARY_NAME"
    exit 1
fi

# ------------------------------------------------------------------
# 2. 生成 .icns 图标
# ------------------------------------------------------------------
echo "🎨 生成应用图标..."
ICON_SRC="$PROJECT_DIR/src/tray_icon.png"
ICONSET_DIR="$BUNDLE_DIR/AppIcon.iconset"

if [ ! -f "$ICON_SRC" ]; then
    echo "⚠️  未找到图标源文件 $ICON_SRC，跳过图标生成"
else
    rm -rf "$ICONSET_DIR"
    mkdir -p "$ICONSET_DIR"

    # macOS iconset 需要多种尺寸
    sizes=(16 32 64 128 256 512)
    for size in "${sizes[@]}"; do
        sips -z "$size" "$size" "$ICON_SRC" --out "$ICONSET_DIR/icon_${size}x${size}.png" >/dev/null 2>&1
    done
    # @2x 版本
    sips -z 32  32  "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16@2x.png"   >/dev/null 2>&1
    sips -z 64  64  "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32@2x.png"   >/dev/null 2>&1
    sips -z 128 128 "$ICON_SRC" --out "$ICONSET_DIR/icon_64x64@2x.png"   >/dev/null 2>&1
    sips -z 256 256 "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null 2>&1
    sips -z 512 512 "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null 2>&1
    sips -z 1024 1024 "$ICON_SRC" --out "$ICONSET_DIR/icon_512x512@2x.png" >/dev/null 2>&1

    iconutil -c icns "$ICONSET_DIR" -o "$BUNDLE_DIR/AppIcon.icns"
    rm -rf "$ICONSET_DIR"
    echo "✅ 图标生成完成"
fi

# ------------------------------------------------------------------
# 3. 组装 .app bundle
# ------------------------------------------------------------------
echo "📦 组装 App Bundle..."
rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# 复制 Info.plist
cp "$PROJECT_DIR/resources/Info.plist" "$CONTENTS_DIR/Info.plist"

# 复制二进制
cp "$RELEASE_DIR/$BINARY_NAME" "$MACOS_DIR/$BINARY_NAME"
chmod +x "$MACOS_DIR/$BINARY_NAME"

# 复制图标
if [ -f "$BUNDLE_DIR/AppIcon.icns" ]; then
    cp "$BUNDLE_DIR/AppIcon.icns" "$RESOURCES_DIR/AppIcon.icns"
fi

# 复制资源文件（保持与 CARGO_MANIFEST_DIR 相同的相对路径结构）
# AssetSource 会通过 base.join(path) 加载，path 形如 "src/icons/xxx.svg"
mkdir -p "$RESOURCES_DIR/src/icons"
cp "$PROJECT_DIR/src/tray_icon.png" "$RESOURCES_DIR/src/tray_icon.png"
cp "$PROJECT_DIR"/src/icons/*.svg "$RESOURCES_DIR/src/icons/"

echo "✅ App Bundle 已创建: $APP_DIR"
echo ""
echo "📂 目录结构:"
echo "   $APP_DIR/"
echo "   ├── Contents/"
echo "   │   ├── Info.plist"
echo "   │   ├── MacOS/"
echo "   │   │   └── $BINARY_NAME"
echo "   │   └── Resources/"
echo "   │       ├── AppIcon.icns"
echo "   │       └── src/"
echo "   │           ├── icons/ ($(ls "$RESOURCES_DIR/src/icons/" | wc -l | tr -d ' ') 个 SVG)"
echo "   │           └── tray_icon.png"
echo ""
echo "🚀 运行方式:"
echo "   open \"$APP_DIR\""
echo "   # 或拖到 Applications 文件夹"
