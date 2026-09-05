set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

default:
    @just --list

run:
    cargo run

build-release:
    cargo build --release

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

audit-provider-icons:
    cargo test --quiet --example provider_icon_metrics --no-default-features
    cargo run --quiet --example provider_icon_metrics --no-default-features -- src/icons/provider-*.svg

check-provider-icons: audit-provider-icons
    python3 -m unittest scripts/test_check_provider_icons.py
    python3 scripts/check_provider_icons.py --check-preview

render-provider-icons:
    python3 scripts/check_provider_icons.py --write-preview

check-gpui-imports:
    bash ./scripts/test-check-gpui-imports.sh
    ./scripts/check-gpui-imports.sh

check-provider-secret-slicing:
    ./scripts/check-provider-secret-slicing.sh

check-release-panic-profile:
    ./scripts/check-release-panic-profile.sh

check-gnome-extension:
    ./scripts/check-gnome-extension.sh

test-gnome-packaging-contracts:
    ./scripts/test-gnome-packaging-contracts.sh

test-packaging-scripts:
    ./scripts/test-packaging-scripts.sh

test-custom-provider-migration:
    python3 -m unittest scripts/test_migrate_custom_provider_yaml.py

clippy-lib-fast:
    cargo clippy --lib --no-default-features -- -D warnings

test-lib-fast:
    cargo test --lib --no-default-features

ci-fast: fmt-check check-provider-icons check-gpui-imports check-provider-secret-slicing check-release-panic-profile check-gnome-extension test-gnome-packaging-contracts test-packaging-scripts test-custom-provider-migration clippy-lib-fast test-lib-fast
    @true

clippy-lib:
    cargo clippy --lib -- -D warnings

test-lib:
    cargo test --lib

release-verify: ci-fast
    @echo "Core release checks passed. Run 'just release-verify-app' on a machine with the full app toolchain when app-feature validation is required."

release-verify-app: clippy-lib test-lib
    @true

check-app:
    cargo check --bin bananatray --all-features

clippy-app:
    cargo clippy --bin bananatray --all-features -- -D warnings

pre-commit:
    pre-commit run --all-files
