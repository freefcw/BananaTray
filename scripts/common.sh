#!/usr/bin/env bash
#
# BananaTray 打包脚本公共函数
#
# 用法: source scripts/common.sh
#
# 提供:
#   - init_project_vars        初始化项目路径 + 从 Cargo.toml 读取版本号
#   - parse_args               解析公共参数 (--skip-build, --arch)
#   - ensure_build             编译 release 并校验二进制存在
#   - copy_runtime_resources   复制 SVG/PNG 运行时资源到目标目录
#   - install_icons            安装多尺寸 hicolor 图标
#   - install_metainfo         安装 AppStream metainfo
#   - assemble_install_tree    组装标准 Linux 安装树
#
set -euo pipefail

# 初始化项目路径变量，从 Cargo.toml 读取版本号
# 调用后可用: PROJECT_DIR, RELEASE_DIR, BUNDLE_DIR, APP_NAME, VERSION, BINARY
init_project_vars() {
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
    PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

    APP_NAME="bananatray"
    RELEASE_DIR="$PROJECT_DIR/target/release"
    BUNDLE_DIR="$RELEASE_DIR/bundle"
    BINARY="$RELEASE_DIR/$APP_NAME"

    # 从 Cargo.toml 读取版本号（避免多处硬编码）
    VERSION=$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "❌ 无法从 Cargo.toml 读取版本号"
        exit 1
    fi
}

# 解析公共命令行参数
# 设置: SKIP_BUILD, ARCH
# 用法: parse_args "$@"
SKIP_BUILD=false
ARCH="amd64"

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --skip-build)
                SKIP_BUILD=true
                shift
                ;;
            --arch)
                ARCH="${2:-amd64}"
                shift 2
                ;;
            *)
                shift
                ;;
        esac
    done
}

# 编译 release 并校验二进制存在
ensure_build() {
    if [ "$SKIP_BUILD" = false ]; then
        echo "🔨 正在编译 release 版本..."
        cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"
        echo "✅ 编译完成"
    else
        echo "⏭️  跳过编译步骤"
    fi

    if [ ! -f "$BINARY" ]; then
        echo "❌ 未找到 release 二进制: $BINARY"
        exit 1
    fi
}

# 复制运行时资源 (SVG 图标 + tray_icon.png) 到目标目录
# 用法: copy_runtime_resources <target_dir>
#   target_dir 下会创建 src/icons/ 和 src/tray_icon.png
copy_runtime_resources() {
    local target_dir="$1"
    mkdir -p "$target_dir/src/icons"
    cp "$PROJECT_DIR/src/tray_icon.png" "$target_dir/src/tray_icon.png"
    cp "$PROJECT_DIR"/src/icons/*.svg "$target_dir/src/icons/"
}

# 安装多尺寸应用图标到 hicolor 图标主题目录
# 用法: install_icons <prefix_dir>
#   例如 install_icons "$PKG_DIR/usr" 会安装到 $PKG_DIR/usr/share/icons/hicolor/...
# 支持 ImageMagick (convert)、macOS (sips)，否则直接复制原图
install_icons() {
    local prefix_dir="$1"
    local icon_src="$PROJECT_DIR/src/tray_icon.png"

    if [ ! -f "$icon_src" ]; then
        echo "⚠️  未找到图标源文件 $icon_src，跳过图标安装"
        return
    fi

    for size in 16 32 48 64 128 256; do
        local icon_dir="$prefix_dir/share/icons/hicolor/${size}x${size}/apps"
        mkdir -p "$icon_dir"
        if command -v convert &>/dev/null; then
            convert "$icon_src" -resize "${size}x${size}" "$icon_dir/$APP_NAME.png"
        elif command -v sips &>/dev/null; then
            sips -z "$size" "$size" "$icon_src" --out "$icon_dir/$APP_NAME.png" >/dev/null 2>&1
        else
            cp "$icon_src" "$icon_dir/$APP_NAME.png"
        fi
    done
}

# 安装 AppStream metainfo 文件
# 用法: install_metainfo <prefix_dir>
#   安装到 $prefix_dir/share/metainfo/com.bananatray.app.metainfo.xml
install_metainfo() {
    local prefix_dir="$1"
    local metainfo_src="$PROJECT_DIR/resources/linux/com.bananatray.app.metainfo.xml"

    if [ ! -f "$metainfo_src" ]; then
        echo "⚠️  未找到 metainfo 文件 $metainfo_src，跳过"
        return
    fi

    mkdir -p "$prefix_dir/share/metainfo"
    cp "$metainfo_src" "$prefix_dir/share/metainfo/com.bananatray.app.metainfo.xml"
}

# 组装标准 Linux 安装树 (FHS 布局)
# 用法: assemble_install_tree <root_dir>
#   在 root_dir 下创建: usr/bin/, usr/share/applications/, icons, metainfo, resources
#   注意: root_dir 应为包的根目录（如 $PKG_DIR），函数会在其下创建 usr/ 子树
assemble_install_tree() {
    local root_dir="$1"

    # 二进制
    mkdir -p "$root_dir/usr/bin"
    cp "$BINARY" "$root_dir/usr/bin/$APP_NAME"
    chmod 755 "$root_dir/usr/bin/$APP_NAME"

    # .desktop 启动器
    mkdir -p "$root_dir/usr/share/applications"
    cp "$PROJECT_DIR/resources/linux/bananatray.desktop" \
       "$root_dir/usr/share/applications/$APP_NAME.desktop"

    # 图标 (多尺寸 hicolor)
    install_icons "$root_dir/usr"

    # AppStream metainfo
    install_metainfo "$root_dir/usr"

    # 运行时资源 (SVG 图标 + tray icon)
    copy_runtime_resources "$root_dir/usr/share/$APP_NAME"
}
