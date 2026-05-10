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

if [ "$cargo_version" != "$tag_version" ]; then
    echo "Cargo.toml version ($cargo_version) must match tag version ($tag_version)."
    exit 1
fi
