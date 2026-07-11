#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/scripts/check-gnome-extension.sh"

fail() {
    echo "error: $*" >&2
    exit 1
}

write_valid_fixture() {
    local fixture="$1"

    mkdir -p \
        "$fixture/gnome-shell-extension" \
        "$fixture/resources/linux" \
        "$fixture/scripts" \
        "$fixture/src/application/selectors"
    printf 'const schema_version = 1;\n' > "$fixture/gnome-shell-extension/quotaClient.js"
    printf 'const schema_version = 1;\n' > "$fixture/scripts/gnome-extension-mock-daemon.js"
    printf 'schema_version: 1,\n' > "$fixture/src/application/selectors/dbus_dto.rs"
    printf 'ExecStart=@BANANATRAY_EXEC@\n' > "$fixture/resources/linux/com.bananatray.Daemon.service"
    printf 'ExecStart=@BANANATRAY_EXEC@\n' > "$fixture/resources/linux/bananatray.service"
    printf 'systemctl --user daemon-reload\n' > "$fixture/scripts/bundle-deb.sh"
    printf 'systemctl --user daemon-reload\n' > "$fixture/scripts/bundle-rpm.sh"
}

expect_missing_contract_failure() {
    local fixture="$1"
    local relative_path="$2"
    local output

    printf 'contract removed\n' > "$fixture/$relative_path"
    if output="$(check_gnome_packaging_contracts "$fixture" 2>&1)"; then
        fail "missing contract in $relative_path should fail"
    fi
    [[ "$output" == *"$relative_path"* ]] || \
        fail "failure for $relative_path should identify the drifting file"
}

contract_files=(
    "gnome-shell-extension/quotaClient.js"
    "scripts/gnome-extension-mock-daemon.js"
    "src/application/selectors/dbus_dto.rs"
    "resources/linux/com.bananatray.Daemon.service"
    "resources/linux/bananatray.service"
    "scripts/bundle-deb.sh"
    "scripts/bundle-rpm.sh"
)

baseline="$(mktemp -d "${TMPDIR:-/tmp}/bananatray-gnome-contract-test.XXXXXX")"
trap 'rm -rf "$baseline"' EXIT
write_valid_fixture "$baseline"
check_gnome_packaging_contracts "$baseline"

for relative_path in "${contract_files[@]}"; do
    fixture="$baseline/case-${relative_path//\//-}"
    mkdir -p "$fixture"
    cp -R "$baseline/gnome-shell-extension" "$baseline/resources" \
        "$baseline/scripts" "$baseline/src" "$fixture/"
    expect_missing_contract_failure "$fixture" "$relative_path"
done

echo "GNOME packaging contract tests passed"
