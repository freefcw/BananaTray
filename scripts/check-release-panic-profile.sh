#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

# panic = "unwind" (Cargo default) is a precondition for the provider-refresh panic guard:
# src/refresh/coordinator.rs uses catch_unwind to convert provider panics into
# RefreshResult::Failed and clears in-flight state. Once any profile sets panic = "abort",
# a panic terminates the whole tray process and catch_unwind never runs — and this only
# shows up in release builds, so dev tests cannot catch it.
# Hence Cargo.toml must never contain panic = "abort" (double- or single-quoted TOML string).
# Note: the grep below intentionally does not distinguish TOML values from comments —
# writing the literal panic = "abort" even inside a Cargo.toml comment will fail the
# check. This strictness is deliberate; refer to the concept in prose instead
# (e.g. "abort panic strategy") when documenting.

if grep -Eq 'panic[[:space:]]*=[[:space:]]*"abort"' Cargo.toml \
  || grep -Eq "panic[[:space:]]*=[[:space:]]*'abort'" Cargo.toml; then
  echo "error: panic = \"abort\" found in Cargo.toml" >&2
  echo "provider refresh relies on panic = \"unwind\" so catch_unwind in src/refresh/coordinator.rs" >&2
  echo "can convert provider panics to RefreshResult::Failed and clear in-flight state." >&2
  echo "remove the setting to restore the default unwind strategy (also covers the release profile)" >&2
  exit 1
fi

echo "Release panic profile check passed"
