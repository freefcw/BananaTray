#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMP_DIRS=()

cleanup() {
    if [[ ${#TEMP_DIRS[@]} -gt 0 ]]; then
        rm -rf "${TEMP_DIRS[@]}"
    fi
}
trap cleanup EXIT

make_temp_dir() {
    local prefix="$1"
    local temp_dir

    temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/${prefix}.XXXXXX")"
    printf '%s\n' "$temp_dir"
}

fail() {
    echo "error: $*" >&2
    exit 1
}

expect_failure() {
    local description="$1"
    shift

    if "$@" >/dev/null 2>&1; then
        fail "$description should fail"
    fi
}

test_common_argument_validation() {
    expect_failure "unknown packaging argument" \
        bash -c 'source "$1"; parse_args "skip-build" --unexpected' \
        _ "$ROOT_DIR/scripts/common.sh"
    expect_failure "argument unsupported by the current script" \
        bash -c 'source "$1"; parse_args "skip-build" --arch arm64' \
        _ "$ROOT_DIR/scripts/common.sh"
    expect_failure "missing --arch value" \
        bash -c 'source "$1"; parse_args "skip-build arch" --arch' \
        _ "$ROOT_DIR/scripts/common.sh"

    bash -c 'source "$1"; parse_args "skip-build arch" --skip-build --arch arm64; [[ "$SKIP_BUILD" == true && "$ARCH" == arm64 ]]' \
        _ "$ROOT_DIR/scripts/common.sh"
    bash -c 'source "$1"; parse_args "skip-build dmg" --dmg --skip-build; [[ "$CREATE_DMG" == true && "$SKIP_BUILD" == true ]]' \
        _ "$ROOT_DIR/scripts/common.sh"
    bash -c 'source "$1"; parse_args "skip-build no-sign" --skip-build --no-sign; [[ "$SKIP_BUILD" == true && "$SIGN_DMG" == false ]]' \
        _ "$ROOT_DIR/scripts/common.sh"
}

test_required_app_logo() {
    local fixture
    fixture="$(make_temp_dir bananatray-resources-test)"
    TEMP_DIRS+=("$fixture")

    mkdir -p "$fixture/project/src/icons" "$fixture/project/src/tray"
    printf 'tray' > "$fixture/project/src/tray/tray_icon.png"
    printf '<svg/>' > "$fixture/project/src/icons/provider.svg"

    expect_failure "missing app_logo.png" \
        bash -c 'source "$1"; PROJECT_DIR="$2"; copy_runtime_resources "$3"' \
        _ "$ROOT_DIR/scripts/common.sh" "$fixture/project" "$fixture/output"
}

write_fake_tool() {
    local path="$1"

    cat > "$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

tool="$(basename "$0")"
printf '%s %s\n' "$tool" "$*" >> "$FAKE_TOOL_LOG"

case "$tool" in
    cargo)
        mkdir -p "$FAKE_PROJECT_DIR/target/release"
        printf 'binary' > "$FAKE_PROJECT_DIR/target/release/bananatray"
        chmod +x "$FAKE_PROJECT_DIR/target/release/bananatray"
        ;;
    sips)
        output=""
        while [[ $# -gt 0 ]]; do
            if [[ "$1" == "--out" ]]; then
                output="$2"
                break
            fi
            shift
        done
        [[ -n "$output" ]]
        mkdir -p "$(dirname "$output")"
        printf 'icon' > "$output"
        ;;
    iconutil)
        while [[ $# -gt 0 ]]; do
            if [[ "$1" == "-o" ]]; then
                output="$2"
                break
            fi
            shift
        done
        printf 'icns' > "$output"
        ;;
    security)
        printf '  1) FAKE "Fake Developer"\n'
        ;;
    codesign)
        ;;
    create-dmg)
        args=("$@")
        output_index=$((${#args[@]} - 2))
        output="${args[$output_index]}"
        [[ ! -e "$output" ]] || exit 42
        mkdir -p "$(dirname "$output")"
        printf 'dmg' > "$output"
        ;;
    hdiutil)
        case "${1:-}" in
            create)
                output="${*: -1}"
                mkdir -p "$(dirname "$output")"
                printf 'temporary dmg' > "$output"
                ;;
            attach)
                [[ "${FAIL_DMG_VERIFY:-false}" != "true" ]] || exit 43
                if [[ " $* " == *" -readwrite "* ]]; then
                    printf '/dev/disk41 Apple_HFS BananaTray\n'
                else
                    printf '/dev/disk73 Apple_HFS BananaTray\n'
                fi
                ;;
            info)
                printf '/dev/disk99 Apple_HFS Unrelated\n'
                ;;
            detach)
                ;;
            convert)
                output=""
                while [[ $# -gt 0 ]]; do
                    if [[ "$1" == "-o" ]]; then
                        output="$2"
                        break
                    fi
                    shift
                done
                [[ -n "$output" ]]
                mkdir -p "$(dirname "$output")"
                printf 'readonly dmg' > "$output"
                ;;
        esac
        ;;
esac
EOF
    chmod +x "$path"
}

setup_macos_fixture() {
    local fixture="$1"
    local fake_tool="$fixture/fake-tool"

    mkdir -p \
        "$fixture/project/scripts" \
        "$fixture/project/resources/macos" \
        "$fixture/project/src/icons" \
        "$fixture/project/src/tray" \
        "$fixture/fake-bin"
    cp "$ROOT_DIR/scripts/common.sh" "$fixture/project/scripts/common.sh"
    cp "$ROOT_DIR/scripts/bundle.sh" "$fixture/project/scripts/bundle.sh"
    cp "$ROOT_DIR/scripts/bundle-dmg.sh" "$fixture/project/scripts/bundle-dmg.sh"
    chmod +x "$fixture/project/scripts/bundle.sh" "$fixture/project/scripts/bundle-dmg.sh"

    cat > "$fixture/project/Cargo.toml" <<'EOF'
[package]
name = "bananatray"
version = "0.1.0"
homepage = "https://example.com/bananatray"
repository = "https://example.com/bananatray/repository"
EOF
    cat > "$fixture/project/resources/macos/Info.plist" <<'EOF'
<plist><string>0.0.0</string><!-- APP_VERSION --></plist>
EOF
    printf 'entitlements' > "$fixture/project/resources/macos/BananaTray.entitlements"
    printf 'background' > "$fixture/project/resources/macos/dmg-background.png"
    printf 'tray' > "$fixture/project/src/tray/tray_icon.png"
    printf 'logo' > "$fixture/project/src/icons/app_logo.png"
    printf '<svg/>' > "$fixture/project/src/icons/provider.svg"
    printf 'license' > "$fixture/project/LICENSE"

    write_fake_tool "$fake_tool"
    for tool in cargo sips iconutil security codesign create-dmg hdiutil; do
        ln -s "$fake_tool" "$fixture/fake-bin/$tool"
    done
}

run_dmg_script() {
    local fixture="$1"
    shift

    PATH="$fixture/fake-bin:/usr/bin:/bin" \
        FAKE_PROJECT_DIR="$fixture/project" \
        FAKE_TOOL_LOG="$fixture/tools.log" \
        CI=true \
        bash "$fixture/project/scripts/bundle-dmg.sh" "$@"
}

test_dmg_workflow() {
    local fixture cargo_builds detach_calls
    fixture="$(make_temp_dir bananatray-dmg-test)"
    TEMP_DIRS+=("$fixture")
    setup_macos_fixture "$fixture"
    : > "$fixture/tools.log"

    run_dmg_script "$fixture" >/dev/null
    cargo_builds="$(grep -c '^cargo build ' "$fixture/tools.log")"
    [[ "$cargo_builds" -eq 1 ]] || fail "bundle-dmg.sh should build exactly once, got $cargo_builds"

    : > "$fixture/tools.log"
    printf 'stale' > "$fixture/project/target/release/bundle/bananatray.dmg"
    CODESIGN_IDENTITY="Fake Developer" run_dmg_script "$fixture" --skip-build --no-sign >/dev/null
    if grep -q '^codesign .*\.dmg$' "$fixture/tools.log"; then
        fail "--no-sign must skip DMG signing regardless of argument position"
    fi
    detach_calls="$(grep '^hdiutil detach ' "$fixture/tools.log" || true)"
    [[ "$detach_calls" == 'hdiutil detach /dev/disk73' ]] ||
        fail "DMG verification detached unexpected devices: $detach_calls"

    : > "$fixture/tools.log"
    if FAIL_DMG_VERIFY=true run_dmg_script "$fixture" --skip-build --no-sign >/dev/null 2>&1; then
        fail "DMG verification failure must return a non-zero status"
    fi
}

test_hdiutil_fallback() {
    local fixture output detach_calls expected_detaches
    fixture="$(make_temp_dir bananatray-hdiutil-test)"
    TEMP_DIRS+=("$fixture")
    setup_macos_fixture "$fixture"
    rm "$fixture/fake-bin/create-dmg"
    : > "$fixture/tools.log"

    if ! output="$(run_dmg_script "$fixture" --no-sign 2>&1)"; then
        fail "hdiutil fallback failed: $output"
    fi

    grep -q '^hdiutil create ' "$fixture/tools.log" ||
        fail "bundle-dmg.sh should use hdiutil create when create-dmg is unavailable"
    grep -q '^hdiutil convert ' "$fixture/tools.log" ||
        fail "hdiutil fallback should convert the writable image to a read-only DMG"
    detach_calls="$(grep '^hdiutil detach ' "$fixture/tools.log" || true)"
    expected_detaches=$'hdiutil detach /dev/disk41\nhdiutil detach /dev/disk73'
    [[ "$detach_calls" == "$expected_detaches" ]] ||
        fail "hdiutil fallback detached unexpected devices: $detach_calls"
    if grep -q '^hdiutil info ' "$fixture/tools.log"; then
        fail "DMG verification must not discover an unrelated device via hdiutil info"
    fi
}

test_common_argument_validation
test_required_app_logo
test_dmg_workflow
test_hdiutil_fallback

# 解析 AppStream release date：无论走 git 提交日期还是当前日期回退，
# 返回结果必须始终是 ISO 8601 的 YYYY-MM-DD。
test_meta_release_date_format() {
    local d
    d="$(bash -c 'source "$1"; PROJECT_DIR="$2"; meta_release_date' \
        _ "$ROOT_DIR/scripts/common.sh" "$ROOT_DIR")" \
        || fail "meta_release_date should succeed"
    if [[ ! "$d" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
        fail "meta_release_date must return an ISO 8601 YYYY-MM-DD date, got: $d"
    fi
}

# install_metainfo 必须：替换所有 @PLACEHOLDER@（无残留）、
# 生成合法 release date、保留 <developer> 块。
# 直接对仓库内真实模板做渲染验证（自身打包资源自检）。
test_metainfo_render() {
    local fixture out
    fixture="$(make_temp_dir bananatray-metainfo-test)"
    TEMP_DIRS+=("$fixture")

    bash -c '
        source "$1"
        PROJECT_DIR="$2"
        HOMEPAGE_URL="https://example.com/BananaTray"
        REPOSITORY_URL="https://example.com/BananaTray"
        BUGTRACKER_URL="https://example.com/BananaTray/issues"
        VERSION="0.1.0"
        install_metainfo "$3"
    ' _ "$ROOT_DIR/scripts/common.sh" "$ROOT_DIR" "$fixture/out" \
        || fail "install_metainfo failed"

    out="$fixture/out/share/metainfo/com.bananatray.app.metainfo.xml"
    [[ -f "$out" ]] || fail "install_metainfo did not produce the metainfo file"

    # 不应残留任何 @PLACEHOLDER@
    if grep -Eq '@[A-Za-z_]+@' "$out"; then
        local leftovers
        leftovers="$(grep -oE '@[A-Za-z_]+@' "$out" | sort -u | tr '\n' ' ')"
        fail "metainfo still contains unresolved placeholders: $leftovers"
    fi

    # release date 必须是 YYYY-MM-DD
    if ! grep -Eq '<release version="[^"]*" date="[0-9]{4}-[0-9]{2}-[0-9]{2}"' "$out"; then
        fail "metainfo release date is not a valid YYYY-MM-DD"
    fi

    # <developer> 块必须保留（AppStream pedantic 校验所需）
    grep -q '<developer' "$out" || fail "metainfo is missing the <developer> block"
}

test_meta_release_date_format
test_metainfo_render

if LC_ALL=C grep -n $'\357\277\275' "$ROOT_DIR/scripts/bundle.sh" >/dev/null; then
    fail "scripts/bundle.sh contains a Unicode replacement character"
fi

echo "Packaging script tests passed"
