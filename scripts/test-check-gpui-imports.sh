#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKER="$PROJECT_DIR/scripts/check-gpui-imports.sh"
tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/bananatray-gpui-imports-test.XXXXXX")"
trap 'rm -rf "$tmp_dir"' EXIT

mkdir -p "$tmp_dir/scripts" "$tmp_dir/src"
cp "$CHECKER" "$tmp_dir/scripts/check-gpui-imports.sh"

assert_allowed() {
    local source="$1"
    printf '%s\n' "$source" > "$tmp_dir/src/case.rs"
    if ! bash "$tmp_dir/scripts/check-gpui-imports.sh" >/dev/null; then
        echo "error: explicit GPUI import was rejected: $source" >&2
        exit 1
    fi
}

assert_forbidden() {
    local source="$1"
    printf '%b\n' "$source" > "$tmp_dir/src/case.rs"
    if bash "$tmp_dir/scripts/check-gpui-imports.sh" >/dev/null 2>&1; then
        echo "error: GPUI glob import was not rejected: $source" >&2
        exit 1
    fi
}

assert_allowed 'use gpui::{App, Context, Window};'
assert_allowed 'pub use gpui::App;'

assert_forbidden 'use gpui::*;'
assert_forbidden 'pub use gpui::*;'
assert_forbidden 'use gpui :: * ;'
assert_forbidden 'pub use gpui::prelude :: * ;'
assert_forbidden 'use gpui::{App, *};'
assert_forbidden 'pub(crate) use gpui :: { App,\n    *\n};'

echo "GPUI glob import checker tests passed"
