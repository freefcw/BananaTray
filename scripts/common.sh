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

# 读取 Cargo.toml [package] 下的字符串字段。
cargo_package_field() {
    local field="$1"

    awk -F ' = ' -v key="$field" '
        /^\[package\]/ { in_package = 1; next }
        /^\[/ { in_package = 0 }
        in_package && $1 == key {
            value = $2
            gsub(/^"/, "", value)
            gsub(/"$/, "", value)
            print value
            exit
        }
    ' "$PROJECT_DIR/Cargo.toml"
}

# 初始化项目路径变量，从 Cargo.toml 读取版本号和仓库地址
# 调用后可用: PROJECT_DIR, RELEASE_DIR, BUNDLE_DIR, APP_NAME, VERSION, BINARY,
#            HOMEPAGE_URL, REPOSITORY_URL, BUGTRACKER_URL
init_project_vars() {
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
    PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

    APP_NAME="bananatray"
    RELEASE_DIR="$PROJECT_DIR/target/release"
    BUNDLE_DIR="$RELEASE_DIR/bundle"
    BINARY="$RELEASE_DIR/$APP_NAME"

    # 从 Cargo.toml 读取打包元数据（避免多处硬编码）
    VERSION=$(cargo_package_field version)
    HOMEPAGE_URL=$(cargo_package_field homepage)
    REPOSITORY_URL=$(cargo_package_field repository)
    BUGTRACKER_URL="${REPOSITORY_URL}/issues"

    if [ -z "$VERSION" ] || [ -z "$HOMEPAGE_URL" ] || [ -z "$REPOSITORY_URL" ]; then
        echo "❌ 无法从 Cargo.toml 读取版本号或仓库地址"
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
#   target_dir 下会创建 src/icons/ 和 src/tray/tray_icon.png
copy_runtime_resources() {
    local target_dir="$1"
    mkdir -p "$target_dir/src/icons"
    mkdir -p "$target_dir/src/tray"
    cp "$PROJECT_DIR/src/tray/tray_icon.png" "$target_dir/src/tray/tray_icon.png"
    cp "$PROJECT_DIR"/src/icons/*.svg "$target_dir/src/icons/"
}

# 安装多尺寸应用图标到 hicolor 图标主题目录
# 用法: install_icons <prefix_dir>
#   例如 install_icons "$PKG_DIR/usr" 会安装到 $PKG_DIR/usr/share/icons/hicolor/...
# 支持 ImageMagick (convert)、macOS (sips)，否则直接复制原图
install_icons() {
    local prefix_dir="$1"
    local icon_src="$PROJECT_DIR/src/tray/tray_icon.png"

    if [ ! -f "$icon_src" ]; then
        echo "⚠️  未找到图标源文件 ${icon_src}，跳过图标安装"
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
    local metainfo_template="$PROJECT_DIR/resources/linux/com.bananatray.app.metainfo.xml.in"
    local metainfo_dest="$prefix_dir/share/metainfo/com.bananatray.app.metainfo.xml"

    if [ ! -f "$metainfo_template" ]; then
        echo "⚠️  未找到 metainfo 模板 ${metainfo_template}，跳过"
        return
    fi

    mkdir -p "$prefix_dir/share/metainfo"
    sed \
        -e "s|@APP_HOMEPAGE_URL@|$HOMEPAGE_URL|g" \
        -e "s|@APP_BUGTRACKER_URL@|$BUGTRACKER_URL|g" \
        -e "s|@APP_VERSION@|$VERSION|g" \
        "$metainfo_template" > "$metainfo_dest"
}

# 安装 Session D-Bus activation 文件和 systemd user service
# 用法: install_activation_files <prefix_dir>
#   安装到 $prefix_dir/share/dbus-1/services/ 和 $prefix_dir/lib/systemd/user/
#   模板中的 @BANANATRAY_EXEC@ 会替换为安装后的二进制路径。
install_activation_files() {
    local prefix_dir="$1"
    local dbus_service="$PROJECT_DIR/resources/linux/com.bananatray.Daemon.service"
    local systemd_service="$PROJECT_DIR/resources/linux/$APP_NAME.service"
    local installed_exec="${2:-/usr/bin/$APP_NAME}"

    if [ ! -f "$dbus_service" ] || [ ! -f "$systemd_service" ]; then
        echo "⚠️  未找到 D-Bus activation 或 systemd user service 文件，跳过"
        return
    fi

    mkdir -p "$prefix_dir/share/dbus-1/services"
    sed "s|@BANANATRAY_EXEC@|$installed_exec|g" \
        "$dbus_service" > "$prefix_dir/share/dbus-1/services/com.bananatray.Daemon.service"

    mkdir -p "$prefix_dir/lib/systemd/user"
    sed "s|@BANANATRAY_EXEC@|$installed_exec|g" \
        "$systemd_service" > "$prefix_dir/lib/systemd/user/$APP_NAME.service"
}

# AppImage 内部路径不会被宿主 Session Bus 扫描，不能携带指向 /usr/bin 的 activation 文件。
# 用法: remove_activation_files <prefix_dir>
remove_activation_files() {
    local prefix_dir="$1"

    rm -f "$prefix_dir/share/dbus-1/services/com.bananatray.Daemon.service"
    rm -f "$prefix_dir/lib/systemd/user/$APP_NAME.service"
    rmdir "$prefix_dir/share/dbus-1/services" "$prefix_dir/share/dbus-1" 2>/dev/null || true
    rmdir "$prefix_dir/lib/systemd/user" "$prefix_dir/lib/systemd" "$prefix_dir/lib" 2>/dev/null || true
}

# 组装标准 Linux 安装树 (FHS 布局)
# 用法: assemble_install_tree <root_dir>
#   在 root_dir 下创建: usr/bin/, desktop, icons, metainfo, resources, D-Bus activation
#   注意: root_dir 应为包的根目录（如 $PKG_DIR），函数会在其下创建 usr/ 子树
assemble_install_tree() {
    local root_dir="$1"
    local installed_exec="${2:-/usr/bin/$APP_NAME}"

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

    # D-Bus activation + systemd user service
    install_activation_files "$root_dir/usr" "$installed_exec"

    # 运行时资源 (SVG 图标 + tray icon)
    copy_runtime_resources "$root_dir/usr/share/$APP_NAME"
}
