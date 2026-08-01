#!/usr/bin/env bash
#
# BananaTray 打包脚本公共函数
#
# 用法: source scripts/common.sh
#
# 提供:
#   - init_project_vars        初始化项目路径 + 版本号（RELEASE_TAG 优先，回退 Cargo.toml）
#   - parse_args               按调用方声明解析打包参数
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

# 初始化项目路径变量，解析版本号（RELEASE_TAG 优先，回退 Cargo.toml）和仓库地址
# 调用后可用: PROJECT_DIR, RELEASE_DIR, BUNDLE_DIR, APP_NAME, VERSION, BINARY,
#            VERSION_BASE, RPM_VERSION, DEB_VERSION,
#            HOMEPAGE_URL, REPOSITORY_URL, BUGTRACKER_URL
init_project_vars() {
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")" && pwd)"
    PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

    APP_NAME="bananatray"
    RELEASE_DIR="$PROJECT_DIR/target/release"
    BUNDLE_DIR="$RELEASE_DIR/bundle"
    BINARY="$RELEASE_DIR/$APP_NAME"

    # 版本号来源优先级：
    #   1. RELEASE_TAG 环境变量（CI workflow 设置，如 v0.1.0-rc.4）→ 剥除 v 前缀
    #   2. Cargo.toml version 字段（本地开发回退）
    if [ -n "${RELEASE_TAG:-}" ]; then
        VERSION="${RELEASE_TAG#v}"
    else
        VERSION=$(cargo_package_field version)
    fi

    # 派生各打包格式专用版本号：
    #   VERSION_BASE  — 基础 MAJOR.MINOR.PATCH，无预发布后缀（macOS plist 用）
    #   RPM_VERSION   — RPM Version 字段禁止连字符，预发布用 ~ 前缀（如 0.1.0~rc.4）
    #                   ~ 在 RPM 排序中低于无后缀版本，即 0.1.0~rc.4 < 0.1.0 ✓
    #   DEB_VERSION   — Debian 同样用 ~ 表示预发布（如 0.1.0~rc.4）
    VERSION_BASE="${VERSION%%-*}"
    if [ "$VERSION" != "$VERSION_BASE" ]; then
        # 有预发布后缀：0.1.0-rc.4 → suffix=rc.4
        local suffix="${VERSION#*-}"
        RPM_VERSION="${VERSION_BASE}~${suffix}"
        DEB_VERSION="${VERSION_BASE}~${suffix}"
    else
        RPM_VERSION="$VERSION"
        DEB_VERSION="$VERSION"
    fi

    HOMEPAGE_URL=$(cargo_package_field homepage)
    REPOSITORY_URL=$(cargo_package_field repository)
    BUGTRACKER_URL="${REPOSITORY_URL}/issues"

    if [ -z "$VERSION" ] || [ -z "$HOMEPAGE_URL" ] || [ -z "$REPOSITORY_URL" ]; then
        echo "❌ 无法从 Cargo.toml 读取版本号或仓库地址"
        exit 1
    fi
}

# 解析打包命令行参数
# 设置: SKIP_BUILD, ARCH, CREATE_DMG, SIGN_DMG
# 用法: parse_args "skip-build arch" "$@"
SKIP_BUILD=false
ARCH="amd64"
CREATE_DMG=false
SIGN_DMG=true

argument_is_allowed() {
    local allowed_args="$1"
    local argument="$2"

    [[ " $allowed_args " == *" $argument "* ]]
}

reject_unsupported_argument() {
    local argument="$1"

    echo "❌ 不支持的参数: $argument" >&2
    return 2
}

parse_args() {
    local allowed_args="${1:-}"
    shift || true

    while [[ $# -gt 0 ]]; do
        case "$1" in
            --skip-build)
                if ! argument_is_allowed "$allowed_args" "skip-build"; then
                    reject_unsupported_argument "$1"
                    return 2
                fi
                SKIP_BUILD=true
                shift
                ;;
            --arch)
                if ! argument_is_allowed "$allowed_args" "arch"; then
                    reject_unsupported_argument "$1"
                    return 2
                fi
                if [[ $# -lt 2 || "$2" == -* ]]; then
                    echo "❌ 参数 --arch 需要一个架构值" >&2
                    return 2
                fi
                ARCH="$2"
                shift 2
                ;;
            --dmg)
                if ! argument_is_allowed "$allowed_args" "dmg"; then
                    reject_unsupported_argument "$1"
                    return 2
                fi
                CREATE_DMG=true
                shift
                ;;
            --no-sign)
                if ! argument_is_allowed "$allowed_args" "no-sign"; then
                    reject_unsupported_argument "$1"
                    return 2
                fi
                SIGN_DMG=false
                shift
                ;;
            *)
                reject_unsupported_argument "$1"
                return 2
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

    # app_logo.png 是设置窗口运行时必需资源；缺失时不要留下部分组装的资源树。
    if [ ! -f "$PROJECT_DIR/src/icons/app_logo.png" ]; then
        echo "❌ 未找到必需资源: $PROJECT_DIR/src/icons/app_logo.png" >&2
        return 1
    fi

    mkdir -p "$target_dir/src/icons"
    mkdir -p "$target_dir/src/tray"
    cp "$PROJECT_DIR"/src/icons/*.png "$target_dir/src/icons/"
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

# 解析 AppStream release date（ISO 8601 YYYY-MM-DD）。
# 优先使用最近一次提交日期（release workflow 按 tag checkout，即该版本发布日期）；
# git 不可用或非工作树时回退到当前日期。
# 用法: meta_release_date
meta_release_date() {
    if git -C "$PROJECT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        local date
        date="$(git -C "$PROJECT_DIR" log -1 --format=%cd --date=short 2>/dev/null || true)"
        if [ -n "$date" ]; then
            printf '%s\n' "$date"
            return
        fi
    fi
    date +%F
}

# 安装 AppStream metainfo 文件
# 用法: install_metainfo <prefix_dir>
#   安装到 $prefix_dir/share/metainfo/com.bananatray.app.metainfo.xml
install_metainfo() {
    local prefix_dir="$1"
    local metainfo_template="$PROJECT_DIR/resources/linux/com.bananatray.app.metainfo.xml.in"
    local metainfo_dest="$prefix_dir/share/metainfo/com.bananatray.app.metainfo.xml"
    local meta_date
    meta_date="$(meta_release_date)"

    if [ ! -f "$metainfo_template" ]; then
        echo "⚠️  未找到 metainfo 模板 ${metainfo_template}，跳过"
        return
    fi

    mkdir -p "$prefix_dir/share/metainfo"
    sed \
        -e "s|@APP_HOMEPAGE_URL@|$HOMEPAGE_URL|g" \
        -e "s|@APP_BUGTRACKER_URL@|$BUGTRACKER_URL|g" \
        -e "s|@APP_VERSION@|$VERSION|g" \
        -e "s|@APP_RELEASE_DATE@|$meta_date|g" \
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
