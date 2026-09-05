#!/usr/bin/env bash
#
# BananaTray GNOME Shell Extension — ZIP 打包脚本
#
# 生成符合 extensions.gnome.org (e.g.o) 提交要求的 zip 文件。
#
# 用法:
#   bash scripts/bundle-gnome-extension.sh              # 打包到 target/release/bundle/
#   bash scripts/bundle-gnome-extension.sh --output /tmp # 指定输出目录
#   bash scripts/bundle-gnome-extension.sh --check       # 打包前执行静态检查
#
# ZIP 内容遵循 e.g.o 审核规范:
#   - 只含运行时文件: metadata.json, extension.js, JS 模块, stylesheet.css, locale/, icons/
#   - 不含构建脚本、.po 源文件、README 或仓库元数据
#   - metadata.json 中 version 为整数（e.g.o 要求）
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

EXT_SRC="$PROJECT_DIR/gnome-shell-extension"
OUTPUT_DIR=""
RUN_CHECK=false

# e.g.o 提交到 zip 中的文件白名单（相对于 gnome-shell-extension/）
# 只含运行时必需文件，排除 po/ 源文件、README 和开发脚本
ZIP_FILES=(
    "metadata.json"
    "extension.js"
    "i18n.js"
    "panelButton.js"
    "quotaClient.js"
    "dbusContract.js"
    "quotaPresentation.js"
    "quotaWidgets.js"
    "stylesheet.css"
)

# 递归包含的子目录（zip 会保留目录结构）
ZIP_DIRS=(
    "locale"
    "icons"
)

usage() {
    cat <<'EOF'
Usage: bash scripts/bundle-gnome-extension.sh [OPTIONS]

Options:
  --output DIR    Output directory for the zip file (default: target/release/bundle/)
  --check         Run check-gnome-extension.sh before packaging
  -h, --help      Show this help

The output zip is named: bananatray@bananatray.github.io-VERSION.zip
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --output)
                OUTPUT_DIR="${2:?--output requires a directory}"
                shift 2
                ;;
            --check)
                RUN_CHECK=true
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                echo "Unknown option: $1" >&2
                usage >&2
                exit 1
                ;;
        esac
    done
}

# 从 metadata.json 读取字符串字段
metadata_field() {
    local field="$1"
    grep "\"${field}\"" "$EXT_SRC/metadata.json" \
        | sed 's/.*: *"\(.*\)".*/\1/' \
        | head -1
}

validate_metadata() {
    local uuid version_name version_int

    uuid="$(metadata_field uuid)"
    version_name="$(metadata_field version-name)"

    if [[ -z "$uuid" ]]; then
        echo "❌ metadata.json 缺少 uuid 字段" >&2
        exit 1
    fi

    if [[ -z "$version_name" ]]; then
        echo "❌ metadata.json 缺少 version-name 字段" >&2
        exit 1
    fi

    # e.g.o 要求 metadata.json 包含整数 version 字段
    version_int=$(grep '"version"' "$EXT_SRC/metadata.json" \
        | grep -v 'version-name\|version-name' \
        | sed 's/.*: *\([0-9]*\).*/\1/' \
        | head -1)
    if [[ -z "$version_int" ]]; then
        echo "❌ metadata.json 缺少整数 version 字段（e.g.o 要求）" >&2
        echo "   💡 请在 metadata.json 中添加 \"version\": N（整数），例如 \"version\": 1" >&2
        exit 1
    fi

    # 校验 url 字段存在（e.g.o 审核推荐）
    local url
    url="$(metadata_field url)"
    if [[ -z "$url" ]]; then
        echo "⚠️  metadata.json 没有 url 字段，e.g.o 审核建议提供仓库地址"
    fi
}

validate_source_files() {
    for file in "${ZIP_FILES[@]}"; do
        if [[ ! -f "$EXT_SRC/$file" ]]; then
            echo "❌ 缺少必需文件: gnome-shell-extension/$file" >&2
            exit 1
        fi
    done

    for dir in "${ZIP_DIRS[@]}"; do
        if [[ ! -d "$EXT_SRC/$dir" ]]; then
            echo "❌ 缺少必需目录: gnome-shell-extension/$dir" >&2
            exit 1
        fi
    done

    # 确认 .mo 文件存在（运行时翻译必须已编译）
    local mo_count
    mo_count=$(find "$EXT_SRC/locale" -name "*.mo" 2>/dev/null | wc -l | tr -d ' ')
    if [[ "$mo_count" -eq 0 ]]; then
        echo "⚠️  locale/ 中没有 .mo 文件，翻译将不可用"
    fi
}

main() {
    parse_args "$@"

    echo "📦 BananaTray GNOME Shell Extension ZIP 打包"
    echo ""

    # 可选：打包前执行静态检查
    if [[ "$RUN_CHECK" == "true" ]]; then
        echo "🔍 执行静态检查..."
        bash "$SCRIPT_DIR/check-gnome-extension.sh"
        echo ""
    fi

    # 校验
    validate_metadata
    validate_source_files

    local uuid version_name
    uuid="$(metadata_field uuid)"
    version_name="$(metadata_field version-name)"

    # 确定输出路径
    if [[ -z "$OUTPUT_DIR" ]]; then
        OUTPUT_DIR="$PROJECT_DIR/target/release/bundle"
    fi
    mkdir -p "$OUTPUT_DIR"

    local zip_name="${uuid}-${version_name}.zip"
    local zip_path="$OUTPUT_DIR/$zip_name"

    # 清理已有 zip
    rm -f "$zip_path"

    echo "📋 元数据:"
    echo "   UUID:    $uuid"
    echo "   版本:    $version_name"
    echo ""

    # 构建 zip（从 gnome-shell-extension/ 目录内执行，保持相对路径）
    echo "🗜️  正在打包..."
    (
        cd "$EXT_SRC"

        # 添加单独文件
        zip -q "$zip_path" "${ZIP_FILES[@]}"

        # 添加子目录（递归，排除 .po 源文件）
        for dir in "${ZIP_DIRS[@]}"; do
            zip -q -r "$zip_path" "$dir/" -x "*.po" -x "*.pot"
        done
    )

    # 验证 zip 内容
    echo ""
    echo "📄 ZIP 内容:"
    unzip -l "$zip_path" | awk 'NR>3 && NF>=4 && (/\// || /\.[a-z]/) {print "   " $4}'
    echo ""

    local zip_size
    zip_size=$(du -h "$zip_path" | cut -f1 | tr -d ' ')
    echo "✅ 打包完成: $zip_path"
    echo "   大小: $zip_size"

    # e.g.o 上传提示
    echo ""
    echo "📤 上传到 extensions.gnome.org:"
    echo "   1. 打开 https://extensions.gnome.org/upload/"
    echo "   2. 上传 $zip_name"
    echo "   3. 等待人工审核（通常 1-7 天）"
    echo ""
    echo "⚠️  审核注意事项:"
    echo "   - 确保 metadata.json 的 shell-version 只列出已测试的版本"
    echo "   - 确保代码可读、无混淆、无多余依赖"
    echo "   - 新版本提交时递增 metadata.json 中的 version 整数"
}

main "$@"
