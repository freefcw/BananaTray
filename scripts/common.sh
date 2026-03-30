#!/usr/bin/env bash
#
# BananaTray 打包脚本公共函数
#
# 用法: source scripts/common.sh
#
# 提供:
#   - init_project_vars   初始化项目路径 + 从 Cargo.toml 读取版本号
#   - parse_args          解析公共参数 (--skip-build, --arch)
#   - ensure_build        编译 release 并校验二进制存在
#   - copy_runtime_resources  复制 SVG/PNG 运行时资源到目标目录
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
