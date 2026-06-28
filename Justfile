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

check-gpui-imports:
    ./scripts/check-gpui-imports.sh

check-provider-secret-slicing:
    ./scripts/check-provider-secret-slicing.sh

check-gnome-extension:
    ./scripts/check-gnome-extension.sh

clippy-lib-fast:
    cargo clippy --lib --no-default-features -- -D warnings

test-lib-fast:
    cargo test --lib --no-default-features

ci-fast: fmt-check check-gpui-imports check-provider-secret-slicing check-gnome-extension clippy-lib-fast test-lib-fast
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
