#!/usr/bin/env bash
#
# 上传构建产物到同名 GitHub draft release。
#
# 语义：
#   - release 不存在时创建 draft release
#   - release 已存在且仍是 draft 时覆盖同名资产
#   - release 已发布时拒绝覆盖
#
set -euo pipefail

release_tag="${1:?usage: upload-release-assets.sh <tag> <asset>...}"
shift

if [ "$#" -eq 0 ]; then
    echo "No release assets provided."
    exit 1
fi

prerelease_args=()
case "$release_tag" in
    *-*) prerelease_args=(--prerelease) ;;
esac

release_state_for_tag() {
    RELEASE_TAG="$release_tag" gh api repos/{owner}/{repo}/releases --paginate \
        --jq '.[] | select(.tag_name == env.RELEASE_TAG) | if .draft then "draft" else "published" end'
}

release_state="$(release_state_for_tag)"

if [ -z "$release_state" ]; then
    if ! gh release create "$release_tag" \
        --verify-tag \
        --draft \
        --generate-notes \
        --title "BananaTray $release_tag" \
        "${prerelease_args[@]}"; then
        # 另一个平台 job 可能刚刚创建了同一个 draft release。
        release_state="$(release_state_for_tag)"
        if [ "$release_state" != "draft" ]; then
            echo "Release $release_tag could not be created or reused as a draft."
            exit 1
        fi
    fi
elif [ "$release_state" = "draft" ]; then
    gh release edit "$release_tag" \
        --title "BananaTray $release_tag" \
        --draft \
        "${prerelease_args[@]}"
else
    echo "Release $release_tag already exists and is published. Refusing to replace published assets."
    exit 1
fi

gh release upload "$release_tag" "$@" --clobber
