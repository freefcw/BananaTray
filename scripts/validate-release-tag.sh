#!/usr/bin/env bash
#
# 校验 GitHub Release tag 与 Cargo.toml 版本一致。
#
set -euo pipefail

release_tag="${1:?usage: validate-release-tag.sh <tag>}"

case "$release_tag" in
    v*) ;;
    *)
        echo "Release tag must start with v, got: $release_tag"
        exit 1
        ;;
esac

cargo_version="$(
    awk -F ' = ' '
        /^\[package\]/ { in_package = 1; next }
        /^\[/ { in_package = 0 }
        in_package && $1 == "version" {
            value = $2
            gsub(/^"/, "", value)
            gsub(/"$/, "", value)
            print value
            exit
        }
    ' Cargo.toml
)"
tag_version="${release_tag#v}"

# 允许预发布后缀：v0.1.0-rc.1 的基础版本是 0.1.0，需与 Cargo.toml 一致。
# 正式 tag v0.1.0 同样通过（无后缀时 base_version == tag_version）。
base_version="${tag_version%%-*}"

if [ "$cargo_version" != "$base_version" ]; then
    echo "Cargo.toml version ($cargo_version) must match tag base version ($base_version)."
    [ "$base_version" != "$tag_version" ] && echo "  (tag $release_tag has pre-release suffix, base version extracted as $base_version)"
    exit 1
fi
